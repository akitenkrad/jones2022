**English** | [日本語](cli.ja.md)

# CLI

Two command-line surfaces: the Rust binary `jones` (the probe pipeline) and the Python `jones-tools` (visualization).

## Rust `jones`

Build with `cargo build --release`; the binary is `target/release/jones`. Three subcommands: `run`, `sweep`, `reproduce` — all always compiled, no feature flags. The whole method is **black-box and logprob-free**: it never requests token probabilities, only the generated text. `--mock` runs the whole pipeline offline with a deterministic scripted client; without it, generation goes to the live **Ollama→OpenAI fallback** with a prompt cache.

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

Runs a single cognitive-bias experiment, scores it, and writes `metrics.csv` + `config.json`.

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

Outputs (under `results/{stamp}/`): `config.json` and `metrics.csv` (long: `experiment, variant, condition, n, functional_accuracy, indicator_rate, delta`). For code experiments (E1–E4) a `baseline` row carries functional accuracy and the `transform` rows carry accuracy + indicator + `Δ`; for E5/E6/E7 accuracy is `NaN` and the signal is the indicator rate.

#### MathEquations — the generated set (E3/E4)

Unlike HumanEval, the paper's MathEquations set is the authors' own and is not public, so the replication **synthesizes** it. `--math-count`/`--math-seed` scale it deterministically: the curated 8 problems are a stable prefix, and any extras are generated from operator-precedence templates (e.g. `(x + y) * k` vs the naïve `x + y * k`). Each generated problem keeps the dataset's invariants — its `distractor_token` occurs only in the wrong body, and its unit test asserts the *canonical* value on inputs where the two readings disagree, so a biased completion provably fails. The exact `n` and seed are recorded in `config.json` (`dataset: "math_equations(n=…, seed=…)"`). `--math-count 8` (the default) reproduces the original curated set unchanged.

```bash
cargo run -p jones2022-simulation -- run --experiment availability --math-count 90 --math-seed 7
```

### `sweep` — vary a parameter

Runs an experiment across a parameter grid and writes a summary. Two experiments are parameterized: **E7** over the package count and **E5** over the anchor ratio.

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

Outputs (under `results/sweep_{stamp}/`): `sweep_config.json` and `sweep_summary.csv` (long: `experiment, param, value, variant, functional_accuracy, indicator_rate, delta`).

### `reproduce` — offline paper-anchor check

Joins the built-in `PAPER_ANCHORS` (the paper's E1–E7 reference values) against observed metrics and classifies each anchor.

```bash
# Offline: run every experiment with the mock client, then classify
cargo run --release -- reproduce --mock

# From a real run's metrics.csv (the experiment that produced results/latest)
cargo run --release -- reproduce
```

| Flag | Default | Meaning |
|---|---|---|
| `--results <DIR>` | `results` | results root to read observed metrics from (`{DIR}/latest/metrics.csv`) |
| `--mock` | `false` | generate observations offline by running every experiment with the mock client |

It never panics on missing data: anchors with no observation become `NO_DATA`. Outputs `paper_anchors.csv` (the reference values) and `reproduce_summary.csv` (observed vs paper, `status ∈ {PASS, off, NO_DATA}`), and prints a `PASS / off / NO_DATA` tally. Because `--mock` is a stub, its "PASS"es are incidental — only a live model produces genuine agreement.

## Python `jones-tools`

Install at the workspace root with `uv sync`, then invoke via `uv run jones-tools <subcommand>`. The CLI dispatches: `visualize`, `visualize-sweep`, `show-experiment-settings`, `reproduce-paper`, `fetch-dataset`. The figure subcommands take a results dir positionally or via a flag (default `results/latest`).

```bash
uv run jones-tools visualize results/latest                 # or --results-dir DIR
uv run jones-tools visualize-sweep results/sweep_20260101_120000   # or --sweep-dir DIR
uv run jones-tools show-experiment-settings results/latest  # or --results-dir DIR
uv run jones-tools reproduce-paper results/latest           # or --results-dir / --summary-csv
uv run jones-tools fetch-dataset                            # fetch the official 164-problem HumanEval
```

| Subcommand | Role | Reads | Writes |
|---|---|---|---|
| `visualize` | single-run figures (experiment auto-detected) | `metrics.csv` + `config.json` | `fig_accuracy.png` (E1–E4), `fig_indicator.png` |
| `visualize-sweep` | swept parameter × indicator (and Δ) | `sweep_summary.csv` + `sweep_config.json` | `fig_sweep.png` |
| `show-experiment-settings` | pretty-print the run config | `config.json` | — (console) |
| `reproduce-paper` | per-anchor observed-vs-paper figure | `reproduce_summary.csv` | `reproduce_paper.png` |
| `fetch-dataset` | download the official HumanEval set | network (`openai/human-eval`, MIT) | `simulation/data/HumanEval.jsonl` |

Figures use a headless (Agg) backend, so they render in CI without a display. See [Visualization](visualization.md).

### `fetch-dataset` — the official HumanEval set

The repo ships only a curated 8-problem HumanEval subset so everything runs offline. `fetch-dataset` downloads the full **164-problem** set (`openai/human-eval`, MIT-licensed) and writes it to `simulation/data/HumanEval.jsonl` — exactly the path the Rust loader auto-detects, so `jones run ... --full` then runs all 164 problems with no further wiring. Only HumanEval is fetchable; the paper's MathEquations set is the authors' own and is not public (the replication synthesizes it in code).

```bash
uv run jones-tools fetch-dataset            # → simulation/data/HumanEval.jsonl (164 problems)
cargo run -p jones2022-simulation -- run --experiment framing --full
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
├── latest -> 20260101_120000/
├── llm_cache.json                  # live runs: prompt→completion cache
├── 20260101_120000/                # run
│   ├── config.json
│   ├── metrics.csv                 # experiment × variant × condition → acc / r / Δ
│   ├── fig_accuracy.png            # visualize (E1–E4)
│   └── fig_indicator.png           # visualize
├── sweep_20260101_120100/          # sweep
│   ├── sweep_config.json
│   ├── sweep_summary.csv           # param × variant → acc / r / Δ
│   └── fig_sweep.png               # visualize-sweep
└── 20260101_120200/                # reproduce
    ├── paper_anchors.csv
    ├── reproduce_summary.csv        # paper vs observed, PASS/off/NO_DATA
    └── reproduce_paper.png          # reproduce-paper
```
