//! Hebrew niqud diacritization and deterministic Latin pronunciation
//! guides, for trango's word-analysis popup (`crates/app/src/
//! niqud_pronunciation.rs`).
//!
//! Ollama's own LLM-guessed pronunciation is unreliable for Hebrew (see
//! `docs/src/developer/specs.md`'s "Hebrew pronunciation" entry). This
//! crate instead runs a real ONNX niqud-diacritization model directly via
//! `ort` (`onnx_client::OnnxNiqudClient` — `tokenizer.rs` + `decode.rs`),
//! no subprocess, and derives the pronunciation guide from its output
//! deterministically, without any further LLM call.

mod client;
mod decode;
mod entry;
mod error;
mod hebrew_detect;
mod onnx_client;
mod tokenizer;
mod transliterate;

pub use client::NiqudClient;
pub use decode::decode;
pub use entry::{NiqudResult, NiqudWord};
pub use error::NiqudError;
pub use hebrew_detect::contains_hebrew;
pub use onnx_client::OnnxNiqudClient;
pub use tokenizer::{strip_niqud, tokenize, Token, Vocab};
pub use transliterate::niqud_to_pronunciation;
