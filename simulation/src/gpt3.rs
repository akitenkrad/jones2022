//! E5 / E6 — GPT-3 cognitive-bias reproductions (design §4, §5.1).
//!
//! Unlike E1–E4 (code generation scored by execution), these are **numeric /
//! choice** tasks: the model answers an estimation question with a number (E5)
//! or picks an option (E6), and the indicator reads that answer directly — there
//! is no sandbox. Both still go through the injected `&dyn LlmClient`, so the
//! offline `--mock` path drives them with a scripted client.
//!
//! - **E5 anchoring** (Jacowitz & Kahneman 1995): for anchor ratio `p`, a high
//!   anchor is `a(1+p)` and a low anchor `a(1-p)`; the indicator is whether the
//!   estimate **shifts toward** the anchor relative to the no-anchor baseline.
//!   The non-numeric ("gibberish") response rate is tracked too (the paper's
//!   41%).
//! - **E6 framing** (Tversky & Kahneman 1981, Asian Disease): the same scenario
//!   in a **save** frame vs a **die** frame; the indicator is the **risky-option
//!   choice rate** per frame.

use serde::Serialize;
use socsim_llm::{LlmClient, LlmConfig};

use crate::eval::MetricRow;

/// One numeric-estimation item: a question with a (rough) ground-truth value `a`.
#[derive(Debug, Clone)]
pub struct EstimationItem {
    /// Stable id.
    pub id: &'static str,
    /// The question text (also the mock's lookup key).
    pub question: &'static str,
    /// Ground-truth-ish anchor-free value `a`.
    pub true_value: f64,
}

/// Which anchor side a prompt carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSide {
    /// No anchor (baseline estimate).
    None,
    /// High anchor `a(1+p)` (phrased "much higher than usual").
    High,
    /// Low anchor `a(1-p)` (phrased "much lower than usual").
    Low,
}

/// The E5 estimation items (small, self-contained — numbers are illustrative).
pub fn estimation_items() -> Vec<EstimationItem> {
    vec![
        EstimationItem {
            id: "redwood_height_ft",
            question: "How tall is the tallest redwood tree, in feet?",
            true_value: 380.0,
        },
        EstimationItem {
            id: "un_member_states",
            question: "How many member states does the United Nations have?",
            true_value: 193.0,
        },
        EstimationItem {
            id: "boiling_point_c",
            question: "At what temperature does water boil at sea level, in Celsius?",
            true_value: 100.0,
        },
        EstimationItem {
            id: "moon_distance_thousand_km",
            question: "How far is the Moon from Earth, in thousands of kilometers?",
            true_value: 384.0,
        },
        EstimationItem {
            id: "piano_keys",
            question: "How many keys are on a standard piano?",
            true_value: 88.0,
        },
        EstimationItem {
            id: "marathon_km",
            question: "How many kilometers is a marathon, rounded to the nearest km?",
            true_value: 42.0,
        },
    ]
}

/// Build the estimation prompt for `item` under `side`, with anchor ratio `p`.
/// The anchor phrasing is natural ("much higher/lower than usual"); the scripted
/// mock keys on it, while a real model just reads the cue.
pub fn estimation_prompt(item: &EstimationItem, side: AnchorSide, _p: f64) -> String {
    let cue = match side {
        AnchorSide::None => "",
        AnchorSide::High => "Some people think the answer is much higher than usual. ",
        AnchorSide::Low => "Some people think the answer is much lower than usual. ",
    };
    format!("{cue}{} Respond with a single number only.", item.question)
}

/// E6 framing side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Frame {
    /// Gain frame: "200 people will be saved" vs a gamble.
    Save,
    /// Loss frame: "400 people will die" vs a gamble.
    Die,
}

impl Frame {
    fn slug(self) -> &'static str {
        match self {
            Frame::Save => "save_frame",
            Frame::Die => "die_frame",
        }
    }
}

/// Build the Asian-Disease framing prompt for `respondent` under `frame`. Option
/// **B** is the risky (gamble) option in both frames; the indicator is the rate
/// of choosing B.
pub fn framing_prompt(respondent: usize, frame: Frame) -> String {
    let body = match frame {
        Frame::Save => "Program A: 200 people will be saved. \
             Program B: a 1/3 chance that 600 people will be saved and a 2/3 chance that no one will be saved.",
        Frame::Die => "Program A: 400 people will die. \
             Program B: a 1/3 chance that no one will die and a 2/3 chance that 600 people will die.",
    };
    format!(
        "Respondent #{respondent}. A disease is expected to kill 600 people. Two programs are proposed. {body} \
         Which program do you favor? Answer with a single letter, A or B."
    )
}

/// Parse the first number out of free text (E5). `None` ⇒ gibberish.
pub fn parse_number(text: &str) -> Option<f64> {
    let mut cur = String::new();
    let mut found: Option<f64> = None;
    for ch in text.chars() {
        if ch.is_ascii_digit() || ch == '.' || (ch == '-' && cur.is_empty()) {
            cur.push(ch);
        } else if !cur.is_empty() {
            if let Ok(v) = cur.parse::<f64>() {
                found = Some(v);
                break;
            }
            cur.clear();
        }
    }
    found.or_else(|| cur.parse::<f64>().ok())
}

/// Parse an A/B choice (E6). Returns `Some(true)` if the risky option B is
/// chosen, `Some(false)` for A, `None` if unparseable.
pub fn parse_choice(text: &str) -> Option<bool> {
    for ch in text.chars() {
        match ch.to_ascii_uppercase() {
            'A' => return Some(false),
            'B' => return Some(true),
            _ => {}
        }
    }
    None
}

fn query(client: &dyn LlmClient, prompt: &str, seed: u64) -> String {
    let cfg = LlmConfig::deterministic()
        .with_system("You are a careful survey respondent. Answer concisely.")
        .with_max_tokens(32)
        .with_seed(seed);
    client
        .complete(prompt, &cfg)
        .map(|r| r.text)
        .unwrap_or_default()
}

/// A long-format E5/E6 row mirroring [`MetricRow`] but for direct-answer tasks
/// (no functional accuracy). Reuses the shared [`MetricRow`] schema with
/// `functional_accuracy`/`delta` = NaN so one CSV schema covers all experiments.
fn answer_row(experiment: &str, variant: &str, n: usize, rate: f64) -> MetricRow {
    MetricRow {
        experiment: experiment.to_string(),
        variant: variant.to_string(),
        condition: "transform".to_string(),
        n,
        functional_accuracy: f64::NAN,
        indicator_rate: rate,
        delta: f64::NAN,
    }
}

/// E5: for each item, compare anchored estimates to the baseline and report the
/// toward-anchor update rate per side plus the gibberish rate.
pub fn run_gpt3_anchoring(client: &dyn LlmClient, p: f64, seed: u64) -> Vec<MetricRow> {
    let items = estimation_items();
    let (mut high_updates, mut low_updates) = (0usize, 0usize);
    let (mut high_n, mut low_n) = (0usize, 0usize);
    let (mut gibberish, mut total) = (0usize, 0usize);

    for item in &items {
        let base = query(client, &estimation_prompt(item, AnchorSide::None, p), seed);
        let high = query(client, &estimation_prompt(item, AnchorSide::High, p), seed);
        let low = query(client, &estimation_prompt(item, AnchorSide::Low, p), seed);
        for r in [&base, &high, &low] {
            total += 1;
            if parse_number(r).is_none() {
                gibberish += 1;
            }
        }
        let (b, h, l) = (parse_number(&base), parse_number(&high), parse_number(&low));
        if let (Some(b), Some(h)) = (b, h) {
            high_n += 1;
            if h > b {
                high_updates += 1;
            }
        }
        if let (Some(b), Some(l)) = (b, l) {
            low_n += 1;
            if l < b {
                low_updates += 1;
            }
        }
    }

    let rate = |num: usize, den: usize| {
        if den == 0 {
            f64::NAN
        } else {
            num as f64 / den as f64
        }
    };
    vec![
        answer_row(
            "gpt3-anchoring",
            "high_anchor",
            high_n,
            rate(high_updates, high_n),
        ),
        answer_row(
            "gpt3-anchoring",
            "low_anchor",
            low_n,
            rate(low_updates, low_n),
        ),
        answer_row("gpt3-anchoring", "gibberish", total, rate(gibberish, total)),
    ]
}

/// E6: risky-option (B) choice rate per frame over `n_respondents`.
pub fn run_gpt3_framing(client: &dyn LlmClient, n_respondents: usize, seed: u64) -> Vec<MetricRow> {
    let mut rows = Vec::new();
    for frame in [Frame::Save, Frame::Die] {
        let (mut risky, mut n) = (0usize, 0usize);
        for i in 0..n_respondents {
            let resp = query(client, &framing_prompt(i, frame), seed);
            if let Some(is_risky) = parse_choice(&resp) {
                n += 1;
                if is_risky {
                    risky += 1;
                }
            }
        }
        let rate = if n == 0 {
            f64::NAN
        } else {
            risky as f64 / n as f64
        };
        rows.push(answer_row("gpt3-framing", frame.slug(), n, rate));
    }
    rows
}

/// Serializable record of the E5 anchor values (written to config.json for
/// provenance: which `a(1±p)` anchors a run used).
#[derive(Debug, Serialize)]
pub struct AnchorRecord {
    pub id: &'static str,
    pub true_value: f64,
    pub low: f64,
    pub high: f64,
}

/// Compute the `a(1±p)` anchor values for the estimation items (for config.json).
pub fn anchor_records(p: f64) -> Vec<AnchorRecord> {
    estimation_items()
        .into_iter()
        .map(|it| AnchorRecord {
            id: it.id,
            true_value: it.true_value,
            low: it.true_value * (1.0 - p),
            high: it.true_value * (1.0 + p),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_number_extracts_first_number() {
        assert_eq!(parse_number("about 380 feet"), Some(380.0));
        assert_eq!(parse_number("I think 1.5 million"), Some(1.5));
        assert_eq!(parse_number("no idea"), None);
    }

    #[test]
    fn parse_choice_reads_a_or_b() {
        assert_eq!(parse_choice("I choose B"), Some(true));
        assert_eq!(parse_choice("A"), Some(false));
        assert_eq!(parse_choice("hmm"), None);
    }

    #[test]
    fn anchor_records_bracket_the_true_value() {
        let recs = anchor_records(0.5);
        let r = recs.iter().find(|r| r.id == "piano_keys").unwrap();
        assert!(r.low < r.true_value && r.true_value < r.high);
    }
}
