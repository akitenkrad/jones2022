//! `jones` — jones2022 CLI: `run` / `sweep` / `reproduce`.
//!
//! - `run` — run one cognitive-bias experiment (E1–E7) and record it as one
//!   runvault run.
//! - `sweep` — vary a parameter (E7 package count / E5 anchor ratio); the sweep
//!   is a parent run and every value is a child run.
//! - `reproduce` — classify observed vs paper anchors (`socsim-reproduce`)
//!   (offline; never panics on missing data).
//!
//! The run directory, its name and `config.json` belong to runvault; this file
//! only decides *what* is worth recording. Why the per-condition rows go to
//! `events.jsonl` rather than `metrics.csv` is argued in [`record`].
//!
//! The `LlmClient` is injected, so every path accepts either the live
//! Ollama→OpenAI fallback (with prompt cache) or, with `--mock`, a deterministic
//! scripted client for an offline smoke that exercises the whole pipeline.
//!
//! Deferred (see `.claude/CLAUDE.md`): live-model runs at scale, the container
//! sandbox, and the GPT-3 / file-deletion experiments against real models.

mod record;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use runvault::{Dataset, Lineage, Llm, Run, RunOptions};
use serde_json::{json, Map, Value};
use socsim_llm::{LlmClient, LlmSettings};
use socsim_reproduce::{build_rows, write_paper_anchors, write_reproduce_summary};

use jones2022_simulation::config::{Experiment, DEFAULT_ANCHOR_RATIO, DEFAULT_NUM_PACKAGES};
use jones2022_simulation::datasets::{load_humaneval, math_equations, math_equations_n, Problem};
use jones2022_simulation::eval::{query_count, run_experiment, run_experiment_observed, MetricRow};
use jones2022_simulation::filedelete::{
    run_file_deletion, run_file_deletion_observed, DEFAULT_TRIALS,
};
use jones2022_simulation::gpt3::{
    anchor_records, anchoring_query_count, framing_query_count, run_gpt3_anchoring,
    run_gpt3_anchoring_observed, run_gpt3_framing, run_gpt3_framing_observed,
};
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
    /// Development run: write it under results/_scratch/ so it is never synced to the vault.
    #[arg(long, global = true)]
    scratch: bool,

    /// Ollama 接続先 URL（指定時は環境変数 OLLAMA_HOST を上書きする）．
    #[arg(long, global = true)]
    ollama_host: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Run one experiment (E1–E7) → one runvault run.
    Run(RunArgs),
    /// Sweep a parameter (E7 num-packages / E5 anchor-ratio) → a sweep parent
    /// run with one child run per value.
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
    /// E3/E4 MathEquations set size. The curated 8 are a stable prefix; extras
    /// beyond 8 are deterministically generated (precedence templates).
    #[arg(long, default_value_t = 8)]
    math_count: usize,
    /// Seed for generating MathEquations problems past the curated 8 (E3/E4).
    #[arg(long, default_value_t = 42)]
    math_seed: u64,
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
    /// client instead of reading the most recent `run` run.
    #[arg(long, default_value_t = false)]
    mock: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let scratch = cli.scratch;
    if let Some(host) = cli.ollama_host.as_deref() {
        std::env::set_var("OLLAMA_HOST", host);
    }
    match cli.command {
        Commands::Run(args) => cmd_run(args, scratch),
        Commands::Sweep(args) => cmd_sweep(args, scratch),
        Commands::Reproduce(args) => cmd_reproduce(args, scratch),
    }
}

/// Build the live (cached Ollama→OpenAI) client; the model id is passed via the
/// environment that the socsim-llm harness reads.
///
/// The prompt cache is deliberately kept at the results *root*, not inside a run:
/// it is shared across runs, and a file that keeps growing after a run finished
/// has no place in that run's manifest.
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

// ─────────────────────────── 実行条件 (parameters) ─────────────────────────

/// 1 条件の実行を決める値．
///
/// `run` の引数からも `sweep` の 1 点からも作る．両方が同じ形の `parameters` を
/// 書くので，手で回した run と sweep の子 run が同じ条件なら同じ `config_hash`
/// を持つ — runvault 側でそのまま突き合わせられる．
struct Conditions {
    experiment: Experiment,
    /// 記録用のモデル id（mock のときは `"mock"`）．
    model: String,
    mock: bool,
    seed: u64,
    dataset: Option<PathBuf>,
    full: bool,
    limit: usize,
    math_count: usize,
    math_seed: u64,
    anchor_ratio: f64,
    respondents: usize,
    num_packages: usize,
    trials: usize,
}

impl Conditions {
    fn of_run(args: &RunArgs) -> Self {
        Self {
            experiment: args.experiment,
            model: model_id(args.mock, &args.model),
            mock: args.mock,
            seed: args.seed,
            dataset: args.dataset.clone(),
            full: args.full,
            limit: args.limit,
            math_count: args.math_count,
            math_seed: args.math_seed,
            anchor_ratio: args.anchor_ratio,
            respondents: args.respondents,
            num_packages: args.num_packages,
            trials: args.trials,
        }
    }

    /// sweep の 1 点．掃引していない側は既定値のままで，`parameters` にも出ない．
    fn of_sweep_point(args: &SweepArgs, num_packages: usize, anchor_ratio: f64) -> Self {
        Self {
            experiment: args.experiment,
            model: model_id(args.mock, &args.model),
            mock: args.mock,
            seed: args.seed,
            dataset: None,
            full: false,
            limit: 0,
            math_count: 8,
            math_seed: 42,
            anchor_ratio,
            respondents: DEFAULT_RESPONDENTS,
            num_packages,
            trials: args.trials,
        }
    }

    /// この実験が実際に読む条件だけを書く．
    ///
    /// 読まない旋回まで混ぜると，中身の同じ 2 つの run が別の `config_hash` を
    /// 持ってしまい，条件が分裂する．効果の無い `--sandbox` を書かないのも同じ理由
    /// （`tempdir` 以外は警告して無視される）．
    fn parameters(&self) -> Value {
        let mut p = Map::new();
        p.insert("experiment".into(), json!(self.experiment.tag()));
        p.insert("model".into(), json!(self.model));
        p.insert("mock".into(), json!(self.mock));
        p.insert("seed".into(), json!(self.seed));
        match self.experiment {
            Experiment::Framing | Experiment::Anchoring => {
                p.insert(
                    "dataset".into(),
                    json!(self.dataset.as_ref().map(|d| d.display().to_string())),
                );
                p.insert("full".into(), json!(self.full));
                p.insert("limit".into(), json!(self.limit));
            }
            Experiment::Availability | Experiment::AttributeSubstitution => {
                p.insert("math_count".into(), json!(self.math_count));
                p.insert("math_seed".into(), json!(self.math_seed));
                p.insert("limit".into(), json!(self.limit));
            }
            Experiment::Gpt3Anchoring => {
                p.insert("anchor_ratio".into(), json!(self.anchor_ratio));
            }
            Experiment::Gpt3Framing => {
                p.insert("respondents".into(), json!(self.respondents));
            }
            Experiment::FileDeletion => {
                p.insert("num_packages".into(), json!(self.num_packages));
                p.insert("trials".into(), json!(self.trials));
            }
        }
        Value::Object(p)
    }

    /// この実験が使うデータ．中身は条件だけで決まるので run の開始前に組める．
    ///
    /// 件数 (`n`) は入れない．E6/E7 では「データの行数」と「観測主体の数」が
    /// 別物（回答者数・試行数）なので，どちらを書いても片方の嘘になる．
    /// 観測主体の数は run スコープの `n_units` が持つ．
    fn dataset(&self) -> Dataset {
        match self.experiment {
            Experiment::Framing | Experiment::Anchoring => {
                Dataset::eval("humaneval").uri(self.humaneval_uri())
            }
            Experiment::Availability | Experiment::AttributeSubstitution => {
                Dataset::eval("math_equations").dataset_id(format!(
                    "jones2022/math_equations?n={}&seed={}",
                    self.math_count, self.math_seed
                ))
            }
            Experiment::Gpt3Anchoring => {
                Dataset::eval("estimation_items").dataset_id("jones2022/estimation_items")
            }
            Experiment::Gpt3Framing => {
                Dataset::eval("asian_disease").dataset_id("jones2022/asian_disease")
            }
            Experiment::FileDeletion => {
                Dataset::eval("synthetic_packages").dataset_id("jones2022/synthetic_packages")
            }
        }
    }

    fn humaneval_uri(&self) -> String {
        match &self.dataset {
            Some(p) => format!("file:{}", p.display()),
            None if self.full => "data/HumanEval.jsonl (if present)".to_string(),
            None => "data/humaneval_sample.jsonl".to_string(),
        }
    }

    /// 問い合わせ先のモデル．
    ///
    /// ライブ経路の提供元は「Ollama を先に試し，届かなければ OpenAI」という順序で
    /// あって，どちらが答えたかは実際に問うまで決まらない．run の開始時点で
    /// `"ollama"` と書けば，OpenAI が答えた run にも同じことが書かれる．
    fn llm(&self) -> Llm {
        Llm {
            provider: if self.mock {
                "mock".to_string()
            } else {
                "ollama+openai-fallback".to_string()
            },
            model_snapshot: self.model.clone(),
            temperature: Some(0.0),
            system_prompt_hash: None,
        }
    }
}

/// 記録に残すモデル id．
fn model_id(mock: bool, model: &str) -> String {
    if mock {
        "mock".to_string()
    } else {
        model.to_string()
    }
}

/// 1 条件の run を開始する．`sweep` の子 run も同じ形で始める．
fn start_run(
    results_root: &Path,
    conditions: &Conditions,
    lineage: Option<Lineage>,
    scratch: bool,
) -> Result<Run> {
    let mut options = RunOptions::new(record::EXPERIMENT, "run")
        .scratch(scratch)
        .repo_id(record::REPO_ID)
        .domain(record::DOMAIN)
        .results_root(results_root)
        .parameters(&conditions.parameters())
        .context("runvault: parameters の組み立てに失敗")?
        .seed_pointers(["/seed"])
        .master_seed(conditions.seed)
        .llm(conditions.llm())
        .data([conditions.dataset()])
        .replication(record::replication_for(conditions.experiment));
    if let Some(lineage) = lineage {
        options = options.lineage(lineage);
    }
    Run::start(options).context("runvault: run の開始に失敗")
}

// ───────────────────────────────── run ─────────────────────────────────────

fn cmd_run(args: RunArgs, scratch: bool) -> Result<()> {
    if args.sandbox != "tempdir" {
        eprintln!(
            "note: --sandbox {:?} not implemented; using the tempdir deletion guard",
            args.sandbox
        );
    }

    let conditions = Conditions::of_run(&args);
    // 実行の前に開始する．duration_sec を実測にするためであり，途中で落ちた run
    // が failed として残るためでもある．
    let mut rv = start_run(&args.results, &conditions, None, scratch)?;

    let (rows, n_units) = execute(&args, &rv)?;
    record::log_conditions(&mut rv, &rows);
    record::log_run_summary(&mut rv, &rows, n_units);

    println!(
        "experiment={} model={} n={} → {}",
        args.experiment.tag(),
        conditions.model,
        n_units,
        rv.dir().display()
    );
    print_rows(&rows);

    rv.finish().context("runvault: run の完了に失敗")?;
    Ok(())
}

/// 1 実験を回して (条件行, 観測主体の数) を返す．
fn execute(args: &RunArgs, rv: &Run) -> Result<(Vec<MetricRow>, usize)> {
    // 進捗の 1 単位は «モデルへの問い合わせ 1 回»．費用がそこにあり，E1-E4 では
    // 1 回の問い合わせにサンドボックスでのテスト実行が続く．実験を単位にすると
    // tick が 1 つしかない．
    //
    // 分母は正確に分かる．どの実験も «問題数 × パス数» や «回答者数 × フレーム数»
    // のように，走らせる前に数えられる決まった回数だけ問い合わせる．早期に
    // 打ち切る条件はどこにもない．
    //
    // stage を開くのはここ — 問題数は `load_humaneval` を通ったあとにしか
    // 分からず，`--limit` / `--full` / データセットの指定で変わる．
    Ok(match args.experiment {
        Experiment::Framing | Experiment::Anchoring => {
            let problems = load_humaneval(args.dataset.as_deref(), args.full, args.limit)?;
            // stage を開くのはクライアントを組む前．ライブ経路の `build_live` は
            // プロンプトキャッシュをディスクから読むので，そのぶんが報告の無い
            // 空白になる．先に開けば 0/N の行がその前に落ちる．
            let mut stage = rv.stage("queries", query_count(args.experiment, problems.len()));
            let client = code_client(args, &problems)?;
            let rows = run_experiment_observed(
                client.as_ref(),
                args.experiment,
                &problems,
                args.seed,
                &mut || stage.tick(),
            );
            stage.close();
            (rows, problems.len())
        }
        Experiment::Availability | Experiment::AttributeSubstitution => {
            let mut problems = math_equations_n(args.math_count, args.math_seed);
            if args.limit > 0 && problems.len() > args.limit {
                problems.truncate(args.limit);
            }
            let mut stage = rv.stage("queries", query_count(args.experiment, problems.len()));
            let client = code_client(args, &problems)?;
            let rows = run_experiment_observed(
                client.as_ref(),
                args.experiment,
                &problems,
                args.seed,
                &mut || stage.tick(),
            );
            stage.close();
            (rows, problems.len())
        }
        Experiment::Gpt3Anchoring => {
            let mut stage = rv.stage("queries", anchoring_query_count());
            let client = simple_client(args.mock, build_gpt3_anchoring_mock, args)?;
            let rows = run_gpt3_anchoring_observed(
                client.as_ref(),
                args.anchor_ratio,
                args.seed,
                &mut || stage.tick(),
            );
            stage.close();
            (rows, anchor_records(args.anchor_ratio).len())
        }
        Experiment::Gpt3Framing => {
            let mut stage = rv.stage("queries", framing_query_count(args.respondents));
            let client = simple_client(args.mock, build_gpt3_framing_mock, args)?;
            let rows = run_gpt3_framing_observed(
                client.as_ref(),
                args.respondents,
                args.seed,
                &mut || stage.tick(),
            );
            stage.close();
            (rows, args.respondents)
        }
        Experiment::FileDeletion => {
            let mut stage = rv.stage("trials", args.trials);
            let client = simple_client(args.mock, build_file_deletion_mock, args)?;
            let rows = run_file_deletion_observed(
                client.as_ref(),
                args.num_packages,
                args.trials,
                args.seed,
                &mut || stage.tick(),
            );
            stage.close();
            (rows, args.trials)
        }
    })
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

fn print_rows(rows: &[MetricRow]) {
    for r in rows {
        println!(
            "  {:<10} {:<22} acc={:>6} r={:>6} Δ={:>6}",
            r.condition,
            r.variant,
            fmt(r.functional_accuracy),
            fmt(r.indicator_rate),
            fmt(r.delta)
        );
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

/// 掃引の 1 点．値は表示と `parameters` の両方で使うので文字列も持つ．
struct SweepPoint {
    label: String,
    conditions: Conditions,
}

fn cmd_sweep(args: SweepArgs, scratch: bool) -> Result<()> {
    let (param, points, values_json): (&str, Vec<SweepPoint>, Value) = match args.experiment {
        Experiment::FileDeletion => {
            let values: Vec<usize> = parse_list(&args.num_packages_values)?;
            let points = values
                .iter()
                .map(|&n| SweepPoint {
                    label: n.to_string(),
                    conditions: Conditions::of_sweep_point(&args, n, DEFAULT_ANCHOR_RATIO),
                })
                .collect();
            ("num_packages", points, json!(values))
        }
        Experiment::Gpt3Anchoring => {
            let values: Vec<f64> = parse_list(&args.anchor_ratio_values)?;
            let points = values
                .iter()
                .map(|&p| SweepPoint {
                    label: format!("{p}"),
                    conditions: Conditions::of_sweep_point(&args, DEFAULT_NUM_PACKAGES, p),
                })
                .collect();
            ("anchor_ratio", points, json!(values))
        }
        other => {
            anyhow::bail!(
                "sweep is only defined for file-deletion (num-packages) and \
                 gpt3-anchoring (anchor-ratio); got {}",
                other.tag()
            );
        }
    };

    // 親は掃引の定義だけを持ち，条件ごとの指標は持たない．それは子 run の担当．
    // シードは全点で共通の 1 つなので，列ではなく `master_seed` として書ける．
    let parent_parameters = json!({
        "experiment": args.experiment.tag(),
        "model": model_id(args.mock, &args.model),
        "mock": args.mock,
        "seed": args.seed,
        "param": param,
        "values": values_json,
        "trials": args.trials,
    });
    let parent = Run::start(
        RunOptions::new(record::EXPERIMENT, "sweep")
            .scratch(scratch)
            .repo_id(record::REPO_ID)
            .domain(record::DOMAIN)
            .results_root(&args.results)
            .parameters(&parent_parameters)
            .context("runvault: sweep の parameters の組み立てに失敗")?
            .seed_pointers(["/seed"])
            .master_seed(args.seed)
            .llm(points[0].conditions.llm())
            .data([points[0].conditions.dataset()])
            .sweep_parent()
            .replication(record::replication_for(args.experiment)),
    )
    .context("runvault: sweep 親 run の開始に失敗")?;

    let sweep_id = parent
        .sweep_id()
        .context("runvault: sweep 親に sweep_id がありません")?
        .to_string();
    let parent_run_uid = parent.run_uid().to_string();

    println!(
        "sweep {} ({}) → {} 条件 → {}",
        args.experiment.tag(),
        param,
        points.len(),
        parent.dir().display()
    );

    // クライアントは 1 本だけ作って全点で使い回す．プロンプトキャッシュを共有し，
    // ライブ経路で条件ごとに接続を張り直さないため．
    let client: Box<dyn LlmClient> = if args.mock {
        match args.experiment {
            Experiment::FileDeletion => Box::new(build_file_deletion_mock()),
            _ => Box::new(build_gpt3_anchoring_mock()),
        }
    } else {
        build_live(&args.model, args.seed, &args.results)?
    };

    // 掃引全体で stage を 1 つ．条件ごとに開け直すと小さな 100% が並ぶだけで，
    // 掃引全体のどこにいるかは分からない．単位は run と同じ «問い合わせ 1 回»
    // (file-deletion は 1 試行 = 1 問い合わせ + サンドボックス実行)．
    //
    // 掃引しているのは `num_packages` (プロンプトに並べるパッケージ数) と
    // `anchor_ratio` (アンカーを付ける割合) で，どちらも問い合わせの «回数» を
    // 変えない — 条件ごとの回数は等しいので，重みではなく数える．
    let per_point = match args.experiment {
        Experiment::FileDeletion => args.trials,
        _ => anchoring_query_count(),
    };
    let mut stage = parent.stage("queries", points.len() * per_point);

    for point in &points {
        let mut child = start_run(
            &args.results,
            &point.conditions,
            Some(Lineage {
                sweep_id: Some(sweep_id.clone()),
                parent_run_uid: Some(parent_run_uid.clone()),
                ..Default::default()
            }),
            scratch,
        )?;

        let (rows, n_units) = match args.experiment {
            Experiment::FileDeletion => (
                run_file_deletion_observed(
                    client.as_ref(),
                    point.conditions.num_packages,
                    args.trials,
                    args.seed,
                    &mut || stage.tick(),
                ),
                args.trials,
            ),
            _ => (
                run_gpt3_anchoring_observed(
                    client.as_ref(),
                    point.conditions.anchor_ratio,
                    args.seed,
                    &mut || stage.tick(),
                ),
                anchor_records(point.conditions.anchor_ratio).len(),
            ),
        };

        record::log_conditions(&mut child, &rows);
        record::log_run_summary(&mut child, &rows, n_units);

        println!("  {param}={}", point.label);
        print_rows(&rows);

        child.finish().context("runvault: 子 run の完了に失敗")?;
    }

    // manifest.csv は finish() で封をされる．その後に 1 行足せば，manifest が
    // 食い違うダイジェストを持つことになる．
    stage.close();

    let dir = parent
        .finish()
        .context("runvault: sweep 親 run の完了に失敗")?;
    println!("掃引の定義 → {}/config.json", dir.display());
    println!("各条件の観測は子 run (subcommand=run) の events.jsonl にあります");
    Ok(())
}

// ─────────────────────────────── reproduce ─────────────────────────────────

fn cmd_reproduce(args: ReproduceArgs, scratch: bool) -> Result<()> {
    let source = if args.mock {
        None
    } else {
        latest_run(&args.results)?
    };
    let observed = match (args.mock, &source) {
        (true, _) => observe_from_rows(&run_all_mock_experiments()?),
        (false, Some(dir)) => observe_from_rows(&read_conditions(dir)?),
        (false, None) => HashMap::new(),
    };
    let source_uid = match &source {
        Some(dir) => Some(read_run_uid(dir)?),
        None => None,
    };

    // 観測をどこから採ったかは条件そのものなので parameters に書く．
    // lineage.derived_from は同じことを系譜として持つが，そちらは config_hash に
    // 入らないので，「別の run を見た reproduce」が同じ条件に見えてしまう．
    let parameters = json!({
        "mock": args.mock,
        "source_run_uid": source_uid,
    });
    let mut options = RunOptions::new(record::EXPERIMENT, "reproduce")
        .scratch(scratch)
        .repo_id(record::REPO_ID)
        // 乱数を使わない決定的な突き合わせ．simulation を名乗ると存在しない
        // master_seed を書くことになる．
        .domain(record::DOMAIN_ANALYSIS)
        .results_root(&args.results)
        .parameters(&parameters)
        .context("runvault: parameters の組み立てに失敗")?
        .replication(record::replication_all());
    if args.mock {
        // --mock は全実験を scripted client で回して観測を作る．何に問うたかは
        // 記録に残す．
        options = options.llm(Llm {
            provider: "mock".to_string(),
            model_snapshot: "mock".to_string(),
            temperature: Some(0.0),
            system_prompt_hash: None,
        });
    }
    if let Some(uid) = &source_uid {
        options = options.lineage(Lineage {
            derived_from: Some(uid.clone()),
            ..Default::default()
        });
    }
    let mut rv = Run::start(options).context("runvault: reproduce run の開始に失敗")?;

    let rows = build_rows(PAPER_ANCHORS, |a| observed.get(a.metric).copied());

    // 論文の報告値は reference.csv の担当．観測値と同じ名前で並ぶので，
    // 差は後から名前で突き合わせて取れる．
    for anchor in PAPER_ANCHORS {
        rv.log_reference(anchor.metric, anchor.paper_value)
            .scope("run")
            .target(anchor.study.to_ascii_lowercase())
            .source(format!("{} ({})", anchor.table_or_fig, anchor.condition))
            .send()
            .with_context(|| format!("{} の報告値の記録に失敗", anchor.metric))?;
    }
    // 観測できたアンカーだけを指標にする．欠測を 0 で埋めると NO_DATA が消える．
    let mut observations: Vec<(&str, f64)> = observed
        .iter()
        .map(|(name, value)| (name.as_str(), *value))
        .collect();
    observations.sort_by(|a, b| a.0.cmp(b.0));
    rv.log_metrics("run", &observations)
        .context("観測値の記録に失敗")?;

    let passes = rows.iter().filter(|r| r.status == "PASS").count();
    let offs = rows.iter().filter(|r| r.status == "off").count();
    let nodata = rows.iter().filter(|r| r.status == "NO_DATA").count();
    rv.log_metrics(
        "run",
        &[
            ("n_anchors", rows.len() as f64),
            ("n_pass", passes as f64),
            ("n_off", offs as f64),
            ("n_no_data", nodata as f64),
        ],
    )
    .context("判定の集計の記録に失敗")?;

    // 判定表そのものは artifacts に置く．許容幅つきの PASS/off/NO_DATA は
    // 指標でも報告値でもなく，1 行 1 アンカーの表なので数値の列には載らない．
    let artifacts = rv.dir().join("artifacts");
    std::fs::create_dir_all(&artifacts).context("artifacts の作成に失敗")?;
    write_paper_anchors(PAPER_ANCHORS, artifacts.join("paper_anchors.csv"))
        .context("writing paper_anchors.csv")?;
    write_reproduce_summary(&rows, artifacts.join("reproduce_summary.csv"))
        .context("writing reproduce_summary.csv")?;

    let origin = match (args.mock, &source) {
        (true, _) => "mock".to_string(),
        (false, Some(dir)) => format!("from {}", dir.display()),
        (false, None) => "no run to observe".to_string(),
    };
    println!(
        "reproduce ({origin}): {} anchors → {passes} PASS / {offs} off / {nodata} NO_DATA",
        rows.len(),
    );
    for r in &rows {
        println!(
            "  {:<4} {:<28} paper={} observed={} → {}",
            r.study, r.metric, r.paper_value, r.observed_value, r.status
        );
    }

    let dir = rv
        .finish()
        .context("runvault: reproduce run の完了に失敗")?;
    println!("判定表 → {}/artifacts/reproduce_summary.csv", dir.display());
    Ok(())
}

/// 観測元にする run — 直近に完了した `run` のうち sweep の子でないもの．
///
/// 旧実装の `results/latest` シンボリックリンクの置き換え．リンクは最後に走った
/// サブコマンドを指していたので，`reproduce` の直後には観測を持たないディレクトリ
/// を指した．ここは `subcommand` で絞るのでその穴が無い．
///
/// 1 回の `run` は 1 実験ぶんなので，ここで埋まるアンカーもその実験のものだけ．
/// これは旧実装と同じで，13 アンカーを揃えるには `--mock` を使う．
fn latest_run(results_root: &Path) -> Result<Option<PathBuf>> {
    let experiment_dir = runvault::paths::experiment_dir(results_root, record::EXPERIMENT);
    let mut best: Option<(String, PathBuf)> = None;
    for dir in runvault::paths::run_dirs(&experiment_dir)? {
        let Ok(meta) = runvault::files::read_json::<runvault::RunMeta>(&dir.join("run.json"))
        else {
            continue;
        };
        if meta.subcommand != "run" {
            continue;
        }
        let is_child = meta
            .lineage
            .as_ref()
            .and_then(|l| l.parent_run_uid.as_ref())
            .is_some();
        if is_child {
            continue;
        }
        let Ok(status) =
            runvault::files::read_json::<runvault::RunStatus>(&dir.join("status.json"))
        else {
            continue;
        };
        if status.state != runvault::State::Finished {
            continue;
        }
        if best.as_ref().is_none_or(|(at, _)| *at < status.finished_at) {
            best = Some((status.finished_at, dir));
        }
    }
    Ok(best.map(|(_, dir)| dir))
}

fn read_run_uid(run_dir: &Path) -> Result<String> {
    let meta = runvault::files::read_json::<runvault::RunMeta>(&run_dir.join("run.json"))?;
    Ok(meta.run_uid)
}

/// run の `events.jsonl` から条件行を読み戻す．
///
/// 書いたときと同じ形に戻すだけ．欠測 (キーが無い) は NaN に戻す — 集約側が
/// `is_nan` で落とすので，0 で埋めると観測しなかった条件が観測されたことになる．
fn read_conditions(run_dir: &Path) -> Result<Vec<MetricRow>> {
    let path = run_dir.join("events.jsonl");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("{} の読み取りに失敗", path.display()))?;
        if value.get("schema").and_then(Value::as_str) != Some(record::CONDITION_EVENT) {
            continue;
        }
        let text_of = |key: &str| {
            value
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let number_of = |key: &str| value.get(key).and_then(Value::as_f64).unwrap_or(f64::NAN);
        rows.push(MetricRow {
            experiment: text_of("experiment"),
            variant: text_of("variant"),
            condition: text_of("condition"),
            n: value.get("n").and_then(Value::as_u64).unwrap_or(0) as usize,
            functional_accuracy: number_of("functional_accuracy"),
            indicator_rate: number_of("indicator_rate"),
            delta: number_of("delta"),
        });
    }
    Ok(rows)
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
