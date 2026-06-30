"""sweep 結果の可視化: 掃引パラメータ vs 失敗指標率 r（と Δ）．

Rust 側 `jones sweep` が `results/sweep_{ts}/sweep_summary.csv`（long:
experiment,param,value,variant,functional_accuracy,indicator_rate,delta）と
`sweep_config.json` を書き出す．本スクリプトは掃引パラメータ（E7: num_packages /
E5: anchor_ratio）を x 軸に，変種ごとの指標率 r を折れ線で描く（E7 のパッケージ数
閾値挙動は Fig 6 風）．コード実験を掃引した場合は Δ パネルも併せて描く．

## 出力

```
{sweep_dir}/fig_sweep.png   ← param 値 × 変種別 r（必要なら Δ）
```

Usage:
    jones-tools visualize-sweep [--sweep-dir results/latest] [--summary-csv PATH]
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path


def _numeric_or_str(v: str):
    try:
        return float(v)
    except (TypeError, ValueError):
        return v


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="jones-tools visualize-sweep")
    parser.add_argument(
        "sweep_dir_pos",
        nargs="?",
        default=None,
        help="sweep 結果ディレクトリ (位置引数; --sweep-dir と同義)",
    )
    parser.add_argument(
        "--sweep-dir",
        "--sweep_dir",
        default="results/latest",
        help="sweep 結果ディレクトリ (default: results/latest)",
    )
    parser.add_argument("--summary-csv", "--summary_csv", default=None)
    args = parser.parse_args(argv)

    sweep_dir = Path(args.sweep_dir_pos or args.sweep_dir)
    summary_path = Path(args.summary_csv) if args.summary_csv else sweep_dir / "sweep_summary.csv"
    if not summary_path.exists():
        print(f"sweep_summary.csv not found at {summary_path}; run `jones sweep` first.")
        return 1

    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import pandas as pd

    df = pd.read_csv(summary_path)
    if df.empty:
        print("sweep_summary.csv has no rows.")
        return 1

    param = str(df["param"].iloc[0]) if "param" in df else "value"
    experiment = str(df["experiment"].iloc[0]) if "experiment" in df else "?"
    cfg_path = sweep_dir / "sweep_config.json"
    if cfg_path.exists():
        cfg = json.loads(cfg_path.read_text(encoding="utf-8"))
        param = cfg.get("param", param)
        experiment = cfg.get("experiment", experiment)

    # x 軸（数値なら昇順ソート）．
    df["_x"] = df["value"].map(_numeric_or_str)
    numeric_x = df["_x"].map(lambda v: isinstance(v, float)).all()
    if numeric_x:
        df = df.sort_values("_x")

    has_delta = bool(df["delta"].notna().any())
    n_panels = 2 if has_delta else 1
    fig, axes = plt.subplots(1, n_panels, figsize=(6 * n_panels, 4.2), squeeze=False)

    ax = axes[0][0]
    for variant, sub in df.groupby("variant"):
        ax.plot(sub["_x"], sub["indicator_rate"], marker="o", label=str(variant))
    ax.set_xlabel(param)
    ax.set_ylabel("indicator rate r")
    ax.set_ylim(0, 1.05)
    ax.set_title(f"jones2022 {experiment} sweep — r vs {param}")
    ax.legend(fontsize=8)
    ax.grid(True, linewidth=0.3, alpha=0.5)

    if has_delta:
        axd = axes[0][1]
        for variant, sub in df.groupby("variant"):
            axd.plot(sub["_x"], sub["delta"], marker="s", label=str(variant))
        axd.set_xlabel(param)
        axd.set_ylabel("sensitivity Δ")
        axd.set_title(f"{experiment} sweep — Δ vs {param}")
        axd.legend(fontsize=8)
        axd.grid(True, linewidth=0.3, alpha=0.5)

    fig.tight_layout()
    out = summary_path.parent / "fig_sweep.png"
    fig.savefig(out, dpi=120)
    plt.close(fig)
    print(f"wrote {out}")

    # 要約: param 値ごとの平均 r．
    summary = df.groupby("value")["indicator_rate"].mean()
    print(summary.to_string())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
