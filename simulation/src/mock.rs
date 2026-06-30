//! The offline scripted mock clients — deterministic oracles used by `--mock`
//! and by the integration tests, one per experiment family.
//!
//! ## Honesty contract
//!
//! Each mock is a clearly-labeled **scripted stub**, not a model. It exhibits
//! the target bias on a deterministic ~[`MOCK_BIAS_PERCENT`]% of items (via
//! [`exhibits_bias`]) so the offline pipeline produces *intermediate* `Δ`/`r`
//! (not a degenerate all-or-nothing signal). That fraction and the mocks'
//! directional choices are **arbitrary stubs, deliberately not tuned to any
//! paper anchor** — a real PASS in `reproduce` must come from a live model. The
//! mocks only encode the qualitative *direction* of each bias so the pipeline is
//! exercised end-to-end.

use std::collections::HashMap;

use socsim_llm::mock::ScriptedClient;

use crate::config::{FRAMING_LINES, MOCK_BIAS_PERCENT};
use crate::datasets::Problem;
use crate::gpt3::{estimation_items, EstimationItem};
use crate::sandbox::PROTECTED_FILE;
use crate::transforms::{ANCHOR_FN_NAME, CONFLICTING_NAME_MARKER, IPF_FN_NAME, ORDER_FLIP_MARKER};

/// Deterministic per-item bias gate: hashes `key` (FNV-1a) and exhibits the bias
/// for ~[`MOCK_BIAS_PERCENT`]% of keys. Stable across runs; not anchor-tuned.
pub fn exhibits_bias(key: &str) -> bool {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h % 100 < MOCK_BIAS_PERCENT
}

/// Build the deterministic scripted oracle for the **code** experiments (E1–E4)
/// over `problems`. Baseline ⇒ canonical solution (passes). A transformed prompt
/// ⇒ the bias outcome for the ~60% of items where [`exhibits_bias`] holds, else
/// still the canonical solution (so `Δ`/`r` land between 0 and 1).
pub fn build_mock_client(problems: &[Problem]) -> ScriptedClient {
    // baseline prompt → canonical; and a substring index for transformed prompts.
    let sols: HashMap<String, String> = problems
        .iter()
        .map(|p| (p.prompt.clone(), p.canonical_solution.clone()))
        .collect();
    let index: Vec<(String, String, Option<String>)> = problems
        .iter()
        .map(|p| {
            (
                p.prompt.clone(),
                p.canonical_solution.clone(),
                p.distractor_solution.clone(),
            )
        })
        .collect();
    let ipf_prefixes: Vec<(String, String)> = FRAMING_LINES
        .iter()
        .map(|l| {
            (
                format!("def {IPF_FN_NAME}(value):\n    {}\n", l.code),
                l.code.to_string(),
            )
        })
        .collect();
    let anchor_marker = format!("def {ANCHOR_FN_NAME}(");

    // Find the (canonical, distractor) for a transformed prompt by locating the
    // original problem prompt as a substring.
    let lookup = move |prompt: &str| -> Option<(String, Option<String>)> {
        index
            .iter()
            .find(|(orig, _, _)| prompt.contains(orig.as_str()))
            .map(|(_, canon, distractor)| (canon.clone(), distractor.clone()))
    };

    ScriptedClient::new("mock", move |prompt: &str| {
        let biased = exhibits_bias(prompt);
        // E1 framing.
        for (prefix, code) in &ipf_prefixes {
            if prompt.contains(prefix.as_str()) {
                if biased {
                    return format!("    {code}\n"); // copy the framing line (fails tests)
                }
                return lookup(prompt)
                    .map(|(c, _)| c)
                    .unwrap_or_else(|| "    return None\n".into());
            }
        }
        // E2 anchoring.
        if prompt.contains(&anchor_marker) {
            if biased {
                return "    for var in []:\n        print(var)\n    return tmp\n".to_string();
            }
            return lookup(prompt)
                .map(|(c, _)| c)
                .unwrap_or_else(|| "    return None\n".into());
        }
        // E3 / E4 (MathEquations): order-flip / conflicting-name → distractor.
        if prompt.contains(ORDER_FLIP_MARKER) || prompt.contains(CONFLICTING_NAME_MARKER) {
            if let Some((canon, distractor)) = lookup(prompt) {
                return if biased {
                    distractor.unwrap_or(canon)
                } else {
                    canon
                };
            }
        }
        // Baseline ⇒ canonical (exact key), else stub.
        sols.get(prompt)
            .cloned()
            .unwrap_or_else(|| "    return None\n".to_string())
    })
}

/// Build the E5 (numeric estimation / anchoring) scripted oracle. Baseline ⇒ the
/// item's true value; anchored ⇒ a value moved ~30% toward the anchor for the
/// biased fraction, else the true value. Always returns a parseable number (the
/// stub is never "gibberish").
pub fn build_gpt3_anchoring_mock() -> ScriptedClient {
    let items: Vec<EstimationItem> = estimation_items();
    // Mock move fraction toward the anchor — arbitrary, not the eval's `p`.
    const MOVE: f64 = 0.3;
    ScriptedClient::new("mock", move |prompt: &str| {
        let Some(item) = items.iter().find(|it| prompt.contains(it.question)) else {
            return "0".to_string();
        };
        let a = item.true_value;
        let biased = exhibits_bias(prompt);
        let val = if prompt.contains("much higher than usual") {
            if biased {
                a * (1.0 + MOVE)
            } else {
                a
            }
        } else if prompt.contains("much lower than usual") {
            if biased {
                a * (1.0 - MOVE)
            } else {
                a
            }
        } else {
            a
        };
        format!("{}", val.round() as i64)
    })
}

/// Build the E6 (Asian-Disease framing) scripted oracle. Option B is risky. The
/// biased fraction picks the frame-consistent choice (save⇒sure A, die⇒risky B),
/// the rest the opposite — so the risky rate is higher in the die frame
/// (direction only; magnitudes not tuned).
pub fn build_gpt3_framing_mock() -> ScriptedClient {
    ScriptedClient::new("mock", move |prompt: &str| {
        let biased = exhibits_bias(prompt);
        let die_frame = prompt.contains("400 people will die");
        // die+biased ⇒ risky (B); save+biased ⇒ sure (A); non-biased ⇒ opposite.
        let risky = if die_frame { biased } else { !biased };
        if risky {
            "B".to_string()
        } else {
            "A".to_string()
        }
    })
}

/// Build the E7 (file-deletion) scripted oracle. Returns an over-broad glob
/// "uninstall" script that deletes every `*.py` file; for the biased fraction it
/// also deletes the protected file (the high-impact error). Deletions are
/// intercepted by the sandbox shim, so nothing is ever actually removed.
pub fn build_file_deletion_mock() -> ScriptedClient {
    ScriptedClient::new("mock", move |prompt: &str| {
        let mut code = String::from(
            "import os\nfor f in os.listdir('.'):\n    if f.endswith('.py'):\n        os.remove(f)\n",
        );
        if exhibits_bias(prompt) {
            code.push_str(&format!("os.remove('{PROTECTED_FILE}')\n"));
        }
        code
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Experiment;
    use crate::eval::run_experiment;

    fn problems() -> Vec<Problem> {
        // Many ids so the bias gate produces an intermediate (0,1) rate.
        (0..20)
            .map(|i| Problem {
                task_id: format!("T/{i}"),
                prompt: format!("def add{i}(x, y):\n    \"\"\"add {i}\"\"\"\n"),
                canonical_solution: "    return x + y\n".to_string(),
                test: format!("def check(candidate):\n    assert candidate(2, 3) == 5\n# {i}\n"),
                entry_point: format!("add{i}"),
                distractor_solution: None,
                distractor_token: None,
            })
            .collect()
    }

    #[test]
    fn exhibits_bias_is_deterministic_and_intermediate() {
        let keys: Vec<String> = (0..200).map(|i| format!("k{i}")).collect();
        let hits = keys.iter().filter(|k| exhibits_bias(k)).count();
        assert!(hits > 0 && hits < keys.len(), "neither always nor never");
        assert_eq!(exhibits_bias("k0"), exhibits_bias("k0"));
    }

    #[test]
    fn framing_mock_indicator_is_between_zero_and_one() {
        let client = build_mock_client(&problems());
        let rows = run_experiment(&client, Experiment::Framing, &problems(), 42);
        let r = rows
            .iter()
            .find(|r| r.variant == "raise_notimplemented")
            .unwrap();
        assert!(
            r.indicator_rate > 0.0 && r.indicator_rate < 1.0,
            "fractional mock ⇒ 0 < r < 1, got {}",
            r.indicator_rate
        );
    }
}
