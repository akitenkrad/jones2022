**English** | [日本語](architecture.ja.md)

# Architecture

## Repository layout

```
replications/jones2022/
├── Cargo.toml                  # Rust workspace (members = ["simulation"])
├── pyproject.toml              # uv workspace (members = ["tools"])
├── simulation/                 # Rust crate `jones-simulation` (bin `jones`)
│   ├── data/
│   │   └── humaneval_sample.jsonl  # bundled 8-problem HumanEval subset (offline; full set fetched to data/HumanEval.jsonl, git-ignored)
│   └── src/
│       ├── main.rs             # clap: run / sweep / reproduce + results writers
│       ├── lib.rs              # crate root; the socsim-delegation table
│       ├── config.rs           # Experiment (E1–E7), FRAMING_LINES, ANCHOR_LINES, Config
│       ├── datasets.rs         # HumanEval loader (bundled subset / full set) + MathEquations (curated 8 + seeded generator)
│       ├── transforms.rs       # semantic-preserving transforms T (IPF / anchor fn / order-flip / conflicting name)
│       ├── indicators.rs       # failure indicators φ (verbatim copy / anchor line / distractor token)
│       ├── eval.rs             # run_experiment: transform → query → sandbox → φ → MetricRow (E1–E4)
│       ├── gpt3.rs             # E5 numeric anchoring + E6 Asian-Disease framing (no execution)
│       ├── filedelete.rs       # E7 driver over the deletion guard
│       ├── sandbox.rs          # isolated test execution + the file-deletion guard
│       ├── anchors.rs          # PAPER_ANCHORS: the paper's reference values for socsim-reproduce
│       └── mock.rs             # deterministic scripted oracles (one per experiment family)
├── tools/                      # Python package `jones-tools` (module `jones_tools`, src layout)
│   └── src/jones_tools/
│       ├── cli.py                       # dispatcher: visualize / visualize-sweep / show-experiment-settings / reproduce-paper
│       ├── visualize.py                 # single-run figures (auto-detects the experiment)
│       ├── visualize_sweep.py           # sweep figures (param × r / Δ)
│       ├── show_experiment_settings.py  # config.json pretty-print
│       └── reproduce_paper.py           # figures from reproduce_summary.csv
└── docs/                       # bilingual (.md + .ja.md)
```

Two projects in one tree: a **Cargo workspace** (`simulation`, crate `jones-simulation`, binary `jones`) and a **uv workspace** (`tools`, package `jones-tools`). The Rust side runs the probe pipeline and writes metrics; the Python side is visualization.

## Built on socsim: what is delegated

jones2022 is a thin layer over the consolidated **socsim** library. It is a **probe + indicator pipeline**, not an ABM tick loop, so it pulls in **no** `socsim-core`/engine/grid/net — the paper has no agents, space, network, or time dynamics, so those primitives are deliberately unused. Every piece of plumbing is delegated, and the only paper-specific Rust code is the transforms, the indicators, and the sandbox:

| Concern | Delegated to | Used in |
|---|---|---|
| LLM generation (logprob-free `complete`) | `socsim-llm` | `eval.rs`, `gpt3.rs`, `filedelete.rs`, `main.rs` |
| Mean / rate aggregation | `socsim-metrics::stats` | `eval.rs`, `main.rs` |
| Paper-anchor PASS/off reproduction harness | `socsim-reproduce` | `anchors.rs`, `main.rs` |
| Timestamped results dir + CSV/JSON writers | `socsim-results` | `main.rs` |

The socsim crates are pulled as git dependencies pinned via `Cargo.lock`. `socsim-llm` is taken with the `live` feature so the live Ollama→OpenAI fallback client is available alongside the `ScriptedClient` mock.

## Two-layer determinism

The socsim core is deterministic; the LLM layer is not. jones2022 keeps that split explicit:

- The **deterministic layer** is the pipeline plumbing: dataset construction, transform application, indicator evaluation, sandbox execution, and metric aggregation are all reproducible given the same inputs.
- The **LLM layer** is confined to `socsim-llm`. Generation is greedy (`temperature = 0`) with the seed pinned, and the live path wraps the Ollama→OpenAI fallback in a **prompt cache** (keyed on prompt + model), so a re-run with a warm cache replays identical completions — turning a noisy model into a reproducible oracle. The model id, temperature, seed, and mock flag are recorded in `config.json`.

## The pipeline

For each experiment, `run` builds the dataset, injects an `LlmClient`, and runs one of three eval paths, each emitting long-format `MetricRow`s (`experiment, variant, condition, n, functional_accuracy, indicator_rate, delta`).

### Code experiments (E1–E4) — `eval.rs` + `sandbox.rs`

E1/E2 run over **HumanEval** and E3/E4 over **MathEquations**. The `run_experiment` driver:

1. **Baseline pass** — query the model on the unmodified prompt, assemble `prompt + completion + test`, and run it in the sandbox to score functional accuracy.
2. **Transform pass** — apply the semantic-preserving transform `T` (`transforms.rs`), query again, score accuracy, and evaluate the indicator `φ` (`indicators.rs`) on the *completion only* (never the assembled program, so the injected lines do not trivially satisfy it).
3. **Aggregate** — functional accuracy per condition, sensitivity `Δ = acc(baseline) − acc(transform)`, and indicator rate `r`, via `socsim-metrics::stats::mean`.

E1 produces one transform row per framing line; E2 measures the indicator per anchor line on a single transform; E3/E4 use one prompt-prefix transform with a per-problem distractor token as `φ`.

### Direct-answer experiments (E5/E6) — `gpt3.rs`

E5 (numeric anchoring) and E6 (Asian-Disease framing) are choice/numeric tasks with **no code execution**. The model answers a number (E5) or a letter (E6); the indicator reads the answer directly (does the estimate shift toward the anchor; is the risky option chosen). Functional accuracy is not applicable and is recorded as `NaN`.

### File-deletion (E7) — `filedelete.rs` + the deletion guard

E7 asks the model to write an "uninstall N packages" script and measures whether it deletes an unrelated **protected** file. The generated code runs under `sandbox::run_with_deletion_guard`, which is safe by construction (see below).

## The sandbox — `sandbox.rs`

Functional-accuracy scoring shells the assembled program out to a Python interpreter in a **throwaway temp directory** with the child's working directory pinned inside it, guarded by a wall-clock timeout. If no interpreter is found the runner reports `Unavailable` (accuracy undetermined) rather than failing, so the indicator path still runs in a Python-less environment.

The **deletion guard** (E7) makes erroneous-deletion measurement safe: a Python preamble monkeypatches every standard deletion API (`os.remove`/`unlink`/`rmdir`/`removedirs`, `shutil.rmtree`, `pathlib.Path.unlink`/`rmdir`) to **record the target path and delete nothing**, the child runs in a temp dir seeded with dummy package files plus the protected file, and code containing escape primitives (`subprocess`, `os.system`, `eval`, …) is refused outright. Deletions are intercepted in-process — nothing is ever actually removed, on the host or in the sandbox.

## Reproduction harness — `anchors.rs`

`PAPER_ANCHORS` holds the paper's quantitative reference values (the verbatim-copy and accuracy-drop figures for E1, the anchor-line rates for E2, the order-flip and named-function figures for E3/E4, the GPT-3 update / risky-choice rates for E5/E6, and the file-deletion rate for E7), fed to `socsim_reproduce::build_rows` alongside an observation lookup. The harness mechanics — PASS/off/NO_DATA classification and the CSV writers — live in `socsim-reproduce`; only the values are jones2022's. `reproduce` either reads the latest run's `metrics.csv` (per-experiment) or, with `--mock`, runs every experiment offline to source every anchor.

## Injection and testing

The eval entry points all take `&dyn LlmClient`, so the entire pipeline is exercised in tests and under `--mock` with `socsim_llm::mock::ScriptedClient` — no live model. The mocks are deterministic scripted stubs that exhibit each bias on a fixed ~60% fraction of items (an arbitrary stub fraction, not tuned to any paper value), so the offline runs produce intermediate `Δ`/`r` and real agreement with the anchors can only come from a live model.

## References

- Jones, E., & Steinhardt, J. (2022). Capturing Failures of Large Language Models via Human Cognitive Biases. *Advances in Neural Information Processing Systems (NeurIPS)*, 35. [arXiv:2202.12299](https://arxiv.org/abs/2202.12299).
- Chen, M., et al. (2021). Evaluating Large Language Models Trained on Code. *arXiv* [arXiv:2107.03374](https://arxiv.org/abs/2107.03374).
