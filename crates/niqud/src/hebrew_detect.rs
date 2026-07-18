//! Detects whether a sentence contains Hebrew script, used to gate the
//! niqud/pronunciation pipeline (`crate::onnx_client::OnnxNiqudClient`)
//! per sentence — trango has no explicit "source language" setting, so this
//! is the only signal available for which sentences niqud even applies to.

/// Whether `text` contains at least one Hebrew-alphabet character (Unicode
/// block U+0590-U+05FF: Hebrew letters, niqud points, and cantillation
/// marks). A single match is enough — trango only ever passes whole
/// subtitle-cue sentences, not mixed-language documents where a partial
/// match would be ambiguous.
pub fn contains_hebrew(text: &str) -> bool {
    text.chars().any(|c| ('\u{0590}'..='\u{05FF}').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hebrew_sentence_is_detected() {
        // Given: a sentence written entirely in Hebrew
        // When:  checking for Hebrew script
        // Then:  it is detected
        assert!(contains_hebrew("הוא שכב במיטה ואמר לעצמו"));
    }

    #[test]
    fn test_latin_sentence_is_not_detected() {
        // Given: a sentence written entirely in Latin script
        // When:  checking for Hebrew script
        // Then:  it is not detected
        assert!(!contains_hebrew("hola mundo"));
    }

    #[test]
    fn test_mixed_script_sentence_is_detected() {
        // Given: a sentence mixing Hebrew with a quoted Latin loanword
        // When:  checking for Hebrew script
        // Then:  it is detected, since at least one Hebrew character is present
        assert!(contains_hebrew("הוא אמר \"hello\" לי"));
    }

    #[test]
    fn test_empty_string_is_not_detected() {
        // Given: an empty string
        // When:  checking for Hebrew script
        // Then:  it is not detected
        assert!(!contains_hebrew(""));
    }

    #[test]
    fn test_niqud_only_text_is_detected() {
        // Given: text containing only a Hebrew diacritic (no base letter),
        //        a rare edge case but still within the Hebrew Unicode block
        // When:  checking for Hebrew script
        // Then:  it is detected
        assert!(contains_hebrew("\u{05B7}"));
    }
}
