**English** | [日本語](visualization.ja.md)

# Visualization (Python `jones-tools`)

Install at the workspace root with `uv sync`, then invoke via `uv run jones-tools <subcommand>`. Figures use a headless (Agg) backend, so they render in a sandbox/CI without a display. The CLI dispatches: `visualize`, `visualize-sweep`, `show-experiment-settings`, `reproduce-paper`. Each accepts the run directory positionally or via a flag; omit it and the run is resolved with `runvault path --latest`, which needs the `runvault` binary on `PATH` (or `RUNVAULT=<path to it>`). Figures are written **outside** the run, to `results/jones/figures/{run_slug}/` — what is made after a run ended is not part of its record.

## `visualize` — single-run figures

```bash
uv run jones-tools visualize                 # or a run directory / --results-dir DIR
```

Reads the run's `events.jsonl` (the `x.jones2022.condition` lines) and `config.json`, auto-detects the experiment, and writes:

- **`fig_accuracy.png`** (code experiments E1–E4 only) — a bar chart of functional accuracy: the `baseline` bar (blue) next to one bar per transform variant (orange). For E1 this is the Table 1 view (accuracy drop per framing line); for E4 it is the Table 2 view.
- **`fig_indicator.png`** (all experiments) — the failure-indicator rate `r` per transform variant, plus, for code experiments, a second panel for the sensitivity `Δ`. For E5 the variants are the high/low anchor-update rates and the gibberish rate; for E6 they are the risky-choice rates of the save vs die frames (the Table 9 view); for E7 it is the file-deletion rate.

It prints the per-variant accuracy / `r` / `Δ` table to the console. Experiments without functional accuracy (E5/E6/E7) write only `fig_indicator.png`.

## `visualize-sweep` — sweep figures

```bash
uv run jones-tools visualize-sweep           # or a sweep parent / --sweep-dir DIR
```

runvault keeps no sweep table on disk, so this rebuilds it: the parent's `parameters.param` names the swept key and each child's `parameters[param]` is its value, so the children's condition rows are stacked with a value column. It writes `fig_sweep.png`: the swept parameter (x-axis) against the indicator rate `r`, one line per variant, with a second `Δ` panel for code sweeps. For the E7 package-count sweep this is the Fig 6 view (deletion rate rising with package count); for the E5 anchor-ratio sweep it shows the anchoring effect strengthening with `p`. It also prints the mean `r` per parameter value.

## `reproduce-paper` — figures from the anchor table

```bash
uv run jones-tools reproduce-paper           # or a reproduce run / --summary-csv PATH / --output-dir DIR
```

Reads the `artifacts/reproduce_summary.csv` that `jones reproduce` writes and renders `reproduce_paper.png`: a per-anchor observed-vs-paper bar figure, with the paper value and the observed value side by side per metric, bars colored by `status` (PASS = green, off = orange, NO_DATA = grey with a "no data" marker and no observed bar) and a tolerance band around each paper value. It prints a `PASS / off / NO_DATA` tally. Anchors with no observation render the paper value and a "no data" marker only.

## `show-experiment-settings` — inspect a run

```bash
uv run jones-tools show-experiment-settings  # or a run directory / --results-dir DIR
```

Prints the run's identity from `run.json` (`run_uid`, subcommand, domain, the two hashes) and then the conditions from `config.json`'s `parameters`.

## Reading the figures (qualitative)

The figures are the qualitative view of the numbers in `events.jsonl` / the sweep's children / `reproduce_summary.csv`. What to look for:

| Figure | What to look for | Experiment |
|---|---|---|
| `fig_accuracy.png` | transform bars below the baseline bar (a real accuracy drop `Δ`) | E1–E4 |
| `fig_indicator.png` | a high indicator rate `r` for the biased variants; for E6, die-frame riskier than save-frame | E1–E7 |
| `fig_sweep.png` | a threshold/monotone trend in `r` across the swept parameter | E7 (packages) / E5 (ratio) |
| `reproduce_paper.png` | observed bars inside the paper-value tolerance band (PASS, green) | E1–E7 |

Numbers come from the CSVs; the figures are a convenience view. Because `--mock` is a deterministic stub, the genuine reproduction figures come from live-model runs.
