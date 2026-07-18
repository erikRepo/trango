//! The `NiqudClient` trait: adds niqud diacritics and a derived Latin
//! pronunciation guide to a Hebrew sentence's words.

use crate::entry::NiqudResult;
use crate::error::NiqudError;

/// Adds niqud diacritics and a derived Latin pronunciation guide to a
/// Hebrew sentence's words. A trait rather than a concrete type so
/// `crates/app`'s tests can swap in a fixed-response fake instead of
/// depending on a real niqud CLI install (mirrors
/// `word_analysis::OllamaClient`'s role for Ollama).
pub trait NiqudClient {
    /// Adds niqud to `sentence` and derives each word's pronunciation
    /// guide from it, in source-sentence order.
    fn transliterate_sentence(&self, sentence: &str) -> Result<NiqudResult, NiqudError>;
}
