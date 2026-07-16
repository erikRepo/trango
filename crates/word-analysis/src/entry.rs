//! The `WordAnalysis` data model: a sentence broken into per-word
//! translation/pronunciation entries, as returned by a local Ollama model
//! (see `ollama::OllamaClient::analyze_sentence`) and persisted via
//! `cache::AnalysisCache`.

use serde::{Deserialize, Serialize};

/// A single word from an analyzed sentence, with its translation and a
/// phonetic pronunciation guide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordEntry {
    /// The word exactly as it appears in the source sentence.
    pub word: String,
    /// The word's meaning in the target language.
    pub translation: String,
    /// A phonetic pronunciation guide, readable by a target-language
    /// speaker (e.g. "shalom" for Hebrew "שלום").
    pub pronunciation: String,
}

/// A whole sentence's word-by-word analysis, in source-sentence order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordAnalysis {
    /// The sentence's words, in the order they appear in the source text.
    pub words: Vec<WordEntry>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_word_analysis_serializes_to_the_expected_json_shape() {
        // Given: a WordAnalysis with one word
        // When:  serializing it to JSON
        // Then:  the shape matches what the Ollama prompt asks the model
        //        to produce — {"words":[{"word","translation","pronunciation"}]}
        let analysis = WordAnalysis {
            words: vec![WordEntry {
                word: "שלום".to_string(),
                translation: "hello".to_string(),
                pronunciation: "shalom".to_string(),
            }],
        };

        let json = serde_json::to_string(&analysis).unwrap();

        assert_eq!(
            json,
            r#"{"words":[{"word":"שלום","translation":"hello","pronunciation":"shalom"}]}"#
        );
    }

    #[test]
    fn test_word_analysis_round_trips_through_json() {
        // Given: a WordAnalysis with multiple words
        // When:  serializing then deserializing it
        // Then:  the result is unchanged
        let analysis = WordAnalysis {
            words: vec![
                WordEntry {
                    word: "hola".to_string(),
                    translation: "hi".to_string(),
                    pronunciation: "OH-lah".to_string(),
                },
                WordEntry {
                    word: "mundo".to_string(),
                    translation: "world".to_string(),
                    pronunciation: "MOON-doh".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&analysis).unwrap();
        let round_tripped: WordAnalysis = serde_json::from_str(&json).unwrap();

        assert_eq!(round_tripped, analysis);
    }
}
