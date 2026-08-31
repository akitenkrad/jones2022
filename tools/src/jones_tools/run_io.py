"""run ディレクトリの読み方をここ 1 箇所に集める．

置き場と同一性は runvault が持つので，どのスクリプトも `results/` を自分で
glob しない．対象の run は `runvault path` が返す．

条件ごとの観測（baseline と各変種）は `metrics.csv` ではなく `events.jsonl` の
`x.jones2022.condition` 行にある．この模型の観測には時間軸が無く，条件を
`metrics.csv` に並べると全行が同じ主キー（name, step, step_unit, scope）を
名乗ってしまうため — 理由の全文は Rust 側 `simulation/src/record.rs`．

runvault より前のレイアウト（フラットな `results/<stamp>/metrics.csv` と
`sweep_summary.csv`）はディスクにまだ残っており，書き換えない．そちらも
読めるようにしておく．
"""
from __future__ import annotations

import os

import pandas as pd
from runvault.read import (
    artifacts_dir,
    config_parameters,
    events_table,
    figures_dir,
    load_run_meta,
    runvault_path,
    sweep_children,
)

__all__ = [
    "CONDITION_EVENT",
    "EXPERIMENT",
    "conditions_table",
    "experiment_and_model",
    "is_runvault_run",
    "latest_run",
    "output_dir",
    "reproduce_summary_path",
    "sweep_table",
]

#: runvault 上の実験名（論文の E1–E7 ではない．そちらは条件）．
EXPERIMENT = "jones"
#: 条件 1 行を表す実験固有のイベント種別．
CONDITION_EVENT = "x.jones2022.condition"

#: 条件表の列．旧レイアウトの `metrics.csv` の列と同じ順にする．
CONDITION_COLUMNS = [
    "experiment",
    "variant",
    "condition",
    "n",
    "functional_accuracy",
    "indicator_rate",
    "delta",
]


def is_runvault_run(run_dir: str | os.PathLike) -> bool:
    """runvault が書いた run か（旧レイアウトなら False）．"""
    return load_run_meta(run_dir, required=False) is not None


def latest_run(
    results_root: str = "results",
    subcommand: str = "run",
    standalone: bool = True,
) -> str:
    """直近に完了した run のディレクトリ．

    `standalone` は sweep の子を除く．子は親と同じ `subcommand` で走るので，
    これを外すと «最後に走った子» が返る．
    """
    return runvault_path(
        EXPERIMENT,
        results_root=results_root,
        subcommand=subcommand,
        standalone=standalone,
    )


def conditions_table(run_dir: str | os.PathLike) -> pd.DataFrame:
    """条件 1 行の表（baseline + 変種）．

    旧レイアウトでは同じ表が `metrics.csv` にそのまま入っているので，
    そちらはそのまま読む．
    """
    if is_runvault_run(run_dir):
        # 予約列（unit_id / ts / schema / run_uid）は表示に要らない．
        df = events_table(run_dir, kind=CONDITION_EVENT).reindex(columns=CONDITION_COLUMNS)
    else:
        legacy = os.path.join(str(run_dir), "metrics.csv")
        if not os.path.exists(legacy):
            raise FileNotFoundError(f"metrics.csv not found: {legacy}")
        df = pd.read_csv(legacy)
    # 欠測は JSON では null，旧 CSV では NaN で入る．どちらも NaN に揃えないと
    # 列が object 型になり，`notna()` や `float()` の結果が経路で変わる．
    for column in ("n", "functional_accuracy", "indicator_rate", "delta"):
        df[column] = pd.to_numeric(df[column], errors="coerce")
    return df


def experiment_and_model(run_dir: str | os.PathLike) -> tuple[str, str]:
    """この run が回した実験（E1–E7）と，問い合わせたモデル．"""
    params = config_parameters(run_dir, required=False) or {}
    return str(params.get("experiment", "?")), str(params.get("model", "?"))


def output_dir(run_dir: str | os.PathLike) -> str:
    """図の置き場．run が終わった後に作るものは run の外に置く．"""
    out = figures_dir(run_dir)
    os.makedirs(out, exist_ok=True)
    return out


def reproduce_summary_path(run_dir: str | os.PathLike) -> str:
    """`reproduce` が書いた判定表．runvault の run では artifacts の下にある．"""
    if is_runvault_run(run_dir):
        return os.path.join(artifacts_dir(run_dir), "reproduce_summary.csv")
    return os.path.join(str(run_dir), "reproduce_summary.csv")


def sweep_table(sweep_dir: str | os.PathLike) -> tuple[pd.DataFrame, str, str]:
    """掃引の long 表と，(掃引パラメータ名, 実験名)．

    runvault はこの表をディスクに持たない．親の `parameters.param` が掃引した
    キーの名前で，各子の `parameters[param]` がその値なので，子の条件行に値の列を
    付けて積み直す．旧レイアウトでは `sweep_summary.csv` がそれにあたる．
    """
    if not is_runvault_run(sweep_dir):
        legacy = os.path.join(str(sweep_dir), "sweep_summary.csv")
        if not os.path.exists(legacy):
            raise FileNotFoundError(f"sweep_summary.csv not found: {legacy}")
        df = pd.read_csv(legacy)
        param = str(df["param"].iloc[0]) if "param" in df else "value"
        experiment = str(df["experiment"].iloc[0]) if "experiment" in df else "?"
        return df, param, experiment

    parameters = config_parameters(sweep_dir) or {}
    param = str(parameters.get("param", "value"))
    experiment = str(parameters.get("experiment", "?"))
    children = sweep_children(sweep_dir)
    if not children:
        raise SystemExit(
            f"error: この sweep 親に属する子 run がありません: {sweep_dir}\n"
            "  子は lineage.parent_run_uid で親を指す．親子が同じ results ルート"
            "にあるか確認すること．"
        )
    frames = []
    for child in children:
        child_parameters = config_parameters(child) or {}
        df = conditions_table(child)
        df = df[df["condition"] == "transform"].copy()
        df["param"] = param
        df["value"] = child_parameters.get(param)
        df["run_dir"] = child
        frames.append(df)
    return pd.concat(frames, ignore_index=True), param, experiment
