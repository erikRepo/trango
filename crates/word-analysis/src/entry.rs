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
    /// speaker (e.g. "shalom" for Hebrew "שלום"). Always describes
    /// `word` as a whole, exactly as it's actually pronounced together in
    /// speech — for a word made of multiple morphemes (see `parts`),
    /// this is the fused pronunciation of the combined form, never a
    /// per-morpheme breakdown, since that's not how it sounds when
    /// spoken.
    pub pronunciation: String,
    /// A breakdown of `word` into its own translatable morphemes, for
    /// words that combine an attachable prefix particle with a following
    /// word (e.g. Hebrew's לסרטים = ל "to" + סרטים "movies", written
    /// with no space between them). Empty for words with nothing to
    /// break down — the overwhelming majority of words in any language.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parts: Vec<WordPart>,
}

/// One morpheme within a [`WordEntry`] that combines multiple
/// translatable parts — see [`WordEntry::parts`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WordPart {
    /// This part's own text, e.g. "ל".
    pub word: String,
    /// This part's own meaning, e.g. "to".
    pub translation: String,
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
        // Given: a WordAnalysis with one word and no parts
        // When:  serializing it to JSON
        // Then:  the shape matches what the Ollama prompt asks the model
        //        to produce — {"words":[{"word","translation","pronunciation"}]}
        //        — "parts" is omitted entirely when empty, so a simple
        //        word's cached JSON doesn't grow for no reason
        let analysis = WordAnalysis {
            words: vec![WordEntry {
                word: "שלום".to_string(),
                translation: "hello".to_string(),
                pronunciation: "shalom".to_string(),
                parts: Vec::new(),
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
                    parts: Vec::new(),
                },
                WordEntry {
                    word: "mundo".to_string(),
                    translation: "world".to_string(),
                    pronunciation: "MOON-doh".to_string(),
                    parts: Vec::new(),
                },
            ],
        };

        let json = serde_json::to_string(&analysis).unwrap();
        let round_tripped: WordAnalysis = serde_json::from_str(&json).unwrap();

        assert_eq!(round_tripped, analysis);
    }

    #[test]
    fn test_word_entry_with_parts_round_trips_and_serializes_parts() {
        // Given: a WordEntry for a prefixed Hebrew word, broken into parts
        // When:  serializing then deserializing it
        // Then:  "parts" appears in the JSON and survives the round trip
        let entry = WordEntry {
            word: "לסרטים".to_string(),
            translation: "to the movies".to_string(),
            pronunciation: "le-sratim".to_string(),
            parts: vec![
                WordPart {
                    word: "ל".to_string(),
                    translation: "to".to_string(),
                },
                WordPart {
                    word: "סרטים".to_string(),
                    translation: "movies".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"parts\""));

        let round_tripped: WordEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(round_tripped, entry);
    }

    #[test]
    fn test_word_entry_without_parts_field_deserializes_as_empty() {
        // Given: JSON with no "parts" field at all (e.g. a cache file
        //        written before this field existed, or a non-Hebrew
        //        word Ollama never mentions it for)
        // When:  deserializing it
        // Then:  parts defaults to empty rather than failing to parse
        let json = r#"{"word":"hola","translation":"hi","pronunciation":"OH-lah"}"#;

        let entry: WordEntry = serde_json::from_str(json).unwrap();

        assert_eq!(entry.parts, Vec::new());
    }
}
