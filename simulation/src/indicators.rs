//! Failure indicators `φ` (jones2022 §3.3, design §1 step 4 / §4.3(2)).
//!
//! An indicator tests whether a model's **completion** exhibits the target
//! failure feature. Crucially, `φ` is evaluated on the *generated text only*,
//! never on the assembled program — otherwise the transform's own injected lines
//! (the IPF body / anchor function) would trivially satisfy it.
//!
//! - [`framing_hit`] (E1) — the framing line appears **verbatim** in the output
//!   (the model copied the irrelevant preceding function's body).
//! - [`anchor_hit`] (E2) — an anchor line (`for var in` / `print(var)` /
//!   `return tmp`) appears in the output (the model anchored on the distractor).

use crate::config::FramingLine;

/// E1 indicator: did the completion copy the framing line verbatim?
pub fn framing_hit(completion: &str, line: &FramingLine) -> bool {
    completion.contains(line.code)
}

/// E2 indicator: does the completion contain the given anchor line?
pub fn anchor_hit(completion: &str, anchor_code: &str) -> bool {
    completion.contains(anchor_code)
}

/// E3/E4 indicator: did the completion emit the problem's distractor (the
/// "available" naïve answer / the named-function variant)? `token` is the
/// problem's `distractor_token`. Returns `false` when the problem has no
/// distractor (e.g. a HumanEval problem mis-routed here).
pub fn distractor_hit(completion: &str, token: Option<&str>) -> bool {
    token.is_some_and(|t| completion.contains(t))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ANCHOR_LINES, FRAMING_LINES};

    #[test]
    fn framing_hit_detects_verbatim_copy() {
        let line = &FRAMING_LINES[0]; // raise NotImplementedError
        assert!(framing_hit("    raise NotImplementedError\n", line));
        assert!(!framing_hit("    return len(s)\n", line));
    }

    #[test]
    fn anchor_hit_detects_each_anchor_line() {
        let completion = "    for var in data:\n        print(var)\n    return tmp\n";
        for (_, code) in ANCHOR_LINES {
            assert!(anchor_hit(completion, code));
        }
        assert!(!anchor_hit("    return sorted(x)\n", "for var in"));
    }

    #[test]
    fn distractor_hit_detects_token() {
        assert!(distractor_hit("    return x + y * 2\n", Some("x + y * 2")));
        assert!(!distractor_hit(
            "    return (x + y) * 2\n",
            Some("x + y * 2")
        ));
        assert!(!distractor_hit("    return 1\n", None));
    }
}
