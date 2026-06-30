**English** | [日本語](usecases.ja.md)

# Use cases

What you can do with this reimplementation of Jones & Steinhardt (2022). Each example runs the probe pipeline (or its offline reproduction) and reports its metrics. For the conceptual explanation of each experiment see [The studies](studies.md); see [CLI](cli.md) for every flag and [Architecture](architecture.md) for how the pipeline is wired.

> The live `run` / `sweep` examples need a local **Ollama** server (`http://localhost:11434`) with a code model pulled (`ollama pull codellama`), or `--mock` for an offline smoke. The method is logprob-free, so any model works.

## 1. Offline reproduce (no model, no downloads)

The whole transform → generation → sandbox → indicator → reproduce path runs offline with the deterministic scripted client, so the wiring is exercisable in CI:

```bash
cargo run --release -- run --experiment framing --mock --seed 42
cargo run --release -- reproduce --mock
```

`--mock` injects a scripted oracle that exhibits each bias on a fixed fraction of items; `reproduce --mock` runs all seven experiments, joins the paper's E1–E7 anchors against the result, and prints a `PASS / off / NO_DATA` tally. This proves the plumbing; the mock's "PASS"es are incidental, since real agreement with the paper values can only come from a live model.

## 2. A single live experiment

Run one experiment against a real code model and read the sensitivity and indicator:

```bash
cargo run --release -- run --experiment framing --model codellama --seed 42
uv run jones-tools visualize results/latest
```

`metrics.csv` carries the `baseline` accuracy and, per framing line, the transformed accuracy, the sensitivity `Δ`, and the verbatim-copy rate `r`. `visualize` writes `fig_accuracy.png` and `fig_indicator.png`. The paper's framing effect is a 22.3–30.5 pt accuracy drop with a verbatim-copy rate up to 81%; a robust modern model may resist most framing lines while still showing the effect on a spec-conflicting line such as `return False`.

## 3. Scoped live smoke (a couple of problems)

Live runs cost queries; scope a smoke to the first one or two problems:

```bash
cargo run --release -- run --experiment framing --model codellama --limit 2
```

`--limit N` keeps only the first N code problems (E1–E4). The chosen HumanEval set and its size are logged, so the scoping is explicit.

To run E1/E2 over the full 164-problem HumanEval set, fetch it once and pass `--full`:

```bash
uv run jones-tools fetch-dataset                         # → data/HumanEval.jsonl (164 problems, MIT)
cargo run --release -- run --experiment framing --model codellama --full
```

## 4. Availability and attribute substitution (E3 / E4)

The MathEquations experiments need no external data — the set is synthesized in code:

```bash
cargo run --release -- run --experiment availability --mock
cargo run --release -- run --experiment attribute-substitution --mock
uv run jones-tools visualize results/latest
```

Each writes a `baseline` row and one `transform` row whose `Δ` is the accuracy drop and whose `r` is the rate at which the output is the distractor (unary-first solution for E3, named operation for E4).

The set ships as a curated 8 problems and scales deterministically toward the paper's ~90-per-setting size with `--math-count N --math-seed S` (the 8 stay a stable prefix; extras are generated from operator-precedence templates). The exact `n` and seed are recorded in `config.json`:

```bash
cargo run --release -- run --experiment availability --mock --math-count 90 --math-seed 7
```

## 5. The GPT-3 reproductions (E5 / E6)

The numeric / choice experiments need no code execution:

```bash
cargo run --release -- run --experiment gpt3-anchoring --mock --anchor-ratio 0.5
cargo run --release -- run --experiment gpt3-framing   --mock --respondents 20
uv run jones-tools visualize results/latest
```

E5 reports the toward-anchor update rate (high / low) and the gibberish rate; E6 reports the risky-choice rate per frame, where the die (loss) frame should be riskier than the save (gain) frame.

## 6. File-deletion threshold (E7 sweep)

Sweep the package count to find the deletion threshold (the paper's Fig 6: deletion is rare at ≤2 packages and ≥80% at ≥3):

```bash
cargo run --release -- sweep --experiment file-deletion --mock --num-packages-values 1,2,3,4,5,6
uv run jones-tools visualize-sweep results/sweep_<stamp>
```

The generated "uninstall" scripts run under the deletion guard, so nothing is ever actually deleted; `sweep_summary.csv` has the protected-file deletion rate per package count, and `visualize-sweep` plots it.

## 7. Anchor-ratio sweep (E5)

```bash
cargo run --release -- sweep --experiment gpt3-anchoring --mock --anchor-ratio-values 0.1,0.2,0.5,0.8
uv run jones-tools visualize-sweep results/sweep_<stamp>
```

`visualize-sweep` plots the toward-anchor update rate (and gibberish rate) against the anchor ratio `p` — the paper's anchoring effect strengthens as `p` grows.

## 8. Cross-model comparison

Because the model id is just `--model`, the same experiment can be run across models to see how the failure direction is preserved across model generations:

```bash
for m in codellama qwen2.5-coder deepseek-coder starcoder2; do
  cargo run --release -- run --experiment framing --model "$m" --results "results-$m"
done
```

Each model writes its own results tree; compare the `Δ` and `r` columns of their `metrics.csv`. The paper's claim is that the failure direction survives across model generations even as the magnitude varies with model size and instruction tuning.

## 9. Inspect a run

```bash
uv run jones-tools show-experiment-settings results/latest
```

Pretty-prints the run's `config.json` (experiment, model, seed, dataset, per-experiment parameters), resolving the `results/latest` symlink.
