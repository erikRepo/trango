//! Word-by-word sentence analysis via a local Ollama instance, for
//! trango's Ctrl+A popup and "Analyze all sentences" batch loop.
//!
//! Provides the `WordAnalysis`/`WordEntry` data model, `AnalysisCache` for
//! persisting analyses to a JSON sidecar file next to the subtitle they
//! belong to (`cache_path_for`, `load_cache`, `save_cache`), and the
//! `OllamaClient` trait with an `ureq`-backed `HttpOllamaClient`
//! implementation for listing installed models and analyzing a sentence.

mod cache;
mod entry;
mod error;
mod ollama;

pub use cache::{cache_path_for, load_cache, save_cache, AnalysisCache};
pub use entry::{WordAnalysis, WordEntry};
pub use error::OllamaError;
pub use ollama::{build_prompt, HttpOllamaClient, OllamaClient};
