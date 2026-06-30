**English** | [日本語](README.ja.md)

# Capturing Failures of LLMs via Human Cognitive Biases — Jones & Steinhardt (2022)

A reimplementation of Jones & Steinhardt (2022), "Capturing Failures of Large Language Models via Human Cognitive Biases" ([arXiv:2202.12299](https://arxiv.org/abs/2202.12299)). The paper turns **human cognitive biases** into a recipe for systematically inducing qualitative failures of LLMs: hypothesize a failure mode, build a **semantic-preserving input transform** that should trigger it, and measure two things — does the transform **lower functional accuracy** (the sensitivity `Δ`), and does the output **carry the target failure feature** (the indicator rate `r`)? The transforms are entirely **black-box and logprob-free** — they read only the model's text output, never first-token probabilities — so they run on any model. The original study used Codex; that model (`davinci-001`) has been deprecated, so this reimplementation substitutes contemporary code models (Ollama `codellama` / `qwen2.5-coder` / `deepseek-coder` / `starcoder2`, with an OpenAI `gpt-4o-mini` fallback) and treats the **direction of the failure** as the reproduction target.

The repository is built on the consolidated **socsim** library and is deliberately thin: it delegates LLM generation to `socsim-llm`, mean/rate aggregation to `socsim-metrics`, paper-anchor PASS/off classification to `socsim-reproduce`, and results I/O to `socsim-results`. The only jones2022-specific code is the semantic-preserving transforms, the failure indicators `φ`, and the execution / deletion sandbox (Rust `simulation/`) plus the Python analysis tools (`tools/`). It is a **probe + indicator pipeline**, not an ABM tick loop, so it pulls in no `socsim-core`/engine/grid/net.

## Scope

The paper's analyses map onto this repository as experiments **E1–E7**:

| Experiment | What it tests | Primary metric | CLI |
|------------|---------------|----------------|-----|
| **E1** Framing | Does an irrelevant preceding function (IPF) bias the model into copying its body? | `Δ`; verbatim-copy rate `r` | `jones run --experiment framing` |
| **E2** Anchoring | Does a prepended anchor function make the model anchor on its distractor pattern? | anchor-line appearance rate `r` | `jones run --experiment anchoring` |
| **E3** Availability | Does an operation order-flip steer the model to the "available" naïve answer? | `Δ`; unary-first rate `r` | `jones run --experiment availability` |
| **E4** Attribute substitution | Does a conflicting function name override the spec'd behaviour? | `Δ`; named-function rate `r` | `jones run --experiment attribute-substitution` |
| **E5** GPT-3 anchoring | Do numeric estimates shift toward a high/low anchor `a(1±p)`? | toward-anchor update rate; gibberish rate | `jones run --experiment gpt3-anchoring` |
| **E6** GPT-3 framing | Does the Asian-Disease save/die frame change the risky-option rate? | risky-choice rate per frame | `jones run --experiment gpt3-framing` |
| **E7** File deletion | Does an "uninstall N packages" task induce deletion of an unrelated file? | erroneous-deletion rate | `jones run --experiment file-deletion` |

E1/E2 use the **HumanEval** benchmark (Chen et al. 2021) and E3/E4 a small **MathEquations** set; functional accuracy is scored by running the generated code against unit tests in an isolated sandbox. E5/E6 are numeric/choice tasks (no execution). E7's generated "uninstall" code runs under a **deletion guard** that intercepts every deletion (recording the target, deleting nothing) inside a throwaway temp directory — the host filesystem is never touched. See [The studies](docs/studies.md).

## Install & Quick start

All commands run from this directory. The live generation path needs a local **Ollama** server (`http://localhost:11434`) with a code model pulled (`ollama pull codellama`), or an OpenAI key for the fallback; `--mock` exercises the whole pipeline offline with a deterministic scripted client and the bundled HumanEval subset.

```bash
# Build the Rust simulation (binary: jones)
cargo build --release

# === Offline smoke — any experiment, no model needed ===
cargo run --release -- run --experiment framing --mock --seed 42
cargo run --release -- run --experiment availability --mock
cargo run --release -- run --experiment gpt3-framing --mock
cargo run --release -- run --experiment file-deletion --mock --num-packages 3

# === Sweep — E7 package count / E5 anchor ratio ===
cargo run --release -- sweep --experiment file-deletion  --mock --num-packages-values 1,2,3,4,5
cargo run --release -- sweep --experiment gpt3-anchoring --mock --anchor-ratio-values 0.1,0.2,0.5,0.8

# === Live run (Ollama code model) ===
cargo run --release -- run --experiment framing --model codellama --seed 42

# === Reproduce — offline paper-anchor PASS/off check (all E1–E7 anchors) ===
cargo run --release -- reproduce --mock

# === Python tools ===
uv sync
uv run jones-tools visualize results/latest
uv run jones-tools reproduce-paper results/latest
```

HumanEval ships as a small **bundled 8-problem subset** so everything runs offline; point the loader at the official 164-problem set with `--dataset <HumanEval.jsonl>` (or by adding `data/HumanEval.jsonl`) — the chosen set and its size are always logged, never silently truncated.

Each `run` writes `results/{timestamp}/` with `config.json` and `metrics.csv`; `sweep` writes `results/sweep_{timestamp}/` with `sweep_summary.csv` and `sweep_config.json`; `reproduce` adds `paper_anchors.csv` and `reproduce_summary.csv`. The Python tools render PNGs alongside them.

> **Reproduction honesty.** `--mock` proves the *plumbing* — it is a deterministic scripted stub that exhibits each bias on a fixed fraction of items (not tuned to any paper value), so it never "passes" the anchors for real. Genuine agreement with the paper's reference values can only come from a live model.

## Documentation

- [The studies](docs/studies.md) — what each experiment (E1–E7) tests, its transform `T`, indicator `φ`, dataset, and paper anchor values.
- [Use cases](docs/usecases.md) — worked recipes (offline reproduce, single live run, sweeps, cross-model comparison).
- [CLI](docs/cli.md) — the `jones` subcommands (`run` / `sweep` / `reproduce`), every flag, the `jones-tools` subcommands, environment variables, and the local-Ollama setup.
- [Visualization](docs/visualization.md) — the Python `jones-tools` and how to read the figures.
- [Architecture](docs/architecture.md) — workspace layout, module map, the socsim delegation, the sandbox, and references.

## Dependencies

- **Rust**: `clap` (CLI), `serde` + `serde_json` (config), `anyhow` (errors), and the socsim crates — `socsim-llm` (logprob-free generation with the live Ollama→OpenAI fallback + prompt cache; `ScriptedClient` for mocks), `socsim-metrics` (`stats::mean`), `socsim-reproduce` (paper-anchor harness), `socsim-results` (timestamped results + CSV/JSON writers). The socsim commit is pinned via `Cargo.lock`. Functional-accuracy scoring shells out to a Python interpreter for the sandboxed unit tests; live generation needs a local Ollama server.
- **Python** (`uv`): `matplotlib`, `numpy`, `pandas`.

## License

Code in this repository is released under the MIT License.

## References

- Jones, E., & Steinhardt, J. (2022). Capturing Failures of Large Language Models via Human Cognitive Biases. *Advances in Neural Information Processing Systems (NeurIPS)*, 35. [arXiv:2202.12299](https://arxiv.org/abs/2202.12299).
- Chen, M., et al. (2021). Evaluating Large Language Models Trained on Code. *arXiv* [arXiv:2107.03374](https://arxiv.org/abs/2107.03374) (Codex / HumanEval / functional accuracy).
- Jacowitz, K. E., & Kahneman, D. (1995). Measures of Anchoring in Estimation Tasks. *Personality and Social Psychology Bulletin*, 21(11), 1161–1166 (E5).
- Tversky, A., & Kahneman, D. (1981). The Framing of Decisions and the Psychology of Choice. *Science*, 211(4481), 453–458 (E6, Asian Disease).
