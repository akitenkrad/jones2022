//! The jones2022 evaluation pipeline: for one experiment, run the **baseline**
//! (identity) and **transformed** passes over the HumanEval subset, score
//! functional accuracy in the sandbox, evaluate the failure indicator `φ`, and
//! aggregate into long-format metric rows.
//!
//! Mirrors gong2026's `eval` shape: the [`LlmClient`](socsim_llm::LlmClient) is
//! injected as `&dyn LlmClient`, so the whole pipeline runs in tests and under
//! `--mock` with a `ScriptedClient` — no live model.
//!
//! Metrics (design §4.3(6)):
//! - `functional_accuracy` = mean over problems of `1[pass(M(p))]`.
//! - `delta` (sensitivity `Δ`) = `acc(baseline) − acc(transform)`.
//! - `indicator_rate` (`r`) = mean over transformed outputs of `1[φ(output)]`.

use serde::Serialize;
use socsim_llm::{LlmClient, LlmConfig};

use crate::config::{Experiment, ANCHOR_LINES, FRAMING_LINES};
use crate::datasets::Problem;
use crate::indicators::{anchor_hit, distractor_hit, framing_hit};
use crate::sandbox::{assemble_program, run_tests, ExecOutcome, DEFAULT_TIMEOUT};
use crate::transforms;

/// Token budget for a code completion (a function body).
const COMPLETION_MAX_TOKENS: u32 = 512;

/// Code-completion system prompt (greedy / temperature 0, matching the paper's
/// deterministic decoding).
const SYSTEM_PROMPT: &str = "You are an expert Python programmer. Complete the \
final function. Reply with ONLY the function body as indented Python code (no \
signature, no markdown fences, no commentary).";

/// One long-format metrics row: experiment × variant × condition.
#[derive(Debug, Clone, Serialize)]
pub struct MetricRow {
    /// Experiment tag (`"framing"` / `"anchoring"`).
    pub experiment: String,
    /// Variant slug: `"-"` for the baseline row; a framing-line slug (E1) or an
    /// anchor-line slug (E2) for transform rows.
    pub variant: String,
    /// `"baseline"` (identity transform) or `"transform"`.
    pub condition: String,
    /// Number of problems evaluated.
    pub n: usize,
    /// Functional accuracy (NaN when undetermined, e.g. no Python interpreter).
    pub functional_accuracy: f64,
    /// Indicator rate `r` (NaN on the baseline row, where `φ` is not applicable).
    pub indicator_rate: f64,
    /// Sensitivity `Δ` = baseline_acc − transform_acc (0 on baseline; NaN when
    /// accuracy is undetermined).
    pub delta: f64,
}

/// Query the model for a single completion under the deterministic config.
fn query(client: &dyn LlmClient, prompt: &str, seed: u64) -> String {
    let cfg = LlmConfig::deterministic()
        .with_system(SYSTEM_PROMPT)
        .with_max_tokens(COMPLETION_MAX_TOKENS)
        .with_seed(seed);
    match client.complete(prompt, &cfg) {
        Ok(resp) => resp.text,
        Err(e) => {
            eprintln!("LLM completion failed: {e}");
            String::new()
        }
    }
}

/// Mean of the determined pass/fail outcomes (1.0 = pass), or NaN when none are
/// determined (e.g. Python unavailable).
fn accuracy(outcomes: &[ExecOutcome]) -> f64 {
    let determined: Vec<f64> = outcomes
        .iter()
        .filter_map(|o| o.passed().map(|p| if p { 1.0 } else { 0.0 }))
        .collect();
    if determined.is_empty() {
        f64::NAN
    } else {
        socsim_metrics::stats::mean(&determined)
    }
}

/// Mean of a boolean indicator vector (NaN when empty).
fn rate(hits: &[bool]) -> f64 {
    if hits.is_empty() {
        return f64::NAN;
    }
    let v: Vec<f64> = hits.iter().map(|&b| if b { 1.0 } else { 0.0 }).collect();
    socsim_metrics::stats::mean(&v)
}

/// `Δ` = baseline − transform, propagating NaN when either is undetermined.
fn delta(baseline_acc: f64, transform_acc: f64) -> f64 {
    if baseline_acc.is_nan() || transform_acc.is_nan() {
        f64::NAN
    } else {
        baseline_acc - transform_acc
    }
}

/// Run one experiment end-to-end over `problems`, returning the metric rows
/// (one baseline row + one transform row per variant).
pub fn run_experiment(
    client: &dyn LlmClient,
    experiment: Experiment,
    problems: &[Problem],
    seed: u64,
) -> Vec<MetricRow> {
    run_experiment_observed(client, experiment, problems, seed, &mut || {})
}

/// How many model queries [`run_experiment`] will make over `n_problems`.
///
/// One baseline pass over the problems, then one pass per transform variant:
/// five framing lines for E1, and a single transform for E2/E3/E4 (E2's several
/// anchor lines are scored on the *same* completions, not re-queried). Known
/// before the run, so the stage that counts them carries a share and an
/// estimate rather than a bare tally.
pub fn query_count(experiment: Experiment, n_problems: usize) -> usize {
    let passes = match experiment {
        Experiment::Framing => 1 + FRAMING_LINES.len(),
        Experiment::Anchoring | Experiment::Availability | Experiment::AttributeSubstitution => 2,
        // Not this code path; `execute` routes those elsewhere.
        Experiment::Gpt3Anchoring | Experiment::Gpt3Framing | Experiment::FileDeletion => 0,
    };
    passes * n_problems
}

/// The same, calling `on_query` once for every model query.
///
/// The callback is where a caller counts its progress. One query is the unit
/// because it is the unit the cost is in: each is a model call followed by the
/// sandboxed test run that scores it. The experiment would otherwise be a
/// single tick.
pub fn run_experiment_observed(
    client: &dyn LlmClient,
    experiment: Experiment,
    problems: &[Problem],
    seed: u64,
    on_query: &mut dyn FnMut(),
) -> Vec<MetricRow> {
    // ── Baseline (identity) pass ───────────────────────────────────────────
    let baseline_outcomes: Vec<ExecOutcome> = problems
        .iter()
        .map(|p| {
            on_query();
            let completion = query(client, &transforms::identity(&p.prompt), seed);
            run_tests(
                &assemble_program(&p.prompt, &completion, &p.test, &p.entry_point),
                DEFAULT_TIMEOUT,
            )
        })
        .collect();
    let baseline_acc = accuracy(&baseline_outcomes);

    let mut rows = vec![MetricRow {
        experiment: experiment.tag().to_string(),
        variant: "-".to_string(),
        condition: "baseline".to_string(),
        n: problems.len(),
        functional_accuracy: baseline_acc,
        indicator_rate: f64::NAN,
        delta: 0.0,
    }];

    // ── Transformed pass ───────────────────────────────────────────────────
    match experiment {
        Experiment::Framing => {
            // One transform per framing line; φ = verbatim copy of that line.
            for line in FRAMING_LINES {
                let mut outcomes = Vec::with_capacity(problems.len());
                let mut hits = Vec::with_capacity(problems.len());
                for p in problems {
                    let transformed = transforms::framing(&p.prompt, line);
                    on_query();
                    let completion = query(client, &transformed, seed);
                    hits.push(framing_hit(&completion, line));
                    outcomes.push(run_tests(
                        &assemble_program(&transformed, &completion, &p.test, &p.entry_point),
                        DEFAULT_TIMEOUT,
                    ));
                }
                let acc_t = accuracy(&outcomes);
                rows.push(MetricRow {
                    experiment: experiment.tag().to_string(),
                    variant: line.slug.to_string(),
                    condition: "transform".to_string(),
                    n: problems.len(),
                    functional_accuracy: acc_t,
                    indicator_rate: rate(&hits),
                    delta: delta(baseline_acc, acc_t),
                });
            }
        }
        Experiment::Anchoring => {
            // A single anchor transform; φ measured per anchor line on the same
            // completions.
            let mut outcomes = Vec::with_capacity(problems.len());
            let mut completions = Vec::with_capacity(problems.len());
            for p in problems {
                let transformed = transforms::anchoring(&p.prompt);
                on_query();
                let completion = query(client, &transformed, seed);
                outcomes.push(run_tests(
                    &assemble_program(&transformed, &completion, &p.test, &p.entry_point),
                    DEFAULT_TIMEOUT,
                ));
                completions.push(completion);
            }
            let acc_t = accuracy(&outcomes);
            let d = delta(baseline_acc, acc_t);
            for (slug, code) in ANCHOR_LINES {
                let hits: Vec<bool> = completions.iter().map(|c| anchor_hit(c, code)).collect();
                rows.push(MetricRow {
                    experiment: experiment.tag().to_string(),
                    variant: slug.to_string(),
                    condition: "transform".to_string(),
                    n: problems.len(),
                    functional_accuracy: acc_t,
                    indicator_rate: rate(&hits),
                    delta: d,
                });
            }
        }
        // E3 / E4 (MathEquations): a single prompt-prefix transform; φ = the
        // problem's distractor token appears in the output.
        Experiment::Availability | Experiment::AttributeSubstitution => {
            let (variant, transform): (&str, fn(&str) -> String) = match experiment {
                Experiment::Availability => ("order_flip", transforms::order_flip),
                _ => ("conflicting_name", transforms::conflicting_name),
            };
            let mut outcomes = Vec::with_capacity(problems.len());
            let mut hits = Vec::with_capacity(problems.len());
            for p in problems {
                let transformed = transform(&p.prompt);
                on_query();
                let completion = query(client, &transformed, seed);
                hits.push(distractor_hit(&completion, p.distractor_token.as_deref()));
                outcomes.push(run_tests(
                    &assemble_program(&transformed, &completion, &p.test, &p.entry_point),
                    DEFAULT_TIMEOUT,
                ));
            }
            let acc_t = accuracy(&outcomes);
            rows.push(MetricRow {
                experiment: experiment.tag().to_string(),
                variant: variant.to_string(),
                condition: "transform".to_string(),
                n: problems.len(),
                functional_accuracy: acc_t,
                indicator_rate: rate(&hits),
                delta: delta(baseline_acc, acc_t),
            });
        }
        // E5/E6/E7 are direct-answer / sandboxed tasks handled by `gpt3` /
        // `filedelete`, never routed through this code-execution path.
        Experiment::Gpt3Anchoring | Experiment::Gpt3Framing | Experiment::FileDeletion => {
            eprintln!(
                "internal: {} is not a code-execution experiment; use the gpt3/filedelete path",
                experiment.tag()
            );
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use socsim_llm::mock::ScriptedClient;

    fn problems() -> Vec<Problem> {
        vec![Problem {
            task_id: "T/0".to_string(),
            prompt: "def add(x, y):\n    \"\"\"add\"\"\"\n".to_string(),
            canonical_solution: "    return x + y\n".to_string(),
            test: "def check(candidate):\n    assert candidate(2, 3) == 5\n".to_string(),
            entry_point: "add".to_string(),
            distractor_solution: None,
            distractor_token: None,
        }]
    }

    #[test]
    fn rate_and_delta_helpers() {
        assert!((rate(&[true, false, true, true]) - 0.75).abs() < 1e-12);
        assert!(rate(&[]).is_nan());
        assert!(delta(1.0, 0.25) - 0.75 < 1e-12);
        assert!(delta(f64::NAN, 0.5).is_nan());
    }

    #[test]
    fn framing_indicator_fires_on_scripted_copy() {
        // A scripted "model" that always copies `raise NotImplementedError`.
        let client = ScriptedClient::constant("mock", "    raise NotImplementedError\n");
        let rows = run_experiment(&client, Experiment::Framing, &problems(), 42);
        // One baseline + five framing-line transform rows.
        assert_eq!(rows.len(), 6);
        let rni = rows
            .iter()
            .find(|r| r.variant == "raise_notimplemented")
            .unwrap();
        assert_eq!(rni.indicator_rate, 1.0, "verbatim copy ⇒ r = 1.0");
    }
}
