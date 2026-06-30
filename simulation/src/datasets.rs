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
///
/// This returns the curated 8-problem set. For a larger set (the paper used ~90
/// per setting) use [`math_equations_n`], which keeps these 8 as a stable prefix
/// and appends deterministically generated problems.
pub fn math_equations() -> Vec<Problem> {
    curated_math_equations()
}

/// MathEquations scaled to `count` problems, deterministically from `seed`.
///
/// The curated [`math_equations`] set is the stable prefix; problems beyond it
/// are synthesized by [`generate_math_problem`] (operator-precedence templates).
/// `count <= 8` returns the curated set truncated (the generator is not invoked,
/// so `seed` is irrelevant there). `count` is clamped to at least 1.
///
/// Every generated problem upholds the same invariants as the curated ones: the
/// `distractor_token` occurs in `distractor_solution` and never in
/// `canonical_solution`, and the unit test asserts the *canonical* value on
/// inputs where the canonical and distractor readings disagree (so a biased
/// completion provably fails).
pub fn math_equations_n(count: usize, seed: u64) -> Vec<Problem> {
    let count = count.max(1);
    let mut problems = curated_math_equations();
    if count <= problems.len() {
        problems.truncate(count);
        return problems;
    }
    let mut rng = SplitMix64::new(seed);
    let mut idx = problems.len();
    while problems.len() < count {
        problems.push(generate_math_problem(idx, &mut rng));
        idx += 1;
    }
    problems
}

/// Deterministic SplitMix64 — avoids an `rand` dependency and mirrors the
/// in-repo preference for tiny hand-rolled hashing (cf. `mock::exhibits_bias`).
struct SplitMix64(u64);

impl SplitMix64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform integer in `[lo, hi]` (inclusive).
    fn between(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }
}

const NUM_TEMPLATES: u64 = 5;

/// Single-arg test inputs probed for a discriminating value (canonical ≠ distractor).
const CAND1: [i64; 8] = [2, 3, 4, 5, 6, 7, 8, 9];
/// Two-arg test inputs (all with the first arg ≠ 0 and second ≠ 1, so the
/// templates' discriminating conditions hold for at least three of them).
const CAND2: [(i64, i64); 8] = [
    (1, 2),
    (2, 3),
    (3, 2),
    (4, 3),
    (2, 5),
    (5, 2),
    (3, 4),
    (6, 3),
];

/// Pick up to 3 single-arg test cases where the two readings disagree.
fn cases1(disc: impl Fn(i64) -> bool, expected: impl Fn(i64) -> i64) -> Vec<(String, i64)> {
    CAND1
        .iter()
        .copied()
        .filter(|&n| disc(n))
        .take(3)
        .map(|n| (format!("{n}"), expected(n)))
        .collect()
}

/// Pick up to 3 two-arg test cases where the two readings disagree.
fn cases2(
    disc: impl Fn(i64, i64) -> bool,
    expected: impl Fn(i64, i64) -> i64,
) -> Vec<(String, i64)> {
    CAND2
        .iter()
        .copied()
        .filter(|&(x, y)| disc(x, y))
        .take(3)
        .map(|(x, y)| (format!("{x}, {y}"), expected(x, y)))
        .collect()
}

/// Assemble a generated MathEquations problem. `dist_expr` doubles as the
/// `distractor_token` — by construction it lacks `canonical`'s parentheses, so
/// it can never be a substring of the canonical body.
fn assemble_math(
    idx: usize,
    fname: &str,
    params: &str,
    doc: &str,
    canon_expr: &str,
    dist_expr: &str,
    cases: &[(String, i64)],
) -> Problem {
    let mut test = String::from("def check(candidate):\n");
    for (args, expected) in cases {
        test.push_str(&format!("    assert candidate({args}) == {expected}\n"));
    }
    Problem {
        task_id: format!("Math/{idx}"),
        prompt: format!("def {fname}({params}):\n    \"\"\"{doc}\"\"\"\n"),
        canonical_solution: format!("    return {canon_expr}\n"),
        test,
        entry_point: fname.to_string(),
        distractor_solution: Some(format!("    return {dist_expr}\n")),
        distractor_token: Some(dist_expr.to_string()),
    }
}

/// Synthesize one operator-precedence MathEquations problem with a unique
/// function name `op{idx}`. The template and its constants come from `rng`, so
/// the same seed reproduces the same problem.
fn generate_math_problem(idx: usize, rng: &mut SplitMix64) -> Problem {
    let fname = format!("op{idx}");
    match rng.next_u64() % NUM_TEMPLATES {
        // (x + y) * k  vs  x + y * k   (disagree when x ≠ 0)
        0 => {
            let k = rng.between(2, 6);
            let canon = move |x: i64, y: i64| (x + y) * k;
            let dist = move |x: i64, y: i64| x + y * k;
            let cases = cases2(move |x, y| canon(x, y) != dist(x, y), canon);
            assemble_math(
                idx,
                &fname,
                "x, y",
                &format!("Return the sum of x and y, then multiply the whole sum by {k}."),
                &format!("(x + y) * {k}"),
                &format!("x + y * {k}"),
                &cases,
            )
        }
        // n * 2 + k  vs  n * (2 + k)   (disagree when n ≠ 1)
        1 => {
            let k = rng.between(2, 6);
            let canon = move |n: i64| n * 2 + k;
            let dist = move |n: i64| n * (2 + k);
            let cases = cases1(move |n| canon(n) != dist(n), canon);
            assemble_math(
                idx,
                &fname,
                "n",
                &format!("Return n doubled, then add {k} to the result."),
                &format!("n * 2 + {k}"),
                &format!("n * (2 + {k})"),
                &cases,
            )
        }
        // (n + k) ** 2  vs  n + k ** 2   (disagree when n ≠ 0)
        2 => {
            let k = rng.between(1, 5);
            let canon = move |n: i64| (n + k) * (n + k);
            let dist = move |n: i64| n + k * k;
            let cases = cases1(move |n| canon(n) != dist(n), canon);
            assemble_math(
                idx,
                &fname,
                "n",
                &format!("Return the quantity (n plus {k}), squared."),
                &format!("(n + {k}) ** 2"),
                &format!("n + {k} ** 2"),
                &cases,
            )
        }
        // (x - k) * m  vs  x - k * m   (disagree when x ≠ 0)
        3 => {
            let k = rng.between(2, 5);
            let m = rng.between(2, 5);
            let canon = move |x: i64| (x - k) * m;
            let dist = move |x: i64| x - k * m;
            let cases = cases1(move |x| canon(x) != dist(x), canon);
            assemble_math(
                idx,
                &fname,
                "x",
                &format!("Return the quantity (x minus {k}), times {m}."),
                &format!("(x - {k}) * {m}"),
                &format!("x - {k} * {m}"),
                &cases,
            )
        }
        // a + k * b  vs  (a + k) * b   (disagree when a ≠ 0 and b ≠ 1)
        _ => {
            let k = rng.between(2, 6);
            let canon = move |a: i64, b: i64| a + k * b;
            let dist = move |a: i64, b: i64| (a + k) * b;
            let cases = cases2(move |a, b| canon(a, b) != dist(a, b), canon);
            assemble_math(
                idx,
                &fname,
                "a, b",
                &format!("Return a plus {k} times b."),
                &format!("a + {k} * b"),
                &format!("(a + {k}) * b"),
                &cases,
            )
        }
    }
}

/// The curated, hand-written 8-problem MathEquations set (stable across
/// releases; the deterministic prefix of [`math_equations_n`]).
fn curated_math_equations() -> Vec<Problem> {
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
    fn generated_math_satisfies_invariants() {
        let curated_ids: Vec<String> = curated_math_equations()
            .iter()
            .map(|p| p.task_id.clone())
            .collect();
        for seed in [1u64, 42, 7, 1000] {
            let problems = math_equations_n(40, seed);
            assert_eq!(problems.len(), 40);
            // The curated 8 are the stable prefix.
            let prefix: Vec<String> = problems[..8].iter().map(|p| p.task_id.clone()).collect();
            assert_eq!(prefix, curated_ids);
            // task_ids are unique (mock lookup keys on the prompt; distinct ids
            // mean distinct fnames mean distinct prompts).
            let ids: std::collections::HashSet<&String> =
                problems.iter().map(|p| &p.task_id).collect();
            assert_eq!(ids.len(), 40, "seed {seed}: task_ids must be unique");
            for p in &problems {
                let token = p.distractor_token.as_ref().unwrap();
                let dist = p.distractor_solution.as_ref().unwrap();
                assert!(dist.contains(token.as_str()), "{}: token in distractor", p.task_id);
                assert!(
                    !p.canonical_solution.contains(token.as_str()),
                    "{}: token {token:?} must NOT be in the canonical body",
                    p.task_id
                );
                assert!(
                    p.test.contains("assert candidate"),
                    "{}: at least one assertion",
                    p.task_id
                );
                assert!(p.prompt.contains(&p.entry_point), "{}: prompt names fn", p.task_id);
                assert!(p.test.starts_with("def check"), "{}: has check fn", p.task_id);
            }
        }
    }

    #[test]
    fn generated_math_is_deterministic() {
        let a = math_equations_n(30, 123);
        let b = math_equations_n(30, 123);
        let c = math_equations_n(30, 124);
        assert_eq!(a.len(), 30);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.prompt, y.prompt);
            assert_eq!(x.test, y.test);
            assert_eq!(x.canonical_solution, y.canonical_solution);
            assert_eq!(x.distractor_solution, y.distractor_solution);
        }
        // A different seed changes the generated tail (not the curated prefix).
        assert_ne!(
            a[8..].iter().map(|p| p.prompt.clone()).collect::<Vec<_>>(),
            c[8..].iter().map(|p| p.prompt.clone()).collect::<Vec<_>>(),
            "different seeds should differ somewhere in the generated tail"
        );
    }

    #[test]
    fn math_count_clamps_and_truncates() {
        assert_eq!(math_equations_n(0, 1).len(), 1); // clamped to >= 1
        assert_eq!(math_equations_n(3, 1).len(), 3);
        assert_eq!(math_equations_n(8, 1).len(), 8);
        assert_eq!(math_equations_n(9, 1).len(), 9);
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
