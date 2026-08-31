//! runvault への記録の共通部分．
//!
//! 論文メタデータ (research) はどのサブコマンドでも同じなので，ここ 1 箇所で
//! 組み立てる．条件 1 行の書き方と run 全体の集約指標もここに集める．

use runvault::{Replication, Run, Target, Work};
use serde::Serialize;

use jones2022_simulation::config::Experiment;
use jones2022_simulation::eval::MetricRow;
use jones2022_simulation::PAPER_ANCHORS;

/// runvault 上の実験名．`runvault path --experiment` に渡す値でもある．
///
/// 論文の E1–E7 はこちらの「実験」ではなく **条件**なので `parameters.experiment`
/// に入る．両方を実験と呼ぶと `results/` が 7 つに割れ，同じリポジトリの run を
/// まとめて引けなくなる．
pub const EXPERIMENT: &str = "jones";
/// リポジトリの安定 id．git remote の名前とは独立に固定する．
pub const REPO_ID: &str = "jones2022";
/// 変換を当てて LLM に問い，出力の失敗特徴を測る実験なので分野は `llm-safety`．
/// ABM の tick loop も乱数駆動の状態遷移も無いので `simulation` ではない．
pub const DOMAIN: &str = "llm-safety";
/// `reproduce` は観測を論文アンカーと突き合わせるだけの決定的計算なので
/// `analysis`．`simulation` を名乗ると存在しない `master_seed` を書くことになる．
pub const DOMAIN_ANALYSIS: &str = "analysis";

/// 条件 1 行を表す実験固有のイベント種別．
pub const CONDITION_EVENT: &str = "x.jones2022.condition";

/// 設計書 (Obsidian)．
const OBSIDIAN_NOTE: &str = "研究/98_論文レポート/80-再現実験/実装完了/jones2022/設計書.md";

/// この再現実験が対象としている論文．
fn work() -> Work {
    let mut work = Work::arxiv("2202.12299")
        .title("Capturing Failures of Large Language Models via Human Cognitive Biases")
        .year(2022)
        .source_version("published");
    // vault 側の同定にも使えるよう paper-id も残す (work_id は arXiv 側)．
    work.paper_id = Some("P00002059".to_string());
    work
}

/// 論文の study 記号 (`E1`…`E7`) と，そこで見る表・図．
///
/// 対応は `PAPER_ANCHORS` が持っているので，ここでは study から引くだけにする．
/// 二重に書くと片方だけ直したときに黙ってずれる．
fn target_of(study: &str, table_or_fig: &str) -> Target {
    let id = study.to_ascii_lowercase();
    if table_or_fig.starts_with("Table") {
        Target::table(id, table_or_fig)
    } else if table_or_fig.starts_with("Fig") {
        Target::figure(id, table_or_fig)
    } else {
        Target::section(id, table_or_fig)
    }
}

/// 実験 E1–E7 と論文の study 記号の対応．
pub fn study_of(experiment: Experiment) -> &'static str {
    match experiment {
        Experiment::Framing => "E1",
        Experiment::Anchoring => "E2",
        Experiment::Availability => "E3",
        Experiment::AttributeSubstitution => "E4",
        Experiment::Gpt3Anchoring => "E5",
        Experiment::Gpt3Framing => "E6",
        Experiment::FileDeletion => "E7",
    }
}

/// 1 実験の run が対象とする論文の箇所．
///
/// `run` / `sweep` は 1 つの実験しか回さないので，対象もその実験の表・図だけに
/// する．13 アンカー全部を並べると，E1 の run が Table 9 も再現したことになる．
pub fn replication_for(experiment: Experiment) -> Replication {
    let study = study_of(experiment);
    let anchor = PAPER_ANCHORS
        .iter()
        .find(|a| a.study == study)
        .unwrap_or_else(|| panic!("{study} のアンカーが PAPER_ANCHORS にありません"));
    Replication::new(work())
        .target(target_of(study, anchor.table_or_fig))
        .obsidian_note(OBSIDIAN_NOTE)
}

/// `reproduce` が対象とする論文の箇所 — 13 アンカーが載る 7 つの表・図すべて．
pub fn replication_all() -> Replication {
    let mut replication = Replication::new(work());
    let mut seen: Vec<&str> = Vec::new();
    for anchor in PAPER_ANCHORS {
        if seen.contains(&anchor.study) {
            continue;
        }
        seen.push(anchor.study);
        replication = replication.target(target_of(anchor.study, anchor.table_or_fig));
    }
    replication.obsidian_note(OBSIDIAN_NOTE)
}

// ---------------------------------------------------------------------------
// 条件行の記録
// ---------------------------------------------------------------------------

/// `events.jsonl` に書く条件 1 行．
///
/// この模型の観測には時間軸が無い．`metrics.csv` の主キーは
/// (`run_uid`, `name`, `step`, `step_unit`, `scope`) なので，条件ごとの
/// `indicator_rate` を並べると全行が同じキーを名乗り，条件どうしが衝突する．
/// かといって条件を子 run に割るのも実態と違う — 1 回の `run` は 1 つの問題集に
/// 対して baseline と各変種を **ひと続きに**測る 1 回の実行であり，別々の実行では
/// ない．`Δ = baseline − transform` という論文の主張そのものが run をまたいで
/// しまう．条件は «1 回の実行の中で観測された対象» なので，予約語 `unit_id`
/// (観測の主体) を持つイベントとして書く．
///
/// `n` は条件ごとの分母 (E5 なら回答が数値になった項目数，E7 なら実行できた試行数)
/// であって観測主体の数ではない．後者は run スコープの `n_units` が持つ．
#[derive(Serialize)]
struct ConditionEvent<'a> {
    unit_id: String,
    experiment: &'a str,
    variant: &'a str,
    condition: &'a str,
    n: usize,
    functional_accuracy: Option<f64>,
    indicator_rate: Option<f64>,
    delta: Option<f64>,
}

/// 測れなかった値 (NaN) を欠測として落とす．
///
/// JSON に NaN は無く，`null` に落ちると «0 だった» と見分けが付かなくなるので，
/// 型の上で欠測にしてから書く．
fn finite(x: f64) -> Option<f64> {
    x.is_finite().then_some(x)
}

/// 条件 1 行ごとに `events.jsonl` へ 1 行書く．
pub fn log_conditions(run: &mut Run, rows: &[MetricRow]) {
    for row in rows {
        let event = ConditionEvent {
            unit_id: format!("{}:{}", row.condition, row.variant),
            experiment: &row.experiment,
            variant: &row.variant,
            condition: &row.condition,
            n: row.n,
            functional_accuracy: finite(row.functional_accuracy),
            indicator_rate: finite(row.indicator_rate),
            delta: finite(row.delta),
        };
        run.log_event(CONDITION_EVENT, &event).unwrap_or_else(|e| {
            panic!("条件 {} の記録に失敗: {e}", event.unit_id);
        });
    }
}

/// run 全体を 1 つの値で表す指標だけを `metrics.csv` に書く．
///
/// ここに置けるのは «run に 1 つしか無い» 値に限る．条件ごとの値は events の
/// 担当で，こちらに降ろすと主キーが衝突する．欠測の行は書かない — runvault の
/// `metrics.csv` は NaN を許さないし，0 で埋めると «測れなかった» が消える．
///
/// `n_units` は予約指標名で «観測主体の数»．E1–E4 は問題数，E5 は推定項目数，
/// E6 は回答者数，E7 は試行数がそれにあたる．
pub fn log_run_summary(run: &mut Run, rows: &[MetricRow], n_units: usize) {
    let mut values: Vec<(&str, f64)> = vec![
        ("n_units", n_units as f64),
        ("n_conditions", rows.len() as f64),
    ];

    if let Some(accuracy) = rows
        .iter()
        .find(|r| r.condition == "baseline")
        .and_then(|r| finite(r.functional_accuracy))
    {
        values.push(("baseline_functional_accuracy", accuracy));
    }

    let transform = || rows.iter().filter(|r| r.condition == "transform");
    let rates: Vec<f64> = transform()
        .filter_map(|r| finite(r.indicator_rate))
        .collect();
    if !rates.is_empty() {
        // 論文アンカーが «変種のうち最大の逐語コピー率» を見るので最大値も残す．
        values.push((
            "max_indicator_rate",
            rates.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ));
        values.push(("mean_indicator_rate", socsim_metrics::stats::mean(&rates)));
    }
    let deltas: Vec<f64> = transform().filter_map(|r| finite(r.delta)).collect();
    if !deltas.is_empty() {
        values.push(("mean_delta", socsim_metrics::stats::mean(&deltas)));
    }

    run.log_metrics("run", &values)
        .expect("run スコープの指標の記録に失敗");
}

#[cfg(test)]
mod tests {
    use super::*;
    use runvault::meta::TargetKind;

    #[test]
    fn every_experiment_maps_to_an_anchored_study() {
        for experiment in Experiment::all() {
            let study = study_of(*experiment);
            assert!(
                PAPER_ANCHORS.iter().any(|a| a.study == study),
                "{} ({study}) のアンカーがありません",
                experiment.tag()
            );
            // 対象が組めることまで見る (組めなければ replication_for が落ちる)．
            let _ = replication_for(*experiment);
        }
    }

    #[test]
    fn the_study_mapping_is_a_bijection() {
        let mut studies: Vec<&str> = Experiment::all().iter().map(|e| study_of(*e)).collect();
        studies.sort_unstable();
        studies.dedup();
        assert_eq!(
            studies.len(),
            Experiment::all().len(),
            "study が重複しています"
        );
        let anchored: std::collections::BTreeSet<&str> =
            PAPER_ANCHORS.iter().map(|a| a.study).collect();
        assert_eq!(
            anchored.len(),
            studies.len(),
            "アンカー側の study 数と合いません"
        );
    }

    #[test]
    fn a_target_takes_its_kind_from_the_label() {
        assert!(matches!(target_of("E1", "Table 1").kind, TargetKind::Table));
        assert!(matches!(target_of("E2", "Fig 4").kind, TargetKind::Figure));
        assert!(matches!(
            target_of("E3", "§3.3.3").kind,
            TargetKind::Section
        ));
        assert_eq!(target_of("E7", "Fig 6").target_id, "e7");
    }

    #[test]
    fn nan_is_dropped_rather_than_written_as_zero() {
        assert_eq!(finite(0.0), Some(0.0));
        assert_eq!(finite(f64::NAN), None);
        assert_eq!(finite(f64::INFINITY), None);
    }
}
