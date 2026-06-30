"""実行結果ディレクトリの設定値 (config.json) を表示する．

Usage:
    jones-tools show-experiment-settings [--results-dir RESULTS_DIR]
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog="jones-tools show-experiment-settings")
    parser.add_argument(
        "--results-dir",
        default="results/latest",
        help="設定を表示する結果ディレクトリ (default: results/latest)",
    )
    args = parser.parse_args(argv)

    config_path = Path(args.results_dir) / "config.json"
    if not config_path.exists():
        print(f"config.json not found in {args.results_dir}")
        return

    config = json.loads(config_path.read_text(encoding="utf-8"))
    width = max((len(k) for k in config), default=0)
    for key, value in config.items():
        print(f"{key:<{width}} : {value}")


if __name__ == "__main__":
    main()
