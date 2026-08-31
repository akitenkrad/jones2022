"""sweep 結果の可視化: 掃引パラメータ vs 失敗指標率 r（と Δ）．

`jones sweep` は掃引の定義だけを持つ **親 run** と，値ごとの **子 run** を書く．
掃引の表はディスクに無いので，親の `parameters.param`（掃引したキー）と各子の
`parameters[param]`（その値）で子の条件行を積み直して組む．掃引パラメータ
（E7: num_packages / E5: anchor_ratio）を x 軸に，変種ごとの指標率 r を折れ線で
描く（E7 のパッケージ数閾値挙動は Fig 6 風）．コード実験を掃引した場合は Δ
パネルも併せて描く．

## 出力

```
<results>/jones/figures/<sweep の run_slug>/fig_sweep.png
```

--sweep-dir を省略すると
`runvault path --experiment jones --latest --subcommand sweep`
が返す親 run を対象にする．

Usage:
    jones-tools visualize-sweep [--sweep-dir SWEEP_DIR] [--results-root ROOT]
"""
from __future__ import annotations

import argparse
from pathlib import Path

from jones_tools.run_io import latest_run, output_dir, sweep_table


def _numeric_or_str(v):
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
        help="sweep 親 run のディレクトリ (位置引数; --sweep-dir と同義)",
    )
    parser.add_argument(
        "--sweep-dir",
        "--sweep_dir",
        default=None,
        help="sweep 親 run のディレクトリ (省略時は runvault path が返す直近の sweep)",
    )
    parser.add_argument(
        "--results-root",
        default="results",
        help="sweep を探す結果ルート (--sweep-dir 省略時に使う; default: results)",
    )
    args = parser.parse_args(argv)

    sweep_dir = (
        args.sweep_dir_pos
        or args.sweep_dir
        or latest_run(args.results_root, subcommand="sweep", standalone=False)
    )
    try:
        df, param, experiment = sweep_table(sweep_dir)
    except (FileNotFoundError, SystemExit) as e:
        print(f"{e}\n  `jones sweep` を先に実行すること．")
        return 1
    if df.empty:
        print("掃引に行がありません．")
        return 1

    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

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
    out = Path(output_dir(sweep_dir)) / "fig_sweep.png"
    fig.savefig(out, dpi=120)
    plt.close(fig)
    print(f"wrote {out}")

    # 要約: param 値ごとの平均 r．
    summary = df.groupby("value")["indicator_rate"].mean()
    print(summary.to_string())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
