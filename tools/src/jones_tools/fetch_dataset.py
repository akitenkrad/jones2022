"""公式 HumanEval データセットを取得し ``simulation/data/HumanEval.jsonl`` に保存する．

公開・ダウンロード可能なのは **HumanEval** のみ（OpenAI ``openai/human-eval``,
ライセンス MIT）．もう一方の MathEquations は著者独自・非公開のためコード合成
（``datasets::math_equations()``）のままで，ここでは扱わない．

保存先 ``simulation/data/HumanEval.jsonl`` は Rust ローダの ``full_humaneval_path()``
が自動検出するパスなので，取得後は ``jones run --experiment framing --full`` で
そのままフル 164 問に切り替わる（ローダ無改修）．

設計方針:
- **offline-first**: ``run`` から暗黙取得はしない．本コマンドだけが明示的に取得する．
- **冪等**: 既存ファイルが妥当（164 問が ``Problem`` としてパース）なら skip．``--force`` で再取得．
- **アトミック書き込み**: 一時ファイル→rename．中断しても壊れた本体を残さない．
- **provenance**: URL / ライセンス / バイト数 / SHA-256 / 問題数を必ず表示（黙って間引かない）．

Usage:
    jones-tools fetch-dataset [--output PATH] [--url URL] [--force]
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
import sys
import urllib.error
import urllib.request
from pathlib import Path

# 公式 HumanEval の配布物（gzip 圧縮 JSONL）．
DEFAULT_URL = (
    "https://raw.githubusercontent.com/openai/human-eval/master/data/HumanEval.jsonl.gz"
)
LICENSE = "MIT (openai/human-eval)"

# 期待される問題数とスキーマ．取得物の健全性検証に使う．
EXPECTED_COUNT = 164
REQUIRED_KEYS = {"task_id", "prompt", "canonical_solution", "test", "entry_point"}

# 取得物（圧縮 .gz バイト列）の固定 SHA-256．再現性のためピン留めする
# （openai/human-eval master の HumanEval.jsonl.gz, 2026-06 時点で検証）．
# None にすると構造検証（問題数 + スキーマ）のみで通す．
EXPECTED_SHA256: str | None = (
    "b796127e635a67f93fb35c04f4cb03cf06f38c8072ee7cee8833d7bee06979ef"
)


def _repo_root() -> Path:
    """jones2022 リポジトリのルートを返す．

    本ファイルは ``tools/src/jones_tools/fetch_dataset.py`` にあるため，4 階層上が
    リポジトリルート（``simulation/`` を含む）．
    """
    return Path(__file__).resolve().parents[3]


def _default_output() -> Path:
    return _repo_root() / "simulation" / "data" / "HumanEval.jsonl"


def validate_jsonl(text: str) -> list[dict]:
    """JSONL テキストを HumanEval として検証し，パース済みレコード列を返す．

    不正な場合は ``ValueError`` を送出する（部分的に書き込まない / 黙って通さない）．
    """
    problems: list[dict] = []
    for i, line in enumerate(text.splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            obj = json.loads(line)
        except json.JSONDecodeError as exc:
            raise ValueError(f"{i} 行目が JSON として不正です: {exc}") from exc
        missing = REQUIRED_KEYS - obj.keys()
        if missing:
            raise ValueError(
                f"{i} 行目に必須キーがありません: {sorted(missing)} "
                f"(task_id={obj.get('task_id', '?')})"
            )
        problems.append(obj)
    if len(problems) != EXPECTED_COUNT:
        raise ValueError(
            f"HumanEval は {EXPECTED_COUNT} 問のはずですが {len(problems)} 問でした"
        )
    return problems


def _existing_is_valid(output: Path) -> bool:
    if not output.exists():
        return False
    try:
        validate_jsonl(output.read_text(encoding="utf-8"))
        return True
    except (OSError, ValueError):
        return False


def _atomic_write(output: Path, text: str) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    tmp = output.with_name(output.name + ".tmp")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, output)


def fetch(url: str, output: Path, *, force: bool) -> int:
    """``url`` から HumanEval を取得し ``output`` に保存する．戻り値はプロセス終了コード．"""
    if not force and _existing_is_valid(output):
        print(
            f"既存の妥当な HumanEval を検出: {output} "
            f"({EXPECTED_COUNT} 問)．再取得するには --force．"
        )
        return 0

    print(f"fetch: {url}")
    print(f"license: {LICENSE}")
    try:
        req = urllib.request.Request(url, headers={"User-Agent": "jones2022-fetch-dataset"})
        with urllib.request.urlopen(req, timeout=60) as resp:
            compressed = resp.read()
    except (urllib.error.URLError, urllib.error.HTTPError, TimeoutError) as exc:
        print(f"error: ダウンロードに失敗しました: {exc}", file=sys.stderr)
        return 1

    sha = hashlib.sha256(compressed).hexdigest()
    print(f"downloaded: {len(compressed)} bytes  sha256={sha}")
    if EXPECTED_SHA256 is not None and sha != EXPECTED_SHA256:
        print(
            f"error: SHA-256 不一致．expected={EXPECTED_SHA256} got={sha}",
            file=sys.stderr,
        )
        return 1

    try:
        text = gzip.decompress(compressed).decode("utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        print(f"error: gunzip / decode に失敗しました: {exc}", file=sys.stderr)
        return 1

    try:
        problems = validate_jsonl(text)
    except ValueError as exc:
        print(f"error: 取得データの検証に失敗しました: {exc}", file=sys.stderr)
        return 1

    _atomic_write(output, text)
    print(f"saved: {output} ({len(problems)} 問)")
    print("次のように使えます: cargo run -p jones2022-simulation -- run --experiment framing --full")
    return 0


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(
        prog="jones-tools fetch-dataset",
        description="公式 HumanEval (MIT) を取得し simulation/data/HumanEval.jsonl に保存する",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="保存先 JSONL パス (default: <repo>/simulation/data/HumanEval.jsonl)",
    )
    parser.add_argument(
        "--url",
        default=DEFAULT_URL,
        help=f"取得元 URL (default: {DEFAULT_URL})",
    )
    parser.add_argument(
        "--force",
        action="store_true",
        help="既存ファイルが妥当でも再取得する",
    )
    args = parser.parse_args(argv)

    output = args.output if args.output is not None else _default_output()
    sys.exit(fetch(args.url, output, force=args.force))


if __name__ == "__main__":
    main()
