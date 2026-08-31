**English** | [日本語](cli.ja.md)

# CLI

Two command-line surfaces: the Rust binary `jones` (the probe pipeline) and the Python `jones-tools` (visualization).

## Rust `jones`

Build with `cargo build --release`; the binary is `target/release/jones`. Three subcommands: `run`, `sweep`, `reproduce` — all always compiled, no feature flags. Every subcommand records one [runvault](https://github.com/akitenkrad/rs-runvault) run: runvault creates the directory, names it and writes `config.json`, so nothing here composes a results path by hand. The whole method is **black-box and logprob-free**: it never requests token probabilities, only the generated text. `--mock` runs the whole pipeline offline with a deterministic scripted client; without it, generation goes to the live **Ollama→OpenAI fallback** with a prompt cache.

### Local Ollama setup

```bash
# 1. Install and start Ollama (https://ollama.com), then pull a code model:
ollama pull codellama        # or qwen2.5-coder / deepseek-coder / starcoder2

# 2. Ollama listens on http://localhost:11434 by default (override via --ollama-host
#    or the OLLAMA_HOST env var). The OpenAI fallback (e.g. gpt-4o-mini) is used when
#    Ollama is unreachable; set OPENAI_API_KEY (and optionally OPENAI_MODEL) for it.
```

The live model id is passed by `--model`, which the harness reads via `OLLAMA_MODEL` / `OPENAI_MODEL`. Generation is greedy (`temperature = 0`) with the seed pinned, and completions are cached at `<results>/llm_cache.json` so a re-run replays identical text.

### `run` — one experiment (E1–E7)

Runs a single cognitive-bias experiment, scores it, and records it as one run.

```bash
# E1 framing over HumanEval (offline scripted mock)
cargo run --release -- run --experiment framing --mock --seed 42

# E7 file deletion, 3 packages, live model
cargo run --release -- run --experiment file-deletion --model codellama --num-packages 3
```

| Flag | Default | Meaning |
|---|---|---|
| `--experiment <E>` | `framing` | `framing` \| `anchoring` \| `availability` \| `attribute-substitution` \| `gpt3-anchoring` \| `gpt3-framing` \| `file-deletion` (aliases `e1`…`e7`) |
| `--model <MODEL>` | `codellama` | live model id (Ollama code model; OpenAI fallback) |
| `--seed <SEED>` | `42` | generation seed |
| `--mock` | `false` | use a deterministic scripted client instead of a live model (offline) |
| `--results <DIR>` | `results` | results root directory |
| `--dataset <PATH>` | — | HumanEval JSONL path (E1/E2); omit to use the bundled subset |
| `--full` | `false` | prefer the full set at `data/HumanEval.jsonl` if present (E1/E2) |
| `--limit <N>` | `0` | evaluate only the first N code problems (0 = all); for scoped live smokes |
| `--math-count <N>` | `8` | E3/E4 MathEquations set size; the curated 8 are a stable prefix, extras are generated |
| `--math-seed <S>` | `42` | seed for generating MathEquations problems past the curated 8 (E3/E4) |
| `--anchor-ratio <p>` | `0.5` | E5 anchor ratio (high = a(1+p), low = a(1−p)) |
| `--respondents <N>` | `10` | E6 number of framing respondents |
| `--num-packages <N>` | `3` | E7 number of packages to "uninstall" |
| `--trials <N>` | `8` | E7 number of deletion trials |
| `--sandbox <tempdir\|container>` | `tempdir` | E7 sandbox backend (`tempdir` implemented; `container` reserved) |
| `--ollama-host <URL>` | — | override `OLLAMA_HOST` (global flag) |

Outputs (under `results/jones/{run_slug}/`):

- `events.jsonl` — one `x.jones2022.condition` line per condition, carrying `variant, condition, n, functional_accuracy, indicator_rate, delta`. For code experiments (E1–E4) a `baseline` line carries functional accuracy and the `transform` lines carry accuracy + indicator + `Δ`; for E5/E6/E7 accuracy is absent and the signal is the indicator rate.
- `metrics.csv` — only what the run has *one* of: `n_units`, `n_conditions`, `baseline_functional_accuracy`, `max_indicator_rate`, `mean_indicator_rate`, `mean_delta`, all at `scope=run` with no step.
- `config.json` / `run.json` — the conditions the experiment actually reads, plus the paper, the dataset and the model.

**Why the conditions are events, not metrics.** The per-condition rows are not a time series. `metrics.csv` is keyed on `(name, step, step_unit, scope)`, so five `indicator_rate` rows with no step would all claim the same key. Nor are the conditions separate executions — one `run` measures baseline and every variant over one problem set in one pass, and `Δ = baseline − transform` is a *within-run* comparison — so splitting them into child runs would assert executions that never happened. They are observations made inside one execution, which is what `events.jsonl` and its reserved `unit_id` are for.

#### MathEquations — the generated set (E3/E4)

Unlike HumanEval, the paper's MathEquations set is the authors' own and is not public, so the replication **synthesizes** it. `--math-count`/`--math-seed` scale it deterministically: the curated 8 problems are a stable prefix, and any extras are generated from operator-precedence templates (e.g. `(x + y) * k` vs the naïve `x + y * k`). Each generated problem keeps the dataset's invariants — its `distractor_token` occurs only in the wrong body, and its unit test asserts the *canonical* value on inputs where the two readings disagree, so a biased completion provably fails. The set size and seed are recorded in `config.json` (`math_count` / `math_seed`) and the set is identified in `run.json` (`data[].dataset_id`). `--math-count 8` (the default) reproduces the original curated set unchanged.

```bash
cargo run -p jones-simulation -- run --experiment availability --math-count 90 --math-seed 7
```

### `sweep` — vary a parameter

Runs an experiment across a parameter grid. Two experiments are parameterized: **E7** over the package count and **E5** over the anchor ratio. Unlike the conditions inside a `run`, each swept value *is* a separate execution of the model, so the sweep is a **parent run** holding the grid definition and each value is a **child run** (`subcommand=run`, `lineage.parent_run_uid` pointing at the parent). A child of a given condition carries the same `config_hash` as the same condition started by hand.

```bash
cargo run --release -- sweep --experiment file-deletion  --num-packages-values 1,2,3,4,5,6
cargo run --release -- sweep --experiment gpt3-anchoring --anchor-ratio-values 0.1,0.2,0.5,0.8
```

| Flag | Default | Meaning |
|---|---|---|
| `--experiment <E>` | `file-deletion` | `file-deletion` (num-packages) or `gpt3-anchoring` (anchor-ratio) |
| `--mock` | `false` | use the scripted client (offline) |
| `--model <MODEL>` | `codellama` | live model id |
| `--seed <SEED>` | `42` | generation seed |
| `--results <DIR>` | `results` | results root directory |
| `--num-packages-values <list>` | `1,2,3,4,5,6` | E7 package counts (comma-separated) |
| `--anchor-ratio-values <list>` | `0.1,0.2,0.5,0.8` | E5 anchor ratios (comma-separated) |
| `--trials <N>` | `8` | E7 deletion trials per package count |

Outputs: the parent's `config.json` holds `param` and `values`; every condition's observations are in its child run's `events.jsonl`. There is no `sweep_summary.csv` on disk — `jones-tools visualize-sweep` rebuilds that table from the children.

### `reproduce` — offline paper-anchor check

Joins the built-in `PAPER_ANCHORS` (the paper's E1–E7 reference values) against observed metrics and classifies each anchor.

```bash
# Offline: run every experiment with the mock client, then classify
cargo run --release -- reproduce --mock

# From the most recent finished `run` run (that experiment's anchors only)
cargo run --release -- reproduce
```

| Flag | Default | Meaning |
|---|---|---|
| `--results <DIR>` | `results` | results root; observations come from the most recent finished `run` run under it |
| `--mock` | `false` | generate observations offline by running every experiment with the mock client |

Without `--mock` it observes the most recent finished run of `subcommand=run` that is not a sweep child, and records it as `lineage.derived_from`. One `run` covers one experiment, so only that experiment's anchors are filled — the other twelve are `NO_DATA`, as before. `--mock` runs all seven offline to fill every anchor.

It never panics on missing data. The paper's reported values go to `reference.csv` and the observations to `metrics.csv`, both keyed on the anchor's metric name so the difference can be taken later; `artifacts/paper_anchors.csv` and `artifacts/reproduce_summary.csv` keep the tolerance and the `status ∈ {PASS, off, NO_DATA}` verdict. Because `--mock` is a stub, its "PASS"es are incidental — only a live model produces genuine agreement.

The run's `domain` is `analysis`, not `simulation`: the classification uses no RNG, and claiming `simulation` would require writing a `master_seed` that does not exist.

## Python `jones-tools`

Install at the workspace root with `uv sync`, then invoke via `uv run jones-tools <subcommand>`. The CLI dispatches: `visualize`, `visualize-sweep`, `show-experiment-settings`, `reproduce-paper`, `fetch-dataset`. The figure subcommands take a run directory positionally or via a flag; omit it and the run is resolved with `runvault path --latest`, which needs the `runvault` binary on `PATH` (or `RUNVAULT=<path to it>`). Nothing globs `results/`.

Figures are written **outside** the run, to `results/jones/figures/{run_slug}/`: what is made after a run ended is not part of its record, and `manifest.csv` was settled by `finish()`.

```bash
uv run jones-tools visualize                                # latest `run` run
uv run jones-tools visualize results/jones/run_2026...      # or --results-dir DIR
uv run jones-tools visualize-sweep                          # latest sweep parent
uv run jones-tools show-experiment-settings                 # or --results-dir DIR
uv run jones-tools reproduce-paper                          # or --results-dir / --summary-csv
uv run jones-tools fetch-dataset                            # fetch the official 164-problem HumanEval
```

| Subcommand | Role | Reads | Writes |
|---|---|---|---|
| `visualize` | single-run figures (experiment auto-detected) | `events.jsonl` + `config.json` | `fig_accuracy.png` (E1–E4), `fig_indicator.png` |
| `visualize-sweep` | swept parameter × indicator (and Δ) | the sweep parent's children | `fig_sweep.png` |
| `show-experiment-settings` | pretty-print the run config | `config.json` | — (console) |
| `reproduce-paper` | per-anchor observed-vs-paper figure | `reproduce_summary.csv` | `reproduce_paper.png` |
| `fetch-dataset` | download the official HumanEval set | network (`openai/human-eval`, MIT) | `simulation/data/HumanEval.jsonl` |

Figures use a headless (Agg) backend, so they render in CI without a display. See [Visualization](visualization.md).

### `fetch-dataset` — the official HumanEval set

The repo ships only a curated 8-problem HumanEval subset so everything runs offline. `fetch-dataset` downloads the full **164-problem** set (`openai/human-eval`, MIT-licensed) and writes it to `simulation/data/HumanEval.jsonl` — exactly the path the Rust loader auto-detects, so `jones run ... --full` then runs all 164 problems with no further wiring. Only HumanEval is fetchable; the paper's MathEquations set is the authors' own and is not public (the replication synthesizes it in code).

```bash
uv run jones-tools fetch-dataset            # → simulation/data/HumanEval.jsonl (164 problems)
cargo run -p jones-simulation -- run --experiment framing --full
```

| Flag | Default | Meaning |
|---|---|---|
| `--output <PATH>` | `<repo>/simulation/data/HumanEval.jsonl` | where to write the JSONL |
| `--url <URL>` | the `openai/human-eval` `HumanEval.jsonl.gz` raw URL | source archive (gzipped JSONL) |
| `--force` | `false` | re-fetch even if a valid file already exists |

It is **idempotent** (skips when a valid 164-problem file is already present), writes **atomically** (temp file → rename, so an interrupted run never leaves a corrupt file), **verifies** the download against a pinned SHA-256 and the expected 164-problem schema (failing loudly rather than saving a partial set), and prints the URL, license, byte size, SHA-256, and problem count. The downloaded full set is git-ignored — only the 8-problem subset stays tracked. Download is always an explicit, separate step: `run` never fetches implicitly (offline-first).

## Results layout

```
results/
├── llm_cache.json                       # live runs: prompt→completion cache (shared, not in any run)
└── jones/
    ├── latest_finished -> run_2026.../  # maintained by runvault
    ├── run_20260101_120000_<hashes>/    # run (also: a sweep child)
    │   ├── run.json                     # paper / dataset / model / lineage / hashes
    │   ├── config.json                  # the conditions this experiment reads
    │   ├── events.jsonl                 # x.jones2022.condition, one line per condition
    │   ├── metrics.csv                  # scope=run aggregates only
    │   ├── status.json                  # duration_sec lives here, not in metrics
    │   └── manifest.csv
    ├── sweep_20260101_120100_<hashes>/  # sweep parent: the grid definition
    ├── reproduce_20260101_120200_<hashes>/
    │   ├── reference.csv                # the paper's reported values
    │   ├── metrics.csv                  # the observations + PASS/off/NO_DATA counts
    │   └── artifacts/
    │       ├── paper_anchors.csv
    │       └── reproduce_summary.csv    # paper vs observed, with the tolerance
    └── figures/<run_slug>/              # written after the run ended, so outside it
        ├── fig_accuracy.png             # visualize (E1–E4)
        ├── fig_indicator.png            # visualize
        ├── fig_sweep.png                # visualize-sweep
        └── reproduce_paper.png          # reproduce-paper
```

Runs made before this repository moved to runvault are still under `results/{stamp}/` in the old flat shape. They are left exactly as they were; the Python tools read them too.
