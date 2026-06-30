//! Semantic-preserving input transforms `T` (jones2022 §3.3, design §1 step 3).
//!
//! Each transform prepends a **complete, valid, but unused** function before the
//! real HumanEval prompt, leaving the function the model must complete entirely
//! unchanged — so the transform is semantic-preserving for the *target* task,
//! yet biases the model:
//!
//! - [`framing`] (E1) prepends an *irrelevant preceding function* (IPF) whose
//!   body is a salient framing line; the model is biased into copying that line.
//! - [`anchoring`] (E2) prepends an *anchor function* showing a distractor
//!   coding pattern (`for var in` / `print(var)` / `return tmp`) the model
//!   anchors on.
//!
//! The IPF / anchor function carry **stable marker names** (`IPF_FN_NAME` /
//! `ANCHOR_FN_NAME`) so the offline mock client can recognise a transformed
//! prompt deterministically; the names are part of the legitimate transform (a
//! real model sees them too), not a mock-only hack.

use crate::config::FramingLine;

/// Name of the irrelevant preceding function injected by [`framing`].
pub const IPF_FN_NAME: &str = "irrelevant_helper";

/// Name of the anchor function injected by [`anchoring`].
pub const ANCHOR_FN_NAME: &str = "example_helper";

/// Identity transform: the baseline (control) pass leaves the prompt unchanged.
pub fn identity(prompt: &str) -> String {
    prompt.to_string()
}

/// E1 framing: prepend an IPF whose single-statement body is `line.code`, then
/// the original prompt. The IPF is syntactically complete and never called.
pub fn framing(prompt: &str, line: &FramingLine) -> String {
    format!("def {IPF_FN_NAME}(value):\n    {}\n\n\n{prompt}", line.code)
}

/// E2 anchoring: prepend an anchor function demonstrating the distractor pattern
/// (`for var in` / `print(var)` / `return tmp`), then the original prompt.
pub fn anchoring(prompt: &str) -> String {
    format!(
        "def {ANCHOR_FN_NAME}(data):\n    tmp = []\n    for var in data:\n        print(var)\n    return tmp\n\n\n{prompt}"
    )
}

/// Marker comment injected by [`order_flip`]; the mock keys on it to recognise
/// an E3 transform (a real model just reads it as a misleading hint).
pub const ORDER_FLIP_MARKER: &str = "# hint: apply unary operations before binary ones";

/// Marker comment injected by [`conflicting_name`] for E4.
pub const CONFLICTING_NAME_MARKER: &str = "# note: implement this as the common library variant";

/// E3 availability / order-flip (MathEquations): the spec and unit test are
/// **unchanged** (semantic-preserving for the required task); only a misleading
/// operation-order hint is prepended, tempting the model toward the more
/// "available" naïve grouping (the problem's `distractor_solution`).
pub fn order_flip(prompt: &str) -> String {
    format!("{ORDER_FLIP_MARKER}\n{prompt}")
}

/// E4 attribute substitution / conflicting name (MathEquations): a hint is
/// prepended claiming the function is conventionally the "common library
/// variant", substituting the salient name-association for the spec'd behaviour.
/// The spec/test are unchanged; a biased model emits the `distractor_solution`.
///
/// (Design §3.3.4 renames the function to a conflicting name; against the real
/// fixed-`entry_point`/fixed-test harness we keep the signature and inject the
/// conflict as a prompt hint instead — a documented deviation that preserves
/// test consistency while still inducing the named-function bias.)
pub fn conflicting_name(prompt: &str) -> String {
    format!("{CONFLICTING_NAME_MARKER}\n{prompt}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ANCHOR_LINES, FRAMING_LINES};

    #[test]
    fn framing_prepends_ipf_with_the_framing_line() {
        let line = &FRAMING_LINES[0]; // raise NotImplementedError
        let out = framing("def f():\n    pass\n", line);
        assert!(out.contains(&format!("def {IPF_FN_NAME}(")));
        assert!(out.contains(line.code));
        // The original prompt is still present, unchanged, after the IPF.
        assert!(out.trim_end().ends_with("def f():\n    pass"));
    }

    #[test]
    fn anchoring_prepends_all_three_anchor_lines() {
        let out = anchoring("def g():\n    pass\n");
        assert!(out.contains(&format!("def {ANCHOR_FN_NAME}(")));
        for (_, code) in ANCHOR_LINES {
            assert!(
                out.contains(code),
                "anchor function should contain {code:?}"
            );
        }
        assert!(out.contains("def g():"));
    }

    #[test]
    fn identity_is_unchanged() {
        assert_eq!(identity("abc\n"), "abc\n");
    }
}
