**English** | [日本語](README.ja.md)

# Capturing Failures of LLMs via Human Cognitive Biases — Jones & Steinhardt (2022)

A reimplementation of Jones & Steinhardt (2022), "Capturing Failures of Large Language Models via Human Cognitive Biases" ([arXiv:2202.12299](https://arxiv.org/abs/2202.12299)). The paper turns **human cognitive biases** into a recipe for systematically inducing qualitative failures of code-generation LLMs: hypothesize a failure mode, build a **semantic-preserving input transform** that should trigger it, and measure two things — does the transform **lower functional accuracy** (the sensitivity `Δ`), and does the output **carry the target failure feature** (the indicator rate `r`)? The transforms are entirely **black-box and logprob-free** — they read only the model's text output — so they run on any code model.

The repository is built on the consolidated **socsim** library and is deliberately thin: it delegates LLM generation to `socsim-llm`, mean/accuracy aggregation to `socsim-metrics`, paper-anchor PASS/off classification to `socsim-reproduce`, and results I/O to `socsim-results`. The only jones2022-specific code is the semantic-preserving transforms, the failure indicators `φ`, and the execution sandbox (Rust `simulation/`) plus the Python analysis tools (`tools/`). It is a **probe + indicator pipeline**, not an ABM tick loop, so it pulls in no `socsim-core`/engine/grid/net.

## Scope

The paper's analyses map onto this repository as experiments **E1–E7**:

| Experiment | What it tests | Primary metric | CLI |
|------------|---------------|----------------|-----|
| **E1** Framing | Does an irrelevant preceding function (IPF) bias the model into copying its body? | sensitivity `Δ`; verbatim-copy rate | `jones run --experiment framing` |
| **E2** Anchoring | Does a prepended anchor function make the model anchor on its distractor pattern? | anchor-line appearance rate | `jones run --experiment anchoring` |
| **E3** Availability | Does an operation order-flip steer the model to the "available" naïve answer? | `Δ`; unary-first rate | `jones run --experiment availability` |
| **E4** Attribute substitution | Does a conflicting function name override the spec'd behaviour? | `Δ`; named-function rate | `jones run --experiment attribute-substitution` |
| **E5** GPT-3 anchoring | Do numeric estimates shift toward a high/low anchor `a(1±p)`? | toward-anchor update rate; gibberish rate | `jones run --experiment gpt3-anchoring` |
| **E6** GPT-3 framing | Does the Asian-Disease save/die frame change the risky-option rate? | risky-choice rate per frame | `jones run --experiment gpt3-framing` |
| **E7** File deletion | Does an "uninstall N packages" task induce deletion of an unrelated file? | erroneous-deletion rate | `jones run --experiment file-deletion` |

E1/E2 use the **HumanEval** benchmark (Chen et al. 2021) and E3/E4 a small **MathEquations** set; functional accuracy is scored by running the generated code against unit tests in an isolated sandbox. E5/E6 are numeric/choice tasks (no execution). E7's generated "uninstall" code runs under a **deletion guard** that intercepts every deletion (recording the target, deleting nothing) inside a throwaway temp directory — the host filesystem is never touched. A small HumanEval subset ships with the repository so everything runs fully offline; point the loader at the full `HumanEval.jsonl` (`--dataset` / `--full`) to scale up.

A `sweep` subcommand varies a parameter — E7 package count (`--num-packages-values`) or E5 anchor ratio (`--anchor-ratio-values`) — writing `results/sweep_{ts}/sweep_summary.csv`. `reproduce` classifies observed values against the paper's E1–E7 anchors (`socsim-reproduce`).

## Install & Quick start

All commands run from this directory. The live generation path needs a local **Ollama** server with a code model pulled (`ollama pull codellama`); `--mock` exercises the whole pipeline offline with a scripted client and the bundled HumanEval subset.

```bash
# Build the Rust simulation (binary: jones)
cargo build --release

# === E1 framing — offline smoke (no model needed) ===
cargo run --release -- run --experiment framing --mock --seed 42

# === Any experiment — offline smoke (E1–E7) ===
cargo run --release -- run --experiment availability --mock
cargo run --release -- run --experiment gpt3-framing --mock
cargo run --release -- run --experiment file-deletion --mock --num-packages 3

# === Sweep — E7 package count / E5 anchor ratio ===
cargo run --release -- sweep --experiment file-deletion --mock --num-packages-values 1,2,3,4,5
cargo run --release -- sweep --experiment gpt3-anchoring --mock --anchor-ratio-values 0.1,0.2,0.5,0.8

# === Live run (Ollama code model) ===
cargo run --release -- run --experiment framing --model codellama --seed 42

# === Reproduce — offline paper-anchor PASS/off check (all E1–E7 anchors) ===
cargo run --release -- reproduce --mock

# === Python tools ===
uv sync
uv run jones-tools visualize --results-dir results/latest
uv run jones-tools reproduce-paper --results-dir results/latest
```

Each run writes `results/{timestamp}/` with `config.json` and `metrics.csv`; `sweep` writes `results/sweep_{timestamp}/` with `sweep_summary.csv` and `sweep_config.json`; `reproduce` adds `reproduce_summary.csv` and `paper_anchors.csv`.

## References

- Jones, E., & Steinhardt, J. (2022). Capturing Failures of Large Language Models via Human Cognitive Biases. *NeurIPS* 35. [arXiv:2202.12299](https://arxiv.org/abs/2202.12299)
- Chen, M., et al. (2021). Evaluating Large Language Models Trained on Code. [arXiv:2107.03374](https://arxiv.org/abs/2107.03374)
