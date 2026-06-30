"""fetch-dataset のオフラインテスト（ネットワークに接続しない）．

検証・冪等 skip・アトミック書き込みのロジックのみを対象とし，実ダウンロードは
``fetch()`` の skip 分岐で回避する．
"""
from __future__ import annotations

import json

import pytest

from jones_tools import cli, fetch_dataset


def _valid_jsonl(n: int = fetch_dataset.EXPECTED_COUNT) -> str:
    """n 問の妥当な HumanEval 風 JSONL を生成する．"""
    lines = []
    for i in range(n):
        lines.append(
            json.dumps(
                {
                    "task_id": f"HumanEval/{i}",
                    "prompt": f"def f{i}():\n    \"\"\"doc\"\"\"\n",
                    "canonical_solution": "    return 0\n",
                    "test": "def check(c):\n    assert c() == 0\n",
                    "entry_point": f"f{i}",
                }
            )
        )
    return "\n".join(lines) + "\n"


def test_validate_accepts_164_problems():
    problems = fetch_dataset.validate_jsonl(_valid_jsonl())
    assert len(problems) == fetch_dataset.EXPECTED_COUNT


def test_validate_rejects_wrong_count():
    with pytest.raises(ValueError, match="164"):
        fetch_dataset.validate_jsonl(_valid_jsonl(10))


def test_validate_rejects_missing_keys():
    bad = json.dumps({"task_id": "HumanEval/0"}) + "\n"
    with pytest.raises(ValueError, match="必須キー"):
        fetch_dataset.validate_jsonl(bad)


def test_validate_rejects_malformed_json():
    with pytest.raises(ValueError, match="JSON"):
        fetch_dataset.validate_jsonl("{not json}\n")


def test_existing_valid_file_skips_download(tmp_path, capsys):
    out = tmp_path / "HumanEval.jsonl"
    out.write_text(_valid_jsonl(), encoding="utf-8")
    # URL は使われない（skip 分岐に入るため）．ネットワークアクセスは発生しない．
    code = fetch_dataset.fetch("http://invalid.invalid/x.gz", out, force=False)
    assert code == 0
    assert "既存の妥当な HumanEval" in capsys.readouterr().out


def test_invalid_existing_file_is_not_skipped(tmp_path):
    out = tmp_path / "HumanEval.jsonl"
    out.write_text(_valid_jsonl(3), encoding="utf-8")  # 問題数が足りない
    assert fetch_dataset._existing_is_valid(out) is False


def test_atomic_write_creates_parent(tmp_path):
    out = tmp_path / "nested" / "data" / "HumanEval.jsonl"
    fetch_dataset._atomic_write(out, _valid_jsonl())
    assert out.exists()
    assert not out.with_name(out.name + ".tmp").exists()
    assert len(fetch_dataset.validate_jsonl(out.read_text(encoding="utf-8"))) == 164


def test_cli_help_lists_fetch_dataset(capsys):
    with pytest.raises(SystemExit) as exc:
        cli.main(["--help"])
    assert exc.value.code == 0
    assert "fetch-dataset" in capsys.readouterr().out
