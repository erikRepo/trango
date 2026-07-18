//! Parses the niqud CLI wrapper's stdout (see `tools/niqud-cli/`) into a
//! `NiqudResult`, computing each word's pronunciation guide from its
//! niqud text (`transliterate::niqud_to_pronunciation`) rather than
//! trusting the CLI to provide one.

use serde::Deserialize;

use crate::entry::{NiqudResult, NiqudWord};
use crate::error::NiqudError;
use crate::transliterate::niqud_to_pronunciation;

/// The niqud CLI wrapper's stdout shape: `{"words": [{"word": "...",
/// "niqud": "..."}]}`. No `pronunciation` field — that's computed here,
/// not by the CLI.
#[derive(Debug, Deserialize)]
struct CliOutput {
    words: Vec<CliWord>,
}

/// One entry in `CliOutput`'s `words` array.
#[derive(Debug, Deserialize)]
struct CliWord {
    word: String,
    niqud: String,
}

/// Parses `raw_stdout` (the niqud CLI wrapper's full stdout) into a
/// `NiqudResult`, deriving each word's `pronunciation` from its `niqud`
/// text.
pub fn parse_cli_output(raw_stdout: &str) -> Result<NiqudResult, NiqudError> {
    let output: CliOutput = serde_json::from_str(raw_stdout.trim())
        .map_err(|err| NiqudError::InvalidResponse(err.to_string()))?;
    Ok(NiqudResult {
        words: output
            .words
            .into_iter()
            .map(|word| NiqudWord {
                pronunciation: niqud_to_pronunciation(&word.niqud),
                word: word.word,
                niqud: word.niqud,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli_output_derives_pronunciation_from_niqud() {
        // Given: the CLI wrapper's stdout shape for a single word
        // When:  parsing it
        // Then:  the word/niqud fields pass through and pronunciation is
        //        computed from the niqud text, not read from the CLI
        let raw = r#"{"words":[{"word":"שכב","niqud":"שָׁכַב"}]}"#;

        let result = parse_cli_output(raw).unwrap();

        assert_eq!(
            result,
            NiqudResult {
                words: vec![NiqudWord {
                    word: "שכב".to_string(),
                    niqud: "שָׁכַב".to_string(),
                    pronunciation: "sha-khav".to_string(),
                }]
            }
        );
    }

    #[test]
    fn test_parse_cli_output_handles_multiple_words_in_order() {
        // Given: the CLI wrapper's stdout for a whole sentence
        // When:  parsing it
        // Then:  words come back in source order, each with its own
        //        derived pronunciation
        let raw = r#"{"words":[{"word":"שלום","niqud":"שָׁלוֹם"},{"word":"עולם","niqud":"עוֹלָם"}]}"#;

        let result = parse_cli_output(raw).unwrap();

        assert_eq!(result.words.len(), 2);
        assert_eq!(result.words[0].pronunciation, "sha-lom");
    }

    #[test]
    fn test_parse_cli_output_rejects_invalid_json() {
        // Given: stdout that isn't valid JSON at all
        // When:  parsing it
        // Then:  an InvalidResponse error comes back, not a panic
        let result = parse_cli_output("not json at all");

        assert!(matches!(result, Err(NiqudError::InvalidResponse(_))));
    }
}
