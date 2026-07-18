//! Replaces a Hebrew sentence's Ollama-guessed word pronunciations with
//! ones derived deterministically from niqud (see
//! `docs/src/developer/specs.md`'s "Hebrew pronunciation" entry), with
//! graceful degradation at every step — word analysis never fails because
//! of this.

use niqud::NiqudClient;
use word_analysis::WordAnalysis;

/// Replaces `analysis`'s per-word `pronunciation` with niqud-derived
/// values for Hebrew sentences, leaving Ollama's own guesses in place for
/// everything else. Three-way graceful degradation, each logged with
/// `tracing::warn` rather than failing the analysis: `sentence` isn't
/// Hebrew (`niqud_client` isn't even called), the niqud CLI call itself
/// fails (e.g. not installed), or its word count doesn't match `analysis`'s
/// (Ollama's own word-splitting vs. the niqud CLI's whitespace-splitting
/// can disagree, e.g. on Hebrew's attached prefix particles).
pub fn apply_niqud_pronunciation<N: NiqudClient>(
    niqud_client: &N,
    sentence: &str,
    mut analysis: WordAnalysis,
) -> WordAnalysis {
    if !niqud::contains_hebrew(sentence) {
        return analysis;
    }

    match niqud_client.transliterate_sentence(sentence) {
        Ok(niqud_result) if niqud_result.words.len() == analysis.words.len() => {
            for (word, niqud_word) in analysis.words.iter_mut().zip(niqud_result.words) {
                word.pronunciation = niqud_word.pronunciation;
            }
        }
        Ok(niqud_result) => {
            tracing::warn!(
                ollama_words = analysis.words.len(),
                niqud_words = niqud_result.words.len(),
                %sentence,
                "niqud word count did not match Ollama's, keeping Ollama's pronunciation guesses"
            );
        }
        Err(err) => {
            tracing::warn!(%err, %sentence, "niqud transliteration failed, keeping Ollama's pronunciation guesses");
        }
    }

    analysis
}

#[cfg(test)]
mod tests {
    use niqud::{NiqudError, NiqudResult, NiqudWord};
    use word_analysis::WordEntry;

    use super::*;

    /// A `NiqudClient` test double whose `transliterate_sentence` returns a
    /// fixed result (or error) and records every call count, plus a flag
    /// proving whether it was called at all — enough to check the
    /// non-Hebrew short-circuit without any real subprocess.
    #[derive(Default)]
    struct FakeNiqudClient {
        calls: std::sync::Mutex<u32>,
        result: std::sync::Mutex<Option<Result<NiqudResult, NiqudError>>>,
    }

    impl FakeNiqudClient {
        fn returning(result: Result<NiqudResult, NiqudError>) -> Self {
            Self {
                calls: std::sync::Mutex::new(0),
                result: std::sync::Mutex::new(Some(result)),
            }
        }

        fn call_count(&self) -> u32 {
            *self.calls.lock().unwrap()
        }
    }

    impl NiqudClient for FakeNiqudClient {
        fn transliterate_sentence(&self, _sentence: &str) -> Result<NiqudResult, NiqudError> {
            *self.calls.lock().unwrap() += 1;
            self.result
                .lock()
                .unwrap()
                .take()
                .expect("transliterate_sentence called more than once in this test")
        }
    }

    fn word_analysis(word: &str, pronunciation: &str) -> WordAnalysis {
        WordAnalysis {
            words: vec![WordEntry {
                word: word.to_string(),
                translation: "translated".to_string(),
                pronunciation: pronunciation.to_string(),
            }],
        }
    }

    #[test]
    fn test_non_hebrew_sentence_never_calls_the_niqud_client() {
        // Given: a Spanish sentence and a niqud client that would panic if
        //        actually called (its fixed result is consumed on the
        //        first call, so a second call would panic on the None)
        // When:  applying niqud pronunciation
        // Then:  the analysis is returned unchanged and the client was
        //        never called
        let client = FakeNiqudClient::returning(Ok(NiqudResult { words: vec![] }));
        let analysis = word_analysis("hola", "OH-lah");

        let result = apply_niqud_pronunciation(&client, "hola", analysis.clone());

        assert_eq!(result, analysis);
        assert_eq!(client.call_count(), 0);
    }

    #[test]
    fn test_hebrew_sentence_with_matching_word_count_replaces_pronunciation() {
        // Given: a Hebrew sentence whose Ollama analysis has one word, and
        //        a niqud client returning one matching word
        // When:  applying niqud pronunciation
        // Then:  the word's pronunciation is replaced with the niqud-derived
        //        one, translation is untouched
        let client = FakeNiqudClient::returning(Ok(NiqudResult {
            words: vec![NiqudWord {
                word: "שכב".to_string(),
                niqud: "שָׁכַב".to_string(),
                pronunciation: "sha-khav".to_string(),
            }],
        }));
        let analysis = word_analysis("שכב", "shkach");

        let result = apply_niqud_pronunciation(&client, "שכב", analysis);

        assert_eq!(result.words[0].pronunciation, "sha-khav");
        assert_eq!(result.words[0].translation, "translated");
        assert_eq!(client.call_count(), 1);
    }

    #[test]
    fn test_mismatched_word_count_keeps_ollamas_pronunciation() {
        // Given: a Hebrew sentence whose Ollama analysis has one word, but
        //        the niqud client returns two (a word-splitting disagreement)
        // When:  applying niqud pronunciation
        // Then:  Ollama's original pronunciation is kept, not overwritten
        //        with something misaligned
        let client = FakeNiqudClient::returning(Ok(NiqudResult {
            words: vec![
                NiqudWord {
                    word: "ו".to_string(),
                    niqud: "וְ".to_string(),
                    pronunciation: "ve".to_string(),
                },
                NiqudWord {
                    word: "אמר".to_string(),
                    niqud: "אָמַר".to_string(),
                    pronunciation: "a-mar".to_string(),
                },
            ],
        }));
        let analysis = word_analysis("ואמר", "u-amar");

        let result = apply_niqud_pronunciation(&client, "ואמר", analysis);

        assert_eq!(result.words[0].pronunciation, "u-amar");
    }

    #[test]
    fn test_niqud_client_error_keeps_ollamas_pronunciation() {
        // Given: a Hebrew sentence and a niqud client that fails (e.g. no
        //        model configured)
        // When:  applying niqud pronunciation
        // Then:  Ollama's original pronunciation is kept, and no panic
        let client = FakeNiqudClient::returning(Err(NiqudError::ModelLoadFailed(
            "niqud model not configured".to_string(),
        )));
        let analysis = word_analysis("שכב", "shkach");

        let result = apply_niqud_pronunciation(&client, "שכב", analysis);

        assert_eq!(result.words[0].pronunciation, "shkach");
    }
}
