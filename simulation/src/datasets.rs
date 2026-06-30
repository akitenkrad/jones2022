//! HumanEval problem loader.
//!
//! jones2022's code-generation case study (E1–E4) runs over the **HumanEval**
//! benchmark (Chen et al. 2021): 164 hand-written Python problems, each a
//! function signature + docstring (`prompt`), a reference body
//! (`canonical_solution`), a unit-test `check` function (`test`), and the
//! function name (`entry_point`).
//!
//! ## Bundled subset (offline slice)
//!
//! This Phase-1 slice ships a **curated 8-problem subset** at
//! `data/humaneval_sample.jsonl` so the pipeline runs fully offline with no
//! download. The official `task_id`s are preserved and every test is
//! stdlib-only. This is a deliberate, documented subset — *not* a silent
//! truncation of a larger loaded set.
//!
//! ## Full set (pluggable)
//!
//! Point [`load_problems`] at the official `HumanEval.jsonl` (same schema) via
//! `--dataset <path>` to run all 164 problems; nothing else changes.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// One code problem (the canonical HumanEval schema, shared with the official
/// set, plus optional `distractor_*` fields the MathEquations problems use for
/// E3/E4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Problem {
    /// Stable id, e.g. `"HumanEval/0"` or `"Math/0"`.
    pub task_id: String,
    /// Function signature + docstring shown to the model (the completion prompt).
    pub prompt: String,
    /// Reference body that passes the tests (used by the mock client as oracle).
    pub canonical_solution: String,
    /// The `def check(candidate): ...` unit-test block.
    pub test: String,
    /// Function name the test calls, e.g. `"has_close_elements"`.
    pub entry_point: String,
    /// E3/E4 only: a plausible *wrong* body a biased model would emit (the mock
    /// oracle returns it when exhibiting the bias). `None` for HumanEval.
    #[serde(default)]
    pub distractor_solution: Option<String>,
    /// E3/E4 only: a substring unique to `distractor_solution` and absent from
    /// `canonical_solution`, used as the failure indicator `φ`. `None` for
    /// HumanEval.
    #[serde(default)]
    pub distractor_token: Option<String>,
}

/// Resolve the bundled fixture path, preferring `CARGO_MANIFEST_DIR/data/...`
/// (so `cargo test`, whose cwd is the crate dir, and `cargo run`, whose cwd is
/// the workspace root, both find it) and falling back to `data/...` under cwd.
fn bundled_path() -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/humaneval_sample.jsonl");
    if manifest.exists() {
        return manifest;
    }
    PathBuf::from("data/humaneval_sample.jsonl")
}

/// Path to the (optional) full official HumanEval set, vendored next to the
/// bundled subset. Not shipped; drop `HumanEval.jsonl` here to enable the full
/// 164-problem run without `--dataset`.
fn full_humaneval_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("data/HumanEval.jsonl")
}

/// Resolve which HumanEval JSONL to load and report the chosen set + size.
///
/// Precedence: explicit `path` → the full `data/HumanEval.jsonl` if `prefer_full`
/// and it exists → the bundled 8-problem subset. The chosen set and its size are
/// logged to stderr — **no silent truncation**.
pub fn load_humaneval(
    path: Option<&Path>,
    prefer_full: bool,
    limit: usize,
) -> Result<Vec<Problem>> {
    let (resolved, label) = if let Some(p) = path {
        (p.to_path_buf(), "explicit")
    } else if prefer_full && full_humaneval_path().exists() {
        (full_humaneval_path(), "full")
    } else {
        if prefer_full {
            eprintln!(
                "note: --full requested but {} is absent — using the bundled 8-problem subset",
                full_humaneval_path().display()
            );
        }
        (bundled_path(), "bundled-subset")
    };
    let problems = load_problems(Some(&resolved), limit)?;
    eprintln!(
        "HumanEval: loaded {} problems from {} [{label}]{}",
        problems.len(),
        resolved.display(),
        if limit > 0 {
            format!(" (limit {limit})")
        } else {
            String::new()
        }
    );
    Ok(problems)
}

/// Load HumanEval problems from `path`, or the bundled 8-problem subset when
/// `path` is `None`.
///
/// The file is JSONL — one [`Problem`] object per line. `limit` (> 0) keeps only
/// the first N problems (for scoped live smokes); `0` keeps all.
pub fn load_problems(path: Option<&Path>, limit: usize) -> Result<Vec<Problem>> {
    let resolved = path.map(Path::to_path_buf).unwrap_or_else(bundled_path);
    let text = std::fs::read_to_string(&resolved)
        .with_context(|| format!("reading HumanEval JSONL {}", resolved.display()))?;
    let mut problems = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let p: Problem = serde_json::from_str(line).with_context(|| {
            format!(
                "parsing problem on line {} of {}",
                i + 1,
                resolved.display()
            )
        })?;
        problems.push(p);
    }
    if problems.is_empty() {
        anyhow::bail!("no problems loaded from {}", resolved.display());
    }
    if limit > 0 && problems.len() > limit {
        problems.truncate(limit);
    }
    Ok(problems)
}

/// Build one MathEquations problem.
fn math(
    id: &str,
    sig_doc: &str,
    entry: &str,
    canonical: &str,
    test: &str,
    distractor: &str,
    token: &str,
) -> Problem {
    Problem {
        task_id: id.to_string(),
        prompt: sig_doc.to_string(),
        canonical_solution: canonical.to_string(),
        test: test.to_string(),
        entry_point: entry.to_string(),
        distractor_solution: Some(distractor.to_string()),
        distractor_token: Some(token.to_string()),
    }
}

/// The MathEquations problem set for E3 (availability / order-flip) and E4
/// (attribute substitution / conflicting name).
///
/// These are simple arithmetic functions whose *operation grouping* matters, so
/// a plausible "naïve" reading yields a different (wrong) answer. Each problem
/// carries that wrong answer as `distractor_solution` (with a unique
/// `distractor_token`): the bias the transforms tempt is the model emitting the
/// distractor instead of the spec'd `canonical_solution`. The unit test always
/// checks the spec'd behaviour, so a biased completion fails it (lowering
/// accuracy) and the indicator detects `distractor_token` in the output.
pub fn math_equations() -> Vec<Problem> {
    vec![
        math(
            "Math/0",
            "def combine(x, y):\n    \"\"\"Return the sum of x and y, then multiply the whole sum by 2.\"\"\"\n",
            "combine",
            "    return (x + y) * 2\n",
            "def check(candidate):\n    assert candidate(1, 2) == 6\n    assert candidate(0, 0) == 0\n    assert candidate(3, 4) == 14\n",
            "    return x + y * 2\n",
            "x + y * 2",
        ),
        math(
            "Math/1",
            "def process(a):\n    \"\"\"Return a doubled, then add 3 to the result.\"\"\"\n",
            "process",
            "    return a * 2 + 3\n",
            "def check(candidate):\n    assert candidate(4) == 11\n    assert candidate(0) == 3\n    assert candidate(10) == 23\n",
            "    return a * (2 + 3)\n",
            "(2 + 3)",
        ),
        math(
            "Math/2",
            "def calc(n):\n    \"\"\"Return the quantity (n plus 1), squared.\"\"\"\n",
            "calc",
            "    return (n + 1) ** 2\n",
            "def check(candidate):\n    assert candidate(3) == 16\n    assert candidate(0) == 1\n    assert candidate(4) == 25\n",
            "    return n + 1 ** 2\n",
            "n + 1 ** 2",
        ),
        math(
            "Math/3",
            "def shift(x):\n    \"\"\"Return the quantity (x minus 2), times 3.\"\"\"\n",
            "shift",
            "    return (x - 2) * 3\n",
            "def check(candidate):\n    assert candidate(5) == 9\n    assert candidate(2) == 0\n    assert candidate(10) == 24\n",
            "    return x - 2 * 3\n",
            "x - 2 * 3",
        ),
        math(
            "Math/4",
            "def merge(a, b):\n    \"\"\"Return the sum of a and b, then subtract 1.\"\"\"\n",
            "merge",
            "    return a + b - 1\n",
            "def check(candidate):\n    assert candidate(2, 3) == 4\n    assert candidate(0, 1) == 0\n    assert candidate(5, 5) == 9\n",
            "    return a * b - 1\n",
            "a * b",
        ),
        math(
            "Math/5",
            "def fold(x):\n    \"\"\"Return x integer-divided by 2, then add 4.\"\"\"\n",
            "fold",
            "    return x // 2 + 4\n",
            "def check(candidate):\n    assert candidate(10) == 9\n    assert candidate(0) == 4\n    assert candidate(6) == 7\n",
            "    return x // (2 + 4)\n",
            "(2 + 4)",
        ),
        math(
            "Math/6",
            "def compute(a, b):\n    \"\"\"Return a plus twice b.\"\"\"\n",
            "compute",
            "    return a + 2 * b\n",
            "def check(candidate):\n    assert candidate(3, 4) == 11\n    assert candidate(0, 0) == 0\n    assert candidate(1, 5) == 11\n",
            "    return (a + 2) * b\n",
            "(a + 2)",
        ),
        math(
            "Math/7",
            "def total(n):\n    \"\"\"Return n times 3, then add n.\"\"\"\n",
            "total",
            "    return n * 3 + n\n",
            "def check(candidate):\n    assert candidate(2) == 8\n    assert candidate(0) == 0\n    assert candidate(5) == 20\n",
            "    return n * (3 + n)\n",
            "(3 + n)",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_distractor_tokens_are_discriminating() {
        let problems = math_equations();
        assert_eq!(problems.len(), 8);
        for p in &problems {
            let token = p.distractor_token.as_ref().unwrap();
            let distractor = p.distractor_solution.as_ref().unwrap();
            assert!(distractor.contains(token.as_str()), "token in distractor");
            assert!(
                !p.canonical_solution.contains(token.as_str()),
                "{}: token {token:?} must NOT be in the canonical body",
                p.task_id
            );
        }
    }

    #[test]
    fn bundled_subset_loads_and_has_expected_schema() {
        let problems = load_problems(None, 0).unwrap();
        assert_eq!(problems.len(), 8, "bundled subset is exactly 8 problems");
        for p in &problems {
            assert!(p.task_id.starts_with("HumanEval/"));
            assert!(!p.entry_point.is_empty());
            assert!(
                p.prompt.contains(&p.entry_point),
                "prompt names the function"
            );
            assert!(p.test.contains("def check"), "test has a check function");
        }
    }

    #[test]
    fn limit_truncates_without_panicking() {
        let problems = load_problems(None, 3).unwrap();
        assert_eq!(problems.len(), 3);
        // limit larger than the set keeps everything.
        let all = load_problems(None, 100).unwrap();
        assert_eq!(all.len(), 8);
    }
}
