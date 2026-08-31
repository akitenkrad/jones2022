"""論文アンカー一括再現の図出力．

Rust 側 `jones reproduce` が run の `artifacts/` に書く `reproduce_summary.csv` を
読む．スキーマは `socsim-reproduce` 共通:

    study,table_or_fig,condition,metric,paper_value,observed_value,tolerance,status

`status ∈ {PASS, off, NO_DATA}` で，`observed_value` は空のことがある．本スクリプトは
アンカー別の observed-vs-paper 比較図 (`reproduce_paper.png`) を生成し，PASS/off/NO_DATA
を集計表示する．色分け: PASS=緑 / off=オレンジ / NO_DATA=灰．

同じ数値は run の `reference.csv`（論文の報告値）と `metrics.csv`（観測値）にも
名前つきで入っている．こちらの表が持つのは許容幅と PASS/off/NO_DATA の判定．

--results-dir を省略すると
`runvault path --experiment jones --latest --subcommand reproduce`
が返す run を対象にする．図は run の外に書く．

Usage:
    jones-tools reproduce-paper [--results-dir RESULTS_DIR] [--summary-csv PATH]
                                [--output-dir DIR] [--results-root ROOT]
"""
from __future__ import annotations

import argparse
import os
import sys
from pathlib import Path

from jones_tools.run_io import latest_run, output_dir, reproduce_summary_path

_STATUS_COLOR = {
    "PASS": "#2ca02c",
    "off": "#ff7f0e",
    "NO_DATA": "#bbbbbb",
}


def _resolve(arg: str) -> Path:
    p = Path(arg)
    if not p.is_absolute():
        p = Path.cwd() / arg
    return Path(os.path.realpath(p))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        prog="jones-tools reproduce-paper",
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "results_dir_pos",
        nargs="?",
        default=None,
        help="結果ディレクトリ (位置引数; --results-dir と同義)",
    )
    parser.add_argument("--results-dir", "--results_dir", default=None)
    parser.add_argument("--summary-csv", "--summary_csv", default=None)
    parser.add_argument("--output-dir", "--output_dir", default=None)
    parser.add_argument(
        "--results-root",
        default="results",
        help="run を探す結果ルート (--results-dir 省略時に使う; default: results)",
    )
    args = parser.parse_args(argv)
    if args.results_dir_pos:
        args.results_dir = args.results_dir_pos

    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    import numpy as np
    import pandas as pd
    from matplotlib.patches import Patch

    if args.summary_csv:
        summary_path = _resolve(args.summary_csv)
        results_dir = None
    else:
        results_dir = args.results_dir or latest_run(
            args.results_root, subcommand="reproduce", standalone=False
        )
        summary_path = Path(reproduce_summary_path(_resolve(results_dir)))
    if not summary_path.exists():
        print(f"エラー: summary CSV がありません: {summary_path}", file=sys.stderr)
        return 1

    if args.output_dir:
        out_dir = _resolve(args.output_dir)
    elif results_dir is not None:
        out_dir = Path(output_dir(_resolve(results_dir)))
    else:
        out_dir = summary_path.parent
    out_dir.mkdir(parents=True, exist_ok=True)

    df = pd.read_csv(summary_path)
    if df.empty:
        print(f"注意: {summary_path} に行がありません．", file=sys.stderr)
        return 1

    df["paper_value"] = pd.to_numeric(df.get("paper_value"), errors="coerce")
    df["observed_value"] = pd.to_numeric(df.get("observed_value"), errors="coerce")
    df["tolerance"] = pd.to_numeric(df.get("tolerance"), errors="coerce")
    df["status"] = df.get("status", "NO_DATA").fillna("NO_DATA")
    df = df.sort_values(["study", "metric"], kind="stable").reset_index(drop=True)

    n = len(df)
    counts = df["status"].value_counts().to_dict()
    n_pass = counts.get("PASS", 0)
    n_off = counts.get("off", 0)
    n_nodata = counts.get("NO_DATA", 0)
    print("== 論文アンカー再現の集計 ==")
    print(f"  total={n}  PASS={n_pass}  off={n_off}  NO_DATA={n_nodata}")

    fig, ax = plt.subplots(figsize=(10, max(3.0, 0.6 * n + 2.0)))
    y = np.arange(n)
    bar_h = 0.38
    for i, row in df.iterrows():
        color = _STATUS_COLOR.get(str(row["status"]), _STATUS_COLOR["NO_DATA"])
        paper, obs, tol = row["paper_value"], row["observed_value"], row["tolerance"]
        if not np.isnan(paper):
            ax.barh(y[i] + bar_h / 2, paper, height=bar_h, color="#4c78a8",
                    edgecolor="black", linewidth=0.3)
            if not np.isnan(tol):
                ax.barh(y[i] + bar_h / 2, 2 * tol, left=paper - tol, height=bar_h,
                        color="none", edgecolor="#4c78a8", linewidth=0.8, linestyle=":")
        if not np.isnan(obs):
            ax.barh(y[i] - bar_h / 2, obs, height=bar_h, color=color,
                    edgecolor="black", linewidth=0.3)
        else:
            ax.text(0.0, y[i] - bar_h / 2, " no data", va="center", ha="left",
                    fontsize=8, color="#888888", style="italic")

    ax.set_yticks(y)
    ax.set_yticklabels([f"{r['study']}:{r['metric']}" for _, r in df.iterrows()], fontsize=8)
    ax.invert_yaxis()
    ax.set_xlabel("value")
    ax.set_title(f"jones2022 reproduction (PASS={n_pass} off={n_off} NO_DATA={n_nodata})")
    ax.legend(
        handles=[
            Patch(facecolor="#4c78a8", edgecolor="black", label="paper_value"),
            Patch(facecolor=_STATUS_COLOR["PASS"], edgecolor="black", label="observed (PASS)"),
            Patch(facecolor=_STATUS_COLOR["off"], edgecolor="black", label="observed (off)"),
        ],
        loc="lower right",
        fontsize=8,
    )
    fig.tight_layout()
    out = out_dir / "reproduce_paper.png"
    fig.savefig(out, dpi=120)
    plt.close(fig)
    print(f"保存: {out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
