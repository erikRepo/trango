//! Hebrew niqud diacritization and deterministic Latin pronunciation
//! guides, for trango's word-analysis popup (`crates/app/src/
//! niqud_pronunciation.rs`).
//!
//! Ollama's own LLM-guessed pronunciation is unreliable for Hebrew (see
//! `docs/src/developer/specs.md`'s "Hebrew pronunciation" entry). This
//! crate instead shells out to a local niqud/diacritization tool
//! (`process_client::PhonikudCliClient`) and derives the pronunciation
//! guide from its output deterministically, without any further LLM call.

mod hebrew_detect;

pub use hebrew_detect::contains_hebrew;
