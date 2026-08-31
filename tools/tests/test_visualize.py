"""Offline tests for the figure-generating tools (Agg backend, tiny fixtures)."""
from __future__ import annotations

import json
from pathlib import Path

from jones_tools import reproduce_paper, visualize, visualize_sweep
from jones_tools.run_io import output_dir

from conftest import FRAMING_ROWS, GPT3_FRAMING_ROWS


def _figures(run_dir: str) -> Path:
    return Path(output_dir(run_dir))


def _legacy_run_dir(tmp_path: Path) -> Path:
    """runvault 以前のフラットな run．ディスクに残っているので読めること．"""
    d = tmp_path / "20260101_120000"
    d.mkdir()
    (d / "metrics.csv").write_text(
        "experiment,variant,condition,n,functional_accuracy,indicator_rate,delta\n"
        "framing,-,baseline,8,1.0,NaN,0.0\n"
        "framing,raise_notimplemented,transform,8,0.25,0.75,0.75\n"
        "framing,pass,transform,8,0.375,0.625,0.625\n",
        encoding="utf-8",
    )
    (d / "config.json").write_text(
        json.dumps({"experiment": "framing", "model": "mock"}), encoding="utf-8"
    )
    return d


def test_visualize_code_experiment_writes_two_pngs(make_run):
    d = make_run("framing", FRAMING_ROWS)
    assert visualize.main(["--results-dir", d]) == 0
    figures = _figures(d)
    assert (figures / "fig_accuracy.png").exists()
    assert (figures / "fig_indicator.png").exists()
    # 図は run が終わった後に作るものなので run の中には置かない．
    assert not (Path(d) / "fig_accuracy.png").exists()


def test_visualize_gpt3_writes_indicator_png_only(make_run):
    d = make_run("gpt3-framing", GPT3_FRAMING_ROWS)
    assert visualize.main(["--results-dir", d]) == 0
    figures = _figures(d)
    # No accuracy figure (acc is NaN for E5/E6/E7), but the indicator one exists.
    assert not (figures / "fig_accuracy.png").exists()
    assert (figures / "fig_indicator.png").exists()


def test_visualize_reads_a_legacy_run(tmp_path):
    d = _legacy_run_dir(tmp_path)
    assert visualize.main(["--results-dir", str(d)]) == 0
    assert (d / "figures" / "fig_accuracy.png").exists()


def test_visualize_sweep_writes_png(make_sweep):
    rows_for = lambda n: [(f"{n}pkg", "transform", 8, None, 0.5 + 0.1 * n, None)]
    d = make_sweep("file-deletion", "num_packages", [1, 2, 3], rows_for)
    assert visualize_sweep.main(["--sweep-dir", d]) == 0
    assert (_figures(d) / "fig_sweep.png").exists()


def test_reproduce_paper_writes_png(make_run, tmp_path):
    d = _reproduce_run(make_run, tmp_path)
    assert reproduce_paper.main(["--results-dir", d]) == 0
    assert (_figures(d) / "reproduce_paper.png").exists()


def _reproduce_run(make_run, tmp_path):
    """`jones reproduce` の出力を持つ run（判定表は artifacts の下）．"""
    d = make_run("framing", FRAMING_ROWS)
    artifacts = Path(d) / "artifacts"
    artifacts.mkdir(exist_ok=True)
    (artifacts / "reproduce_summary.csv").write_text(
        "study,table_or_fig,condition,metric,paper_value,observed_value,tolerance,status\n"
        "E1,Table 1,framing,framing_verbatim_rate,0.810000,0.750000,0.150000,PASS\n"
        "E1,Table 1,framing,framing_delta,0.264000,0.675000,0.100000,off\n"
        "E7,Fig 6,ge_3_packages,file_deletion_rate,0.800000,,0.150000,NO_DATA\n",
        encoding="utf-8",
    )
    return d


def test_missing_inputs_return_nonzero(tmp_path):
    empty = tmp_path / "empty"
    empty.mkdir()
    assert visualize.main(["--results-dir", str(empty)]) == 1
    assert visualize_sweep.main(["--sweep-dir", str(empty)]) == 1
