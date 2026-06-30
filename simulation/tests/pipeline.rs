//! End-to-end pipeline tests driven by the offline scripted mock client.
//!
//! These exercise the full transform → (mock) generation → sandbox → indicator →
//! metrics path over the bundled datasets, with **no live model**. The accuracy
//! assertions are guarded on a Python interpreter being present so the suite
//! still passes in a Python-less CI (the indicator path needs no Python).

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use socsim_reproduce::build_rows;
use socsim_results::write_csv;

use jones2022_simulation::config::Experiment;
use jones2022_simulation::{
    build_file_deletion_mock, build_gpt3_anchoring_mock, build_gpt3_framing_mock,
    build_mock_client, load_problems, math_equations, run_experiment, run_file_deletion,
    run_gpt3_anchoring, run_gpt3_framing, PAPER_ANCHORS,
};

fn unique_dir(tag: &str) -> PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("jones2022-test-{tag}-{}-{n}", std::process::id()))
}

#[test]
fn framing_mock_pipeline_produces_metrics_and_fires_indicator() {
    let problems = load_problems(None, 0).unwrap();
    let client = build_mock_client(&problems);
    let rows = run_experiment(&client, Experiment::Framing, &problems, 42);

    // 1 baseline + 5 framing-line transform rows.
    assert_eq!(rows.len(), 6);
    let baseline = rows.iter().find(|r| r.condition == "baseline").unwrap();
    let transforms: Vec<_> = rows.iter().filter(|r| r.condition == "transform").collect();

    // The fractional mock fires the indicator on some (not necessarily all)
    // problems: rates are in [0,1] and at least one variant is positive.
    assert!(transforms
        .iter()
        .all(|r| (0.0..=1.0).contains(&r.indicator_rate)));
    assert!(
        transforms.iter().any(|r| r.indicator_rate > 0.0),
        "the framing indicator must fire for some problems"
    );

    // Accuracy / Δ sanity — only when a Python interpreter is available.
    if !baseline.functional_accuracy.is_nan() {
        assert!(
            (baseline.functional_accuracy - 1.0).abs() < 1e-9,
            "mock returns canonical solutions ⇒ baseline accuracy 1.0, got {}",
            baseline.functional_accuracy
        );
        for r in &transforms {
            assert!(
                r.delta >= -1e-9,
                "framing should not improve accuracy (variant {}, Δ={})",
                r.variant,
                r.delta
            );
        }
    }

    // The metrics CSV is actually written.
    let dir = unique_dir("framing");
    std::fs::create_dir_all(&dir).unwrap();
    let csv = dir.join("metrics.csv");
    write_csv(&rows, &csv).unwrap();
    let text = std::fs::read_to_string(&csv).unwrap();
    assert!(text.lines().count() >= 7, "header + 6 rows");
    assert!(text.contains("raise_notimplemented"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn anchoring_mock_pipeline_emits_anchor_lines() {
    let problems = load_problems(None, 0).unwrap();
    let client = build_mock_client(&problems);
    let rows = run_experiment(&client, Experiment::Anchoring, &problems, 42);

    // 1 baseline + 3 anchor-line transform rows; the mock emits all three anchor
    // lines together, so the three per-line rates coincide and are positive.
    assert_eq!(rows.len(), 4);
    let rates: Vec<f64> = ["for_var_in", "print_var", "return_tmp"]
        .iter()
        .map(|v| {
            rows.iter()
                .find(|r| r.variant == *v)
                .unwrap()
                .indicator_rate
        })
        .collect();
    assert!(rates[0] > 0.0, "anchor indicator must fire");
    assert!(rates.iter().all(|r| (r - rates[0]).abs() < 1e-9));
}

#[test]
fn availability_and_attribute_substitution_lower_accuracy() {
    let problems = math_equations();
    let client = build_mock_client(&problems);
    for exp in [Experiment::Availability, Experiment::AttributeSubstitution] {
        let rows = run_experiment(&client, exp, &problems, 42);
        assert_eq!(rows.len(), 2, "baseline + one transform row");
        let t = rows.iter().find(|r| r.condition == "transform").unwrap();
        assert!(
            t.indicator_rate > 0.0,
            "{} distractor must appear",
            exp.tag()
        );
        let baseline = rows.iter().find(|r| r.condition == "baseline").unwrap();
        if !baseline.functional_accuracy.is_nan() {
            assert!(t.delta > 0.0, "{} must lower accuracy", exp.tag());
        }
    }
}

#[test]
fn gpt3_framing_die_frame_is_riskier_than_save() {
    let client = build_gpt3_framing_mock();
    let rows = run_gpt3_framing(&client, 40, 42);
    let save = rows.iter().find(|r| r.variant == "save_frame").unwrap();
    let die = rows.iter().find(|r| r.variant == "die_frame").unwrap();
    assert!(
        die.indicator_rate > save.indicator_rate,
        "die frame ({}) should be riskier than save frame ({})",
        die.indicator_rate,
        save.indicator_rate
    );
}

#[test]
fn gpt3_anchoring_reports_update_and_gibberish_rows() {
    let client = build_gpt3_anchoring_mock();
    let rows = run_gpt3_anchoring(&client, 0.5, 42);
    for variant in ["high_anchor", "low_anchor", "gibberish"] {
        assert!(
            rows.iter().any(|r| r.variant == variant),
            "missing {variant}"
        );
    }
    let g = rows.iter().find(|r| r.variant == "gibberish").unwrap();
    assert_eq!(g.indicator_rate, 0.0, "the numeric mock is never gibberish");
}

#[test]
fn file_deletion_guard_runs_offline_without_touching_host() {
    let client = build_file_deletion_mock();
    let rows = run_file_deletion(&client, 3, 8, 42);
    assert_eq!(rows.len(), 1);
    let r = &rows[0];
    assert_eq!(r.variant, "3pkg");
    // Rate is in [0,1] (NaN only if no Python at all).
    assert!(r.indicator_rate.is_nan() || (0.0..=1.0).contains(&r.indicator_rate));
}

#[test]
fn reproduce_classifies_anchors_offline() {
    // Synthetic observations confirm the reproduce classification path over the
    // full E1–E7 anchor set (13 anchors).
    let observed: std::collections::HashMap<&str, f64> = [
        ("framing_verbatim_rate", 0.80), // within 0.81 ± 0.15 → PASS
        ("framing_delta", 0.05),         // |0.05 − 0.264| > 0.10 → off
        ("risky_die", 0.74),             // within 0.741 ± 0.15 → PASS
    ]
    .into_iter()
    .collect();
    let anchor_rows = build_rows(PAPER_ANCHORS, |a| observed.get(a.metric).copied());
    assert_eq!(anchor_rows.len(), 13);
    let by = |m: &str| anchor_rows.iter().find(|r| r.metric == m).unwrap();
    assert_eq!(by("framing_verbatim_rate").status, "PASS");
    assert_eq!(by("framing_delta").status, "off");
    assert_eq!(by("risky_die").status, "PASS");
    assert_eq!(by("file_deletion_rate").status, "NO_DATA");
}
