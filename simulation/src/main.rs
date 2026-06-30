//! `jones` — jones2022 CLI: `run` / `sweep` / `reproduce`.
//!
//! - `run` — run one cognitive-bias experiment (E1–E7) and write `metrics.csv`
//!   + `config.json`.
//! - `sweep` — vary a parameter (E7 package count / E5 anchor ratio) and write
//!   `sweep_summary.csv` + `sweep_config.json` under `results/sweep_{ts}/`.
//! - `reproduce` — classify observed vs paper anchors (`socsim-reproduce`) and
//!   write `reproduce_summary.csv` (offline; never panics on missing data).
//!
//! The `LlmClient` is injected, so every path accepts either the live
//! Ollama→OpenAI fallback (with prompt cache) or, with `--mock`, a deterministic
//! scripted client for an offline smoke that exercises the whole pipeline.
//!
//! Deferred (see `.claude/CLAUDE.md`): live-model runs at scale, the container
//! sandbox, and the GPT-3 / file-deletion experiments against real models.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;
use socsim_llm::{LlmClient, LlmSettings};
use socsim_reproduce::{build_rows, write_paper_anchors, write_reproduce_summary};
use socsim_results::{
    create_run_dir, ensure_dir, refresh_latest_symlink, timestamp, write_csv, write_json,
};

use jones2022_simulation::config::{Experiment, DEFAULT_ANCHOR_RATIO, DEFAULT_NUM_PACKAGES};
use jones2022_simulation::datasets::{load_humaneval, math_equations, Problem};
use jones2022_simulation::eval::{run_experiment, MetricRow};
use jones2022_simulation::filedelete::{run_file_deletion, DEFAULT_TRIALS};
use jones2022_simulation::gpt3::{anchor_records, run_gpt3_anchoring, run_gpt3_framing};
use jones2022_simulation::{
    build_file_deletion_mock, build_gpt3_anchoring_mock, build_gpt3_framing_mock,
    build_mock_client, PAPER_ANCHORS,
};

/// Default number of E6 framing respondents.
const DEFAULT_RESPONDENTS: usize = 10;

#[derive(Parser, Debug)]
#[command(
    name = "jones",
    about = "jones2022: capturing LLM failures via human cognitive biases"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Ollama 接続先 URL（指定時は環境変数 OLLAMA_HOST を上書きする）．
    #[arg(long, global = true)]
    ollama_host: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run one experiment (E1–E7) → metrics.csv.
    Run(RunArgs),
    /// Sweep a parameter (E7 num-packages / E5 anchor-ratio) → sweep_summary.csv.
    Sweep(SweepArgs),
    /// Offline: classify observed vs PAPER_ANCHORS → reproduce_summary.csv.
    Reproduce(ReproduceArgs),
}

#[derive(clap::Args, Debug)]
struct RunArgs {
    /// Which experiment: framing|anchoring|availability|attribute-substitution|
    /// gpt3-anchoring|gpt3-framing|file-deletion (or e1..e7).
    #[arg(long, value_enum, default_value_t = Experiment::Framing)]
    experiment: Experiment,
    /// Model id for the live path (sets OLLAMA_MODEL / OPENAI_MODEL).
    #[arg(long, default_value = "codellama")]
    model: String,
    /// Generation seed.
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Use a scripted mock client instead of a live model (offline smoke).
    #[arg(long, default_value_t = false)]
    mock: bool,
    /// Results root directory.
    #[arg(long, default_value = "results")]
    results: PathBuf,
    /// HumanEval JSONL path (E1/E2; omit to use the bundled subset).
    #[arg(long)]
    dataset: Option<PathBuf>,
    /// Prefer the full HumanEval set at data/HumanEval.jsonl if present (E1/E2).
    #[arg(long, default_value_t = false)]
    full: bool,
    /// Evaluate only the first N code problems (0 = all). For scoped live smokes.
    #[arg(long, default_value_t = 0)]
    limit: usize,
    /// E5 anchor ratio p (upper = a(1+p), lower = a(1-p)).
    #[arg(long, default_value_t = DEFAULT_ANCHOR_RATIO)]
    anchor_ratio: f64,
    /// E6 number of framing respondents.
    #[arg(long, default_value_t = DEFAULT_RESPONDENTS)]
    respondents: usize,
    /// E7 number of packages to "uninstall".
    #[arg(long, default_value_t = DEFAULT_NUM_PACKAGES)]
    num_packages: usize,
    /// E7 number of deletion trials.
    #[arg(long, default_value_t = DEFAULT_TRIALS)]
    trials: usize,
    /// E7 sandbox backend: tempdir (implemented) | container (deferred).
    #[arg(long, default_value = "tempdir")]
    sandbox: String,
}

#[derive(clap::Args, Debug)]
struct SweepArgs {
    /// Which experiment to sweep (file-deletion or gpt3-anchoring are the
    /// parameterized ones).
    #[arg(long, value_enum, default_value_t = Experiment::FileDeletion)]
    experiment: Experiment,
    /// Use the scripted mock client (offline).
    #[arg(long, default_value_t = false)]
    mock: bool,
    /// Model id for the live path.
    #[arg(long, default_value = "codellama")]
    model: String,
    /// Generation seed.
    #[arg(long, default_value_t = 42)]
    seed: u64,
    /// Results root directory.
    #[arg(long, default_value = "results")]
    results: PathBuf,
    /// E7: comma-separated package counts (design §6 default 1,2,3,4,5,6).
    #[arg(long, default_value = "1,2,3,4,5,6")]
    num_packages_values: String,
    /// E5: comma-separated anchor ratios (design §6 default 0.1,0.2,0.5,0.8).
    #[arg(long, default_value = "0.1,0.2,0.5,0.8")]
    anchor_ratio_values: String,
    /// E7 number of deletion trials per package count.
    #[arg(long, default_value_t = DEFAULT_TRIALS)]
    trials: usize,
}

#[derive(clap::Args, Debug)]
struct ReproduceArgs {
    /// Results root directory to read observed metrics from (if any).
    #[arg(long, default_value = "results")]
    results: PathBuf,
    /// Generate observations offline by running every experiment with the mock
    /// client instead of reading the latest run's metrics.csv.
    #[arg(long, default_value_t = false)]
    mock: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if let Some(host) = cli.ollama_host.as_deref() {
        std::env::set_var("OLLAMA_HOST", host);
    }
    match cli.command {
        Commands::Run(args) => cmd_run(args),
        Commands::Sweep(args) => cmd_sweep(args),
        Commands::Reproduce(args) => cmd_reproduce(args),
    }
}

/// Build the live (cached Ollama→OpenAI) client; the model id is passed via the
/// environment that the socsim-llm harness reads.
fn build_live(model: &str, seed: u64, results: &Path) -> Result<Box<dyn LlmClient>> {
    std::env::set_var("OLLAMA_MODEL", model);
    std::env::set_var("OPENAI_MODEL", model);
    let settings = LlmSettings {
        temperature: 0.0,
        seed,
        cache_path: Some(
            results
                .join("llm_cache.json")
                .to_string_lossy()
                .into_owned(),
        ),
    };
    let client = socsim_llm::build_shared_live_client_from_settings(&settings)
        .map_err(|e| anyhow::anyhow!("building live LLM client: {e}"))?;
    Ok(Box::new(client))
}

fn cmd_run(args: RunArgs) -> Result<()> {
    if args.sandbox != "tempdir" {
        eprintln!(
            "note: --sandbox {:?} not implemented; using the tempdir deletion guard",
            args.sandbox
        );
    }

    let (rows, n, dataset_desc) = match args.experiment {
        Experiment::Framing | Experiment::Anchoring => {
            let problems = load_humaneval(args.dataset.as_deref(), args.full, args.limit)?;
            let client = code_client(&args, &problems)?;
            let rows = run_experiment(client.as_ref(), args.experiment, &problems, args.seed);
            (rows, problems.len(), humaneval_desc(&args))
        }
        Experiment::Availability | Experiment::AttributeSubstitution => {
            let mut problems = math_equations();
            if args.limit > 0 && problems.len() > args.limit {
                problems.truncate(args.limit);
            }
            let client = code_client(&args, &problems)?;
            let rows = run_experiment(client.as_ref(), args.experiment, &problems, args.seed);
            (rows, problems.len(), "math_equations".to_string())
        }
        Experiment::Gpt3Anchoring => {
            let client = simple_client(args.mock, build_gpt3_anchoring_mock, &args)?;
            let rows = run_gpt3_anchoring(client.as_ref(), args.anchor_ratio, args.seed);
            (
                rows,
                anchor_records(args.anchor_ratio).len(),
                "estimation_items".to_string(),
            )
        }
        Experiment::Gpt3Framing => {
            let client = simple_client(args.mock, build_gpt3_framing_mock, &args)?;
            let rows = run_gpt3_framing(client.as_ref(), args.respondents, args.seed);
            (rows, args.respondents, "asian_disease".to_string())
        }
        Experiment::FileDeletion => {
            let client = simple_client(args.mock, build_file_deletion_mock, &args)?;
            let rows =
                run_file_deletion(client.as_ref(), args.num_packages, args.trials, args.seed);
            (rows, args.trials, "synthetic_packages".to_string())
        }
    };

    let run_dir = create_run_dir(&args.results).context("creating run dir")?;
    let stamp = run_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    write_csv(&rows, run_dir.join("metrics.csv")).context("writing metrics.csv")?;

    let cfg_value = serde_json::json!({
        "experiment": args.experiment.tag(),
        "model": if args.mock { "mock" } else { args.model.as_str() },
        "mock": args.mock,
        "seed": args.seed,
        "n": n,
        "dataset": dataset_desc,
        "anchor_ratio": args.anchor_ratio,
        "num_packages": args.num_packages,
        "trials": args.trials,
        "respondents": args.respondents,
    });
    write_json(&cfg_value, run_dir.join("config.json")).context("writing config.json")?;
    refresh_latest_symlink(&args.results, &stamp).ok();

    println!(
        "experiment={} model={} n={} → {}",
        args.experiment.tag(),
        if args.mock { "mock" } else { &args.model },
        n,
        run_dir.join("metrics.csv").display()
    );
    for r in &rows {
        println!(
            "  {:<10} {:<22} acc={:>6} r={:>6} Δ={:>6}",
            r.condition,
            r.variant,
            fmt(r.functional_accuracy),
            fmt(r.indicator_rate),
            fmt(r.delta)
        );
    }
    Ok(())
}

/// Build the code-experiment client (mock keyed on the problem set, or live).
fn code_client(args: &RunArgs, problems: &[Problem]) -> Result<Box<dyn LlmClient>> {
    if args.mock {
        Ok(Box::new(build_mock_client(problems)))
    } else {
        build_live(&args.model, args.seed, &args.results)
    }
}

/// Build a no-dataset client (E5/E6/E7): the given mock builder, or live.
fn simple_client(
    mock: bool,
    build_mock: fn() -> socsim_llm::mock::ScriptedClient,
    args: &RunArgs,
) -> Result<Box<dyn LlmClient>> {
    if mock {
        Ok(Box::new(build_mock()))
    } else {
        build_live(&args.model, args.seed, &args.results)
    }
}

fn humaneval_desc(args: &RunArgs) -> String {
    match &args.dataset {
        Some(p) => format!("humaneval:{}", p.display()),
        None if args.full => "humaneval:full-if-present".to_string(),
        None => "bundled:humaneval_sample.jsonl".to_string(),
    }
}

/// Format a possibly-NaN metric for the console (`"n/a"` when undetermined).
fn fmt(x: f64) -> String {
    if x.is_nan() {
        "n/a".to_string()
    } else {
        format!("{x:.3}")
    }
}

// ───────────────────────────────── sweep ───────────────────────────────────

/// One sweep summary row: an underlying metric row annotated with the swept
/// parameter and its value.
#[derive(Debug, Serialize)]
struct SweepRow {
    experiment: String,
    param: String,
    value: String,
    variant: String,
    functional_accuracy: f64,
    indicator_rate: f64,
    delta: f64,
}

fn sweep_rows(metric_rows: &[MetricRow], param: &str, value: String) -> Vec<SweepRow> {
    metric_rows
        .iter()
        .filter(|r| r.condition == "transform")
        .map(|r| SweepRow {
            experiment: r.experiment.clone(),
            param: param.to_string(),
            value: value.clone(),
            variant: r.variant.clone(),
            functional_accuracy: r.functional_accuracy,
            indicator_rate: r.indicator_rate,
            delta: r.delta,
        })
        .collect()
}

fn parse_list<T: std::str::FromStr>(s: &str) -> Result<Vec<T>>
where
    T::Err: std::fmt::Display,
{
    s.split(',')
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(|t| {
            t.parse::<T>()
                .map_err(|e| anyhow::anyhow!("parsing {t:?}: {e}"))
        })
        .collect()
}

fn cmd_sweep(args: SweepArgs) -> Result<()> {
    let (param, mut summary, values_json): (&str, Vec<SweepRow>, serde_json::Value) =
        match args.experiment {
            Experiment::FileDeletion => {
                let values: Vec<usize> = parse_list(&args.num_packages_values)?;
                let client: Box<dyn LlmClient> = if args.mock {
                    Box::new(build_file_deletion_mock())
                } else {
                    build_live(&args.model, args.seed, &args.results)?
                };
                let mut rows = Vec::new();
                for &n in &values {
                    let m = run_file_deletion(client.as_ref(), n, args.trials, args.seed);
                    rows.extend(sweep_rows(&m, "num_packages", n.to_string()));
                }
                ("num_packages", rows, serde_json::json!(values))
            }
            Experiment::Gpt3Anchoring => {
                let values: Vec<f64> = parse_list(&args.anchor_ratio_values)?;
                let client: Box<dyn LlmClient> = if args.mock {
                    Box::new(build_gpt3_anchoring_mock())
                } else {
                    build_live(&args.model, args.seed, &args.results)?
                };
                let mut rows = Vec::new();
                for &p in &values {
                    let m = run_gpt3_anchoring(client.as_ref(), p, args.seed);
                    rows.extend(sweep_rows(&m, "anchor_ratio", format!("{p}")));
                }
                ("anchor_ratio", rows, serde_json::json!(values))
            }
            other => {
                anyhow::bail!(
                    "sweep is only defined for file-deletion (num-packages) and \
                     gpt3-anchoring (anchor-ratio); got {}",
                    other.tag()
                );
            }
        };
    summary.sort_by(|a, b| {
        (a.value.clone(), a.variant.clone()).cmp(&(b.value.clone(), b.variant.clone()))
    });

    let ts = timestamp();
    let sweep_dir = args.results.join(format!("sweep_{ts}"));
    ensure_dir(&sweep_dir).context("creating sweep dir")?;
    write_csv(&summary, sweep_dir.join("sweep_summary.csv"))
        .context("writing sweep_summary.csv")?;
    let cfg = serde_json::json!({
        "experiment": args.experiment.tag(),
        "param": param,
        "values": values_json,
        "model": if args.mock { "mock" } else { args.model.as_str() },
        "mock": args.mock,
        "seed": args.seed,
        "trials": args.trials,
    });
    write_json(&cfg, sweep_dir.join("sweep_config.json")).context("writing sweep_config.json")?;

    println!(
        "sweep {} ({}) → {} rows → {}",
        args.experiment.tag(),
        param,
        summary.len(),
        sweep_dir.join("sweep_summary.csv").display()
    );
    for r in &summary {
        println!(
            "  {}={:<6} {:<12} r={}",
            r.param,
            r.value,
            r.variant,
            fmt(r.indicator_rate)
        );
    }
    Ok(())
}

// ─────────────────────────────── reproduce ─────────────────────────────────

fn cmd_reproduce(args: ReproduceArgs) -> Result<()> {
    let observed = if args.mock {
        observe_from_rows(&run_all_mock_experiments()?)
    } else {
        observe_from_csv(&args.results.join("latest").join("metrics.csv"))
    };

    let run_dir = create_run_dir(&args.results).context("creating reproduce run dir")?;
    let stamp = run_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let rows = build_rows(PAPER_ANCHORS, |a| observed.get(a.metric).copied());
    write_paper_anchors(PAPER_ANCHORS, run_dir.join("paper_anchors.csv"))
        .context("writing paper_anchors.csv")?;
    write_reproduce_summary(&rows, run_dir.join("reproduce_summary.csv"))
        .context("writing reproduce_summary.csv")?;
    refresh_latest_symlink(&args.results, &stamp).ok();

    let passes = rows.iter().filter(|r| r.status == "PASS").count();
    let offs = rows.iter().filter(|r| r.status == "off").count();
    let nodata = rows.iter().filter(|r| r.status == "NO_DATA").count();
    println!(
        "reproduce ({}): {} anchors → {passes} PASS / {offs} off / {nodata} NO_DATA → {}",
        if args.mock { "mock" } else { "from latest" },
        rows.len(),
        run_dir.join("reproduce_summary.csv").display()
    );
    for r in &rows {
        println!(
            "  {:<4} {:<28} paper={} observed={} → {}",
            r.study, r.metric, r.paper_value, r.observed_value, r.status
        );
    }
    Ok(())
}

/// Run every experiment once with the mock client and collect all metric rows.
fn run_all_mock_experiments() -> Result<Vec<MetricRow>> {
    let mut rows = Vec::new();
    let humaneval = load_humaneval(None, false, 0)?;
    let code = build_mock_client(&humaneval);
    rows.extend(run_experiment(&code, Experiment::Framing, &humaneval, 42));
    rows.extend(run_experiment(&code, Experiment::Anchoring, &humaneval, 42));

    let math = math_equations();
    let mcode = build_mock_client(&math);
    rows.extend(run_experiment(&mcode, Experiment::Availability, &math, 42));
    rows.extend(run_experiment(
        &mcode,
        Experiment::AttributeSubstitution,
        &math,
        42,
    ));

    rows.extend(run_gpt3_anchoring(
        &build_gpt3_anchoring_mock(),
        DEFAULT_ANCHOR_RATIO,
        42,
    ));
    rows.extend(run_gpt3_framing(
        &build_gpt3_framing_mock(),
        DEFAULT_RESPONDENTS,
        42,
    ));
    rows.extend(run_file_deletion(
        &build_file_deletion_mock(),
        DEFAULT_NUM_PACKAGES,
        DEFAULT_TRIALS,
        42,
    ));
    Ok(rows)
}

/// Aggregate the anchor-metric observations from a set of metric rows.
fn observe_from_rows(rows: &[MetricRow]) -> HashMap<String, f64> {
    let mut out = HashMap::new();

    let find = |exp: &str, variant: &str| -> Option<&MetricRow> {
        rows.iter()
            .find(|r| r.experiment == exp && r.condition == "transform" && r.variant == variant)
    };
    // Indicator / delta of a (experiment, variant) transform row, if non-NaN.
    let ind = |exp: &str, variant: &str| {
        find(exp, variant)
            .map(|r| r.indicator_rate)
            .filter(|x| !x.is_nan())
    };
    let del =
        |exp: &str, variant: &str| find(exp, variant).map(|r| r.delta).filter(|x| !x.is_nan());

    // E1 framing aggregates (max verbatim rate, mean Δ).
    let framing: Vec<&MetricRow> = rows
        .iter()
        .filter(|r| r.experiment == "framing" && r.condition == "transform")
        .collect();
    if let Some(max_r) = framing
        .iter()
        .map(|r| r.indicator_rate)
        .filter(|x| !x.is_nan())
        .fold(None, |acc: Option<f64>, x| {
            Some(acc.map_or(x, |m| m.max(x)))
        })
    {
        out.insert("framing_verbatim_rate".to_string(), max_r);
    }
    let fdeltas: Vec<f64> = framing
        .iter()
        .map(|r| r.delta)
        .filter(|x| !x.is_nan())
        .collect();
    if !fdeltas.is_empty() {
        out.insert(
            "framing_delta".to_string(),
            socsim_metrics::stats::mean(&fdeltas),
        );
    }

    // The per-experiment anchor metric → (experiment, variant) observation.
    let pairs: &[(&str, Option<f64>)] = &[
        ("anchor_forvar_rate", ind("anchoring", "for_var_in")),
        ("anchor_printvar_rate", ind("anchoring", "print_var")),
        (
            "availability_unary_first_rate",
            ind("availability", "order_flip"),
        ),
        ("availability_delta", del("availability", "order_flip")),
        (
            "attribsub_named_func_rate",
            ind("attribute-substitution", "conflicting_name"),
        ),
        (
            "attribsub_delta",
            del("attribute-substitution", "conflicting_name"),
        ),
        ("anchor_update_high", ind("gpt3-anchoring", "high_anchor")),
        ("gibberish_rate", ind("gpt3-anchoring", "gibberish")),
        ("risky_save", ind("gpt3-framing", "save_frame")),
        ("risky_die", ind("gpt3-framing", "die_frame")),
        (
            "file_deletion_rate",
            ind("file-deletion", &file_deletion_variant(rows)),
        ),
    ];
    for (key, value) in pairs {
        if let Some(v) = value {
            out.insert(key.to_string(), *v);
        }
    }
    out
}

/// The variant of the first file-deletion transform row (e.g. `"3pkg"`), so the
/// E7 observation closure can address it without hard-coding the package count.
fn file_deletion_variant(rows: &[MetricRow]) -> String {
    rows.iter()
        .find(|r| r.experiment == "file-deletion" && r.condition == "transform")
        .map(|r| r.variant.clone())
        .unwrap_or_default()
}

/// Parse `metrics.csv` (one experiment) into metric rows, then aggregate.
/// Missing file ⇒ empty (every anchor NO_DATA).
fn observe_from_csv(path: &Path) -> HashMap<String, f64> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let mut rows = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 {
            continue; // header
        }
        let c: Vec<&str> = line.split(',').collect();
        if c.len() < 7 {
            continue;
        }
        rows.push(MetricRow {
            experiment: c[0].to_string(),
            variant: c[1].to_string(),
            condition: c[2].to_string(),
            n: c[3].parse().unwrap_or(0),
            functional_accuracy: c[4].parse().unwrap_or(f64::NAN),
            indicator_rate: c[5].parse().unwrap_or(f64::NAN),
            delta: c[6].parse().unwrap_or(f64::NAN),
        });
    }
    observe_from_rows(&rows)
}
