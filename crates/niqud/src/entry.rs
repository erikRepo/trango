//! The niqud pipeline's data model: a sentence broken into per-word niqud
//! (diacritized) text plus the Latin pronunciation guide derived from it
//! (`transliterate::niqud_to_pronunciation`).

use serde::{Deserialize, Serialize};

/// One word's niqud (diacritized) form and the Latin pronunciation guide
/// derived from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NiqudWord {
    /// The word exactly as it appeared in the source sentence (no niqud).
    pub word: String,
    /// The word with niqud diacritics added by the niqud CLI.
    pub niqud: String,
    /// A hyphenated Latin pronunciation guide, computed deterministically
    /// from `niqud` (see `transliterate::niqud_to_pronunciation`) — never
    /// guessed by an LLM.
    pub pronunciation: String,
}

/// A whole sentence's word-by-word niqud result, in source-sentence order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NiqudResult {
    /// The sentence's words, in the order they appear in the source text.
    pub words: Vec<NiqudWord>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_niqud_result_round_trips_through_json() {
        // Given: a NiqudResult with one word
        // When:  serializing then deserializing it
        // Then:  the result is unchanged
        let result = NiqudResult {
            words: vec![NiqudWord {
                word: "שכב".to_string(),
                niqud: "שָׁכַב".to_string(),
                pronunciation: "sha-khav".to_string(),
            }],
        };

        let json = serde_json::to_string(&result).unwrap();
        let round_tripped: NiqudResult = serde_json::from_str(&json).unwrap();

        assert_eq!(round_tripped, result);
    }
}
