"""run 結果の可視化: 実験種別を自動判定し，論文 Table/Fig 風の図を出力する．

条件ごとの観測（baseline と各変種）は run の `events.jsonl` から読む．実験種別は
`config.json` の `parameters.experiment` で判定して以下を生成する．

- コード実験 (E1–E4): `fig_accuracy.png`（baseline vs 変種別の機能的正解率，E1≈Table 1 /
  E4≈Table 2）と `fig_indicator.png`（失敗指標率 r と感度 Δ の 2 パネル）．
- E5 gpt3-anchoring: `fig_indicator.png`（high/low アンカー更新率 + gibberish 率）．
- E6 gpt3-framing: `fig_indicator.png`（save vs die のリスク選択率，≈Table 9）．
- E7 file-deletion: `fig_indicator.png`（誤削除率）．

図は run の外（`<results>/jones/figures/<run_slug>/`）に書く．run が終わった後に
作るものは，その run の manifest に載らないので中には置かない．

--results-dir を省略すると
`runvault path --experiment jones --latest --subcommand run --standalone`
が返す run ディレクトリを対象にする（`runvault` が PATH にある必要がある）．

Usage:
    jones-tools visualize [--results-dir RESULTS_DIR] [--results-root ROOT]
"""
from __future__ import annotations

import argparse
from pathlib import Path

from jones_tools.run_io import conditions_table, experiment_and_model, latest_run, output_dir

_INDICATOR_COLOR = "#d62728"
_BASELINE_COLOR = "#4c78a8"
_TRANSFORM_COLOR = "#ff7f0e"


def _plot_accuracy(df, experiment: str, model: str, out: Path) -> None:
    """baseline vs 変種別の機能的正解率（コード実験のみ）．"""
    import matplotlib.pyplot as plt

    baseline = df[df["condition"] == "baseline"]
    transform = df[df["condition"] == "transform"]
    labels, values, colors = [], [], []
    if not baseline.empty:
        labels.append("baseline")
        values.append(float(baseline["functional_accuracy"].iloc[0]))
        colors.append(_BASELINE_COLOR)
    for _, row in transform.iterrows():
        labels.append(str(row["variant"]))
        values.append(float(row["functional_accuracy"]))
        colors.append(_TRANSFORM_COLOR)

    fig, ax = plt.subplots(figsize=(max(5, 1.1 * len(labels)), 4))
    ax.bar(labels, values, color=colors, edgecolor="black", linewidth=0.3)
    ax.set_ylim(0, 1.05)
    ax.set_ylabel("functional accuracy")
    ax.set_title(f"jones2022 {experiment} — functional accuracy ({model})")
    ax.tick_params(axis="x", labelrotation=30)
    ax.axhline(0.0, color="grey", linewidth=0.5)
    fig.tight_layout()
    fig.savefig(out, dpi=120)
    plt.close(fig)


def _plot_indicator(df, experiment: str, model: str, out: Path, with_delta: bool) -> None:
    """失敗指標率 r（と，コード実験では Δ）を変種別に描く．"""
    import matplotlib.pyplot as plt

    transform = df[df["condition"] == "transform"]
    variants = [str(v) for v in transform["variant"]]
    r = [float(x) for x in transform["indicator_rate"]]

    n_panels = 2 if with_delta else 1
    fig, axes = plt.subplots(1, n_panels, figsize=(5.5 * n_panels, 4), squeeze=False)
    ax = axes[0][0]
    ax.bar(variants, r, color=_INDICATOR_COLOR, edgecolor="black", linewidth=0.3)
    ax.set_ylim(0, 1.05)
    ax.set_ylabel("indicator rate r")
    ax.set_title(f"{experiment} — failure indicator r ({model})")
    ax.tick_params(axis="x", labelrotation=30)

    if with_delta:
        axd = axes[0][1]
        d = [float(x) for x in transform["delta"]]
        axd.bar(variants, d, color="#555555", edgecolor="black", linewidth=0.3)
        axd.set_ylabel("sensitivity Δ")
        axd.set_title(f"{experiment} — accuracy drop Δ")
        axd.tick_params(axis="x", labelrotation=30)
        axd.axhline(0.0, color="grey", linewidth=0.5, linestyle="--")

    fig.tight_layout()
    fig.savefig(out, dpi=120)
    plt.close(fig)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="jones-tools visualize")
    parser.add_argument(
        "results_dir_pos",
        nargs="?",
        default=None,
        help="可視化対象の run ディレクトリ (位置引数; --results-dir と同義)",
    )
    parser.add_argument(
        "--results-dir",
        default=None,
        help="可視化対象の run ディレクトリ (省略時は runvault path が返す直近の run)",
    )
    parser.add_argument(
        "--results-root",
        default="results",
        help="run を探す結果ルート (--results-dir 省略時に使う; default: results)",
    )
    args = parser.parse_args(argv)

    results_dir = args.results_dir_pos or args.results_dir or latest_run(args.results_root)
    try:
        df = conditions_table(results_dir)
    except (FileNotFoundError, SystemExit) as e:
        print(f"{e}\n  `jones run` を先に実行すること．")
        return 1

    import matplotlib

    matplotlib.use("Agg")

    experiment, model = experiment_and_model(results_dir)
    if experiment == "?" and not df.empty:
        experiment = str(df["experiment"].iloc[0])
    transform = df[df["condition"] == "transform"]
    if transform.empty:
        print("no transform rows to plot.")
        return 1

    # コード実験 (E1–E4) は機能的正解率が非 NaN → 正解率図も描く．
    has_accuracy = bool(df["functional_accuracy"].notna().any())

    out_dir = Path(output_dir(results_dir))
    written = []
    if has_accuracy:
        out = out_dir / "fig_accuracy.png"
        _plot_accuracy(df, experiment, model, out)
        written.append(out)
    out = out_dir / "fig_indicator.png"
    _plot_indicator(df, experiment, model, out, with_delta=has_accuracy)
    written.append(out)

    for w in written:
        print(f"wrote {w}")
    cols = ["variant", "functional_accuracy", "indicator_rate", "delta"]
    print(transform[cols].to_string(index=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
