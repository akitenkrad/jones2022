"""jones-tools — 統合 CLI ディスパッチャ．

run 結果の可視化 (`visualize`)，sweep 結果の可視化 (`visualize-sweep`)，実行設定の
表示 (`show-experiment-settings`)，論文アンカー再現図 (`reproduce-paper`) を
サブコマンドでディスパッチする．

Usage:
    jones-tools visualize [--results-dir RESULTS_DIR]
    jones-tools visualize-sweep [--sweep-dir SWEEP_DIR]
    jones-tools show-experiment-settings [--results-dir RESULTS_DIR]
    jones-tools reproduce-paper [--results-dir RESULTS_DIR] [--summary-csv PATH]
"""
from __future__ import annotations

import argparse
import sys

# command -> (module, help). Modules are imported lazily on dispatch.
_COMMANDS = {
    "visualize": (
        "jones_tools.visualize",
        "単一 run の可視化 (実験種別を自動判定: 正解率 / 指標率 r / Δ)",
    ),
    "visualize-sweep": (
        "jones_tools.visualize_sweep",
        "sweep の可視化 (掃引パラメータ × 指標率 r / Δ; E7 は Fig 6 風)",
    ),
    "show-experiment-settings": (
        "jones_tools.show_experiment_settings",
        "実行結果ディレクトリの設定値を表示 (config.json)",
    ),
    "reproduce-paper": (
        "jones_tools.reproduce_paper",
        "論文アンカー一括再現図 (PAPER_ANCHORS 照合・PASS/off)",
    ),
    "fetch-dataset": (
        "jones_tools.fetch_dataset",
        "公式 HumanEval (MIT) を取得し simulation/data/HumanEval.jsonl に保存",
    ),
}


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(
        prog="jones-tools",
        description="Visualization & analysis tools for jones2022",
    )
    subparsers = parser.add_subparsers(dest="command", required=True)
    for name, (_, help_text) in _COMMANDS.items():
        subparsers.add_parser(name, help=help_text, add_help=False)

    argv = sys.argv[1:] if argv is None else argv
    if not argv or argv[0] in {"-h", "--help"}:
        parser.parse_args(argv)
        return

    command = argv[0]
    rest = argv[1:]
    if command in _COMMANDS:
        module_name = _COMMANDS[command][0]
        module = __import__(module_name, fromlist=["main"])
        module.main(rest)
    else:
        parser.parse_args(argv)  # 不正なサブコマンドはここでエラーになる


if __name__ == "__main__":
    main()
