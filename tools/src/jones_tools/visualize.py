"""run 結果の可視化: 実験種別を自動判定し，論文 Table/Fig 風の図を出力する．

`results/<dir>/metrics.csv`（long:
experiment,variant,condition,n,functional_accuracy,indicator_rate,delta）を読み，
`config.json` の `experiment` で種別を判定して以下を生成する．

- コード実験 (E1–E4): `fig_accuracy.png`（baseline vs 変種別の機能的正解率，E1≈Table 1 /
  E4≈Table 2）と `fig_indicator.png`（失敗指標率 r と感度 Δ の 2 パネル）．
- E5 gpt3-anchoring: `fig_indicator.png`（high/low アンカー更新率 + gibberish 率）．
- E6 gpt3-framing: `fig_indicator.png`（save vs die のリスク選択率，≈Table 9）．
- E7 file-deletion: `fig_indicator.png`（誤削除率）．

Usage:
    jones-tools visualize [--results-dir RESULTS_DIR]
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path

_INDICATOR_COLOR = "#d62728"
_BASELINE_COLOR = "#4c78a8"
_TRANSFORM_COLOR = "#ff7f0e"


def _load(results_dir: Path):
    import pandas as pd

    df = pd.read_csv(results_dir / "metrics.csv")
    experiment = str(df["experiment"].iloc[0]) if not df.empty else "?"
    model = "?"
    config_path = results_dir / "config.json"
    if config_path.exists():
        cfg = json.loads(config_path.read_text(encoding="utf-8"))
        experiment = cfg.get("experiment", experiment)
        model = cfg.get("model", "?")
    return df, experiment, model


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
        help="可視化対象の結果ディレクトリ (位置引数; --results-dir と同義)",
    )
    parser.add_argument(
        "--results-dir",
        default="results/latest",
        help="可視化対象の結果ディレクトリ (default: results/latest)",
    )
    args = parser.parse_args(argv)

    results_dir = Path(args.results_dir_pos or args.results_dir)
    if not (results_dir / "metrics.csv").exists():
        print(f"metrics.csv not found in {results_dir}; run `jones run` first.")
        return 1

    import matplotlib

    matplotlib.use("Agg")

    df, experiment, model = _load(results_dir)
    transform = df[df["condition"] == "transform"]
    if transform.empty:
        print("no transform rows to plot.")
        return 1

    # コード実験 (E1–E4) は機能的正解率が非 NaN → 正解率図も描く．
    has_accuracy = bool(df["functional_accuracy"].notna().any())

    written = []
    if has_accuracy:
        out = results_dir / "fig_accuracy.png"
        _plot_accuracy(df, experiment, model, out)
        written.append(out)
    out = results_dir / "fig_indicator.png"
    _plot_indicator(df, experiment, model, out, with_delta=has_accuracy)
    written.append(out)

    for w in written:
        print(f"wrote {w}")
    cols = ["variant", "functional_accuracy", "indicator_rate", "delta"]
    print(transform[cols].to_string(index=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
