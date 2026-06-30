"""jones-tools CLI のスモークテスト（オフライン・importable 検証）．"""
from __future__ import annotations

import json

import pytest

from jones_tools import __version__, cli
from jones_tools import (
    reproduce_paper,
    show_experiment_settings,
    visualize,
    visualize_sweep,
)


def test_modules_import_and_expose_main():
    assert __version__
    for mod in (cli, visualize, visualize_sweep, show_experiment_settings, reproduce_paper):
        assert callable(mod.main)


def test_cli_help_lists_subcommands(capsys):
    # argparse `--help` prints usage and exits 0; that is success, not an error.
    with pytest.raises(SystemExit) as exc:
        cli.main(["--help"])
    assert exc.value.code == 0
    out = capsys.readouterr().out
    assert "visualize" in out
    assert "visualize-sweep" in out
    assert "reproduce-paper" in out


def test_show_experiment_settings_reads_config(tmp_path, capsys):
    cfg = {"experiment": "framing", "model": "mock", "mock": True, "seed": 42}
    (tmp_path / "config.json").write_text(json.dumps(cfg), encoding="utf-8")
    show_experiment_settings.main(["--results-dir", str(tmp_path)])
    out = capsys.readouterr().out
    assert "framing" in out
    assert "experiment" in out
