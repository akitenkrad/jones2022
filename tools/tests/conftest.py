"""図系テストの土台: 本物の runvault run をその場で作る．

`runvault` の Python 実装（第 2 実装）で書くので，フィクスチャは手書きの JSON
ではなく実際に出るファイルそのものになる．形式が変わればテストが落ちる．
"""
from __future__ import annotations

import pytest
from runvault.run import Run

CONDITION_EVENT = "x.jones2022.condition"

#: (variant, condition, n, functional_accuracy, indicator_rate, delta)
FRAMING_ROWS = [
    ("-", "baseline", 8, 1.0, None, 0.0),
    ("raise_notimplemented", "transform", 8, 0.25, 0.75, 0.75),
    ("pass", "transform", 8, 0.375, 0.625, 0.625),
]
#: E5–E7 は機能的正解率も Δ も測れない（欠測は行に書かず null で入る）．
GPT3_FRAMING_ROWS = [
    ("save_frame", "transform", 10, None, 0.3, None),
    ("die_frame", "transform", 10, None, 0.6, None),
]


def _write_conditions(run, experiment, rows):
    for variant, condition, n, accuracy, rate, delta in rows:
        run.log_event(
            CONDITION_EVENT,
            {
                "unit_id": f"{condition}:{variant}",
                "experiment": experiment,
                "variant": variant,
                "condition": condition,
                "n": n,
                "functional_accuracy": accuracy,
                "indicator_rate": rate,
                "delta": delta,
            },
        )
    run.log_metrics("run", {"n_units": float(rows[0][2])})


def _start(results_root, subcommand, parameters, **options):
    return Run.start(
        "jones",
        subcommand,
        repo_id="jones2022",
        # フィクスチャなので llm ブロックを要求しない domain にしておく．
        domain="analysis",
        results_root=results_root,
        parameters=parameters,
        **options,
    )


@pytest.fixture
def results_root(tmp_path):
    return tmp_path / "results"


@pytest.fixture
def make_run(results_root):
    """1 実験ぶんの run を書いて，そのディレクトリを返す．"""

    def _make(experiment, rows, parameters=None):
        run = _start(
            results_root,
            "run",
            {"experiment": experiment, "model": "mock", **(parameters or {})},
        )
        _write_conditions(run, experiment, rows)
        run.finish()
        return str(run.dir)

    return _make


@pytest.fixture
def make_sweep(results_root):
    """sweep 親 1 つと，値ごとの子 run を書いて親のディレクトリを返す．"""

    def _make(experiment, param, values, rows_for):
        parent = _start(
            results_root,
            "sweep",
            {"experiment": experiment, "model": "mock", "param": param, "values": values},
            sweep_parent=True,
        )
        lineage = {"sweep_id": parent.sweep_id, "parent_run_uid": parent.run_uid}
        for value in values:
            child = _start(
                results_root,
                "run",
                {"experiment": experiment, "model": "mock", param: value},
                lineage=lineage,
            )
            _write_conditions(child, experiment, rows_for(value))
            child.finish()
        parent.finish()
        return str(parent.dir)

    return _make
