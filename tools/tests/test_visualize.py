"""Offline tests for the figure-generating tools (Agg backend, tiny fixtures)."""
from __future__ import annotations

import json
from pathlib import Path

from jones_tools import reproduce_paper, visualize, visualize_sweep


def _write(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")


def _code_run_dir(tmp_path: Path) -> Path:
    d = tmp_path / "run_framing"
    d.mkdir()
    _write(
        d / "metrics.csv",
        "experiment,variant,condition,n,functional_accuracy,indicator_rate,delta\n"
        "framing,-,baseline,8,1.0,NaN,0.0\n"
        "framing,raise_notimplemented,transform,8,0.25,0.75,0.75\n"
        "framing,pass,transform,8,0.375,0.625,0.625\n",
    )
    _write(d / "config.json", json.dumps({"experiment": "framing", "model": "mock"}))
    return d


def _gpt3_run_dir(tmp_path: Path) -> Path:
    d = tmp_path / "run_framing_gpt3"
    d.mkdir()
    _write(
        d / "metrics.csv",
        "experiment,variant,condition,n,functional_accuracy,indicator_rate,delta\n"
        "gpt3-framing,save_frame,transform,10,NaN,0.3,NaN\n"
        "gpt3-framing,die_frame,transform,10,NaN,0.6,NaN\n",
    )
    _write(d / "config.json", json.dumps({"experiment": "gpt3-framing", "model": "mock"}))
    return d


def _sweep_dir(tmp_path: Path) -> Path:
    d = tmp_path / "sweep_e7"
    d.mkdir()
    _write(
        d / "sweep_summary.csv",
        "experiment,param,value,variant,functional_accuracy,indicator_rate,delta\n"
        "file-deletion,num_packages,1,1pkg,NaN,0.5,NaN\n"
        "file-deletion,num_packages,2,2pkg,NaN,0.625,NaN\n"
        "file-deletion,num_packages,3,3pkg,NaN,0.875,NaN\n",
    )
    _write(d / "sweep_config.json", json.dumps({"experiment": "file-deletion", "param": "num_packages"}))
    return d


def _reproduce_dir(tmp_path: Path) -> Path:
    d = tmp_path / "reproduce"
    d.mkdir()
    _write(
        d / "reproduce_summary.csv",
        "study,table_or_fig,condition,metric,paper_value,observed_value,tolerance,status\n"
        "E1,Table 1,framing,framing_verbatim_rate,0.810000,0.750000,0.150000,PASS\n"
        "E1,Table 1,framing,framing_delta,0.264000,0.675000,0.100000,off\n"
        "E7,Fig 6,ge_3_packages,file_deletion_rate,0.800000,,0.150000,NO_DATA\n",
    )
    return d


def test_visualize_code_experiment_writes_two_pngs(tmp_path):
    d = _code_run_dir(tmp_path)
    assert visualize.main(["--results-dir", str(d)]) == 0
    assert (d / "fig_accuracy.png").exists()
    assert (d / "fig_indicator.png").exists()


def test_visualize_gpt3_writes_indicator_png_only(tmp_path):
    d = _gpt3_run_dir(tmp_path)
    assert visualize.main(["--results-dir", str(d)]) == 0
    # No accuracy figure (acc is NaN for E5/E6/E7), but the indicator one exists.
    assert not (d / "fig_accuracy.png").exists()
    assert (d / "fig_indicator.png").exists()


def test_visualize_sweep_writes_png(tmp_path):
    d = _sweep_dir(tmp_path)
    assert visualize_sweep.main(["--sweep-dir", str(d)]) == 0
    assert (d / "fig_sweep.png").exists()


def test_reproduce_paper_writes_png(tmp_path):
    d = _reproduce_dir(tmp_path)
    assert reproduce_paper.main(["--results-dir", str(d)]) == 0
    assert (d / "reproduce_paper.png").exists()


def test_missing_inputs_return_nonzero(tmp_path):
    empty = tmp_path / "empty"
    empty.mkdir()
    assert visualize.main(["--results-dir", str(empty)]) == 1
    assert visualize_sweep.main(["--sweep-dir", str(empty)]) == 1
