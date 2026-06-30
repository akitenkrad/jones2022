//! jones2022 — replicating Jones & Steinhardt (2022), "Capturing Failures of
//! Large Language Models via Human Cognitive Biases" (NeurIPS 2022).
//!
//! The paper induces qualitative failures of code-generation LLMs with
//! **semantic-preserving input transforms** seeded by human cognitive biases,
//! then measures the **sensitivity** `Δ` (accuracy drop) and the **indicator
//! rate** `r` (does the output carry the target failure feature `φ`?). It is a
//! black-box, logprob-free probe pipeline — *not* an ABM tick loop — so, like
//! the sibling LLM-eval replication gong2026, it pulls in **no**
//! `socsim-core`/engine/grid/net. Every piece of plumbing is delegated to the
//! consolidated socsim crates; the only jones2022-specific logic is the
//! transforms, the failure indicators, and the execution sandbox.
//!
//! | Concern | Delegated to |
//! |---|---|
//! | LLM code generation (logprob-free) | `socsim-llm` |
//! | Mean / accuracy aggregation | `socsim-metrics::stats` |
//! | Paper-anchor PASS/off reproduction | `socsim-reproduce` |
//! | Timestamped results + CSV/JSON | `socsim-results` |
//!
//! The [`LlmClient`](socsim_llm::LlmClient) is **injected** into [`eval`] as
//! `&dyn LlmClient`, so the whole pipeline is exercised offline with
//! `socsim_llm::mock::ScriptedClient` — no live model.

pub mod anchors;
pub mod config;
pub mod datasets;
pub mod eval;
pub mod filedelete;
pub mod gpt3;
pub mod indicators;
pub mod mock;
pub mod sandbox;
pub mod transforms;

pub use anchors::PAPER_ANCHORS;
pub use config::{Config, Experiment};
pub use datasets::{load_humaneval, load_problems, math_equations, math_equations_n, Problem};
pub use eval::{run_experiment, MetricRow};
pub use filedelete::run_file_deletion;
pub use gpt3::{run_gpt3_anchoring, run_gpt3_framing};
pub use mock::{
    build_file_deletion_mock, build_gpt3_anchoring_mock, build_gpt3_framing_mock, build_mock_client,
};
