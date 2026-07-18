//! Merges Ollama's response to being given niqud's own word list (see
//! `word_analysis::analyze_hebrew_sentence`) onto niqud's word boundaries
//! and pronunciations — see `docs/src/developer/specs.md`'s "Hebrew
//! pronunciation" entry for why niqud, not Ollama, is the source of truth
//! for Hebrew pronunciation.

use niqud::NiqudResult;
use word_analysis::WordAnalysis;

use crate::hebrew_word_merge::merge_by_niqud_boundaries;

/// Merges `analysis` — Ollama's response to being given `niqud_result`'s
/// own words as a fixed, order-preserving list to translate — onto
/// `niqud_result`'s word boundaries and pronunciations. Ollama is asked
/// not to add, drop, merge, split, or reorder any entry, but real use has
/// still occasionally seen it disagree with the count it was given (e.g.
/// re-splitting an attached Hebrew prefix particle anyway); when that
/// happens this logs a `tracing::warn` and reconciles as much of the
/// sentence as it can via [`merge_by_niqud_boundaries`] instead of
/// discarding the correction entirely.
pub fn apply_niqud_pronunciation(
    niqud_result: &NiqudResult,
    analysis: WordAnalysis,
) -> WordAnalysis {
    if analysis.words.len() != niqud_result.words.len() {
        tracing::warn!(
            ollama_words = analysis.words.len(),
            niqud_words = niqud_result.words.len(),
            "Ollama's response word count did not match the niqud word list it was given, \
             merging onto niqud's word boundaries where possible"
        );
    }
    WordAnalysis {
        words: merge_by_niqud_boundaries(analysis.words, &niqud_result.words),
    }
}

#[cfg(test)]
mod tests {
    use niqud::NiqudWord;
    use word_analysis::WordEntry;

    use super::*;

    fn niqud_word(word: &str, pronunciation: &str) -> NiqudWord {
        NiqudWord {
            word: word.to_string(),
            niqud: word.to_string(),
            pronunciation: pronunciation.to_string(),
        }
    }

    fn word_analysis(word: &str, pronunciation: &str) -> WordAnalysis {
        WordAnalysis {
            words: vec![WordEntry {
                word: word.to_string(),
                translation: "translated".to_string(),
                pronunciation: pronunciation.to_string(),
                parts: Vec::new(),
            }],
        }
    }

    #[test]
    fn test_matching_word_count_replaces_pronunciation() {
        // Given: a niqud result with one word, and Ollama's own analysis
        //        of that same one word (as it was asked to)
        // When:  applying niqud pronunciation
        // Then:  the word's pronunciation is replaced with the
        //        niqud-derived one, translation is untouched
        let niqud_result = NiqudResult {
            words: vec![niqud_word("שכב", "sha-khav")],
        };
        let analysis = word_analysis("שכב", "shkach");

        let result = apply_niqud_pronunciation(&niqud_result, analysis);

        assert_eq!(result.words[0].pronunciation, "sha-khav");
        assert_eq!(result.words[0].translation, "translated");
    }

    #[test]
    fn test_mismatched_word_count_still_merges_onto_niqud_boundaries() {
        // Given: a niqud result with two words, but Ollama's response
        //        split them into an extra entry ("ו"+"אמר" instead of a
        //        single "ואמר") despite being given "ואמר" as one word
        // When:  applying niqud pronunciation
        // Then:  merge_by_niqud_boundaries still reconciles it into one
        //        correctly fused entry rather than the mismatch being
        //        left uncorrected
        let niqud_result = NiqudResult {
            words: vec![niqud_word("ואמר", "ve-a-mar")],
        };
        let analysis = WordAnalysis {
            words: vec![
                WordEntry {
                    word: "ו".to_string(),
                    translation: "and".to_string(),
                    pronunciation: "vav".to_string(),
                    parts: Vec::new(),
                },
                WordEntry {
                    word: "אמר".to_string(),
                    translation: "said".to_string(),
                    pronunciation: "amar".to_string(),
                    parts: Vec::new(),
                },
            ],
        };

        let result = apply_niqud_pronunciation(&niqud_result, analysis);

        assert_eq!(result.words.len(), 1);
        assert_eq!(result.words[0].word, "ואמר");
        assert_eq!(result.words[0].pronunciation, "ve-a-mar");
    }
}
