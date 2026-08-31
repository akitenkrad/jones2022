"""run の実行条件を表示する．

条件は `config.json` のエンベロープの `parameters` にある．どの run を見ているかが
分かるよう，`run.json` が持つ同一性（`run_uid` / サブコマンド / ハッシュ）も添える．

--results-dir を省略すると `runvault path` が返す直近の run を対象にする．

Usage:
    jones-tools show-experiment-settings [--results-dir RESULTS_DIR] [--results-root ROOT]
"""
from __future__ import annotations

import argparse

from runvault.read import config_parameters, load_run_meta

from jones_tools.run_io import latest_run


def _print_table(rows: list[tuple[str, object]]) -> None:
    width = max((len(k) for k, _ in rows), default=0)
    for key, value in rows:
        print(f"{key:<{width}} : {value}")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(prog="jones-tools show-experiment-settings")
    parser.add_argument(
        "results_dir_pos",
        nargs="?",
        default=None,
        help="run ディレクトリ (位置引数; --results-dir と同義)",
    )
    parser.add_argument(
        "--results-dir",
        default=None,
        help="設定を表示する run ディレクトリ (省略時は runvault path が返す直近の run)",
    )
    parser.add_argument(
        "--results-root",
        default="results",
        help="run を探す結果ルート (--results-dir 省略時に使う; default: results)",
    )
    args = parser.parse_args(argv)

    results_dir = args.results_dir_pos or args.results_dir or latest_run(args.results_root)

    parameters = config_parameters(results_dir, required=False)
    if parameters is None:
        print(f"config.json not found in {results_dir}")
        return

    meta = load_run_meta(results_dir, required=False)
    if meta is not None:
        _print_table(
            [
                ("run_uid", meta["run_uid"]),
                ("subcommand", meta["subcommand"]),
                ("domain", meta["domain"]),
                ("config_hash", meta["config_hash"][:12]),
                ("execution_hash", meta["execution_hash"][:12]),
            ]
        )
        print("--")
    _print_table(list(parameters.items()))


if __name__ == "__main__":
    main()
