//! Ties `tokenizer.rs`/`decode.rs` to a real `ort::Session`, implementing
//! [`NiqudClient`] without any external process — replaces the removed
//! `PhonikudCliClient`/`tools/niqud-cli/` (see
//! `docs/src/developer/specs.md`'s "Hebrew pronunciation" entry).

use std::path::Path;
use std::sync::{Arc, Mutex};

use ort::session::Session;
use ort::value::Tensor;

use crate::client::NiqudClient;
use crate::decode::decode;
use crate::dylib::ensure_ort_initialized;
use crate::entry::{NiqudResult, NiqudWord};
use crate::error::NiqudError;
use crate::tokenizer::{strip_niqud, tokenize, Vocab};
use crate::transliterate::niqud_to_pronunciation;

/// A [`NiqudClient`] backed by a real ONNX Runtime session — no
/// subprocess, no Python. Cheap to [`Clone`] (an `Arc`-wrapped session +
/// vocab), so the model/tokenizer are loaded once and the same client can
/// be reused across every word-analysis call for the life of the process,
/// rather than reloading per call (unlike the removed CLI wrapper, which
/// paid a fresh process-startup cost every time regardless).
#[derive(Clone)]
pub struct OnnxNiqudClient {
    // ort::Session::run takes &mut self; NiqudClient::transliterate_sentence
    // takes &self (and Clone + Send + 'static, for spawn_batch_analyze/
    // spawn_analyze_sentence to move it onto a background thread) — the
    // Mutex provides the interior mutability that bridges the two.
    session: Arc<Mutex<Session>>,
    vocab: Arc<Vocab>,
}

impl OnnxNiqudClient {
    /// Loads the ONNX model at `model_path` and a `tokenizer.json`
    /// expected as a sibling file in the same folder.
    pub fn load(model_path: &Path) -> Result<Self, NiqudError> {
        ensure_ort_initialized()?;

        let tokenizer_path = model_path.with_file_name("tokenizer.json");
        let tokenizer_json = std::fs::read_to_string(&tokenizer_path).map_err(|err| {
            NiqudError::ModelLoadFailed(format!(
                "failed to read {}: {err}",
                tokenizer_path.display()
            ))
        })?;
        let vocab = Vocab::from_tokenizer_json(&tokenizer_json)?;

        let session = Session::builder()
            .map_err(|err| NiqudError::ModelLoadFailed(err.to_string()))?
            .commit_from_file(model_path)
            .map_err(|err| {
                NiqudError::ModelLoadFailed(format!(
                    "failed to load {}: {err}",
                    model_path.display()
                ))
            })?;

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            vocab: Arc::new(vocab),
        })
    }
}

impl NiqudClient for OnnxNiqudClient {
    fn transliterate_sentence(&self, sentence: &str) -> Result<NiqudResult, NiqudError> {
        tracing::debug!(%sentence, "running niqud transliterate_sentence");
        let niqud_stripped = strip_niqud(sentence);
        let tokens = tokenize(&self.vocab, &niqud_stripped);
        let seq_len = tokens.len();

        let input_ids: Vec<i64> = tokens.iter().map(|t| i64::from(t.id)).collect();
        let attention_mask: Vec<i64> = vec![1; seq_len];
        let token_type_ids: Vec<i64> = vec![0; seq_len];

        let input_ids = Tensor::from_array(([1usize, seq_len], input_ids))
            .map_err(|err| NiqudError::InferenceFailed(err.to_string()))?;
        let attention_mask = Tensor::from_array(([1usize, seq_len], attention_mask))
            .map_err(|err| NiqudError::InferenceFailed(err.to_string()))?;
        let token_type_ids = Tensor::from_array(([1usize, seq_len], token_type_ids))
            .map_err(|err| NiqudError::InferenceFailed(err.to_string()))?;

        let mut session = self
            .session
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let outputs = session
            .run(ort::inputs![
                "input_ids" => input_ids,
                "attention_mask" => attention_mask,
                "token_type_ids" => token_type_ids,
            ])
            .map_err(|err| NiqudError::InferenceFailed(err.to_string()))?;

        let (_, nikud_logits) = outputs["nikud_logits"]
            .try_extract_tensor::<f32>()
            .map_err(|err| NiqudError::InferenceFailed(err.to_string()))?;
        let (_, shin_logits) = outputs["shin_logits"]
            .try_extract_tensor::<f32>()
            .map_err(|err| NiqudError::InferenceFailed(err.to_string()))?;
        let (_, additional_logits) = outputs["additional_logits"]
            .try_extract_tensor::<f32>()
            .map_err(|err| NiqudError::InferenceFailed(err.to_string()))?;

        let diacritized = decode(
            &niqud_stripped,
            &tokens,
            nikud_logits,
            shin_logits,
            additional_logits,
        );

        let result = NiqudResult {
            words: sentence
                .split_whitespace()
                .zip(diacritized.split_whitespace())
                .map(|(word, niqud)| NiqudWord {
                    word: word.to_string(),
                    pronunciation: niqud_to_pronunciation(niqud),
                    niqud: niqud.to_string(),
                })
                .collect(),
        };
        tracing::debug!(?result, "niqud transliterate_sentence result");
        Ok(result)
    }
}

/// `None` (niqud model not configured, e.g. no path set in Settings) is a
/// valid, expected state — `apply_niqud_pronunciation`'s existing graceful
/// degradation already treats any `NiqudClient` error as "keep Ollama's
/// guess", so this needs no special handling beyond producing that error.
impl NiqudClient for Option<OnnxNiqudClient> {
    fn transliterate_sentence(&self, sentence: &str) -> Result<NiqudResult, NiqudError> {
        match self {
            Some(client) => client.transliterate_sentence(sentence),
            None => Err(NiqudError::ModelLoadFailed(
                "niqud model not configured".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_none_client_reports_not_configured() {
        // Given: no niqud model configured
        // When:  transliterating a sentence through the Option<...> impl
        // Then:  a clear ModelLoadFailed error comes back, not a panic
        let client: Option<OnnxNiqudClient> = None;

        let result = client.transliterate_sentence("שכב");

        let Err(NiqudError::ModelLoadFailed(message)) = result else {
            panic!("expected ModelLoadFailed, got {result:?}");
        };
        assert!(message.contains("not configured"));
    }

    /// Real end-to-end coverage against a real downloaded model +
    /// tokenizer.json — not run by default (`cargo test`), since no model
    /// file is committed to the repo or available in CI (same reasoning
    /// as whisper-cli/ffmpeg/Ollama not being tested against real
    /// installs). Run manually with:
    ///
    /// ```sh
    /// NIQUD_TEST_MODEL_PATH=/path/to/phonikud-1.0.int8.onnx \
    ///     cargo test -p niqud -- --ignored real_model
    /// ```
    ///
    /// (`tokenizer.json` must sit next to the model file, matching
    /// `OnnxNiqudClient::load`'s convention.) Also needs `ORT_DYLIB_PATH`
    /// pointed at a working `libonnxruntime.so` — see
    /// `docs/src/developer/technology/ort.md`.
    #[test]
    #[ignore]
    fn test_real_model_transliterates_shakhav_correctly() {
        // Given: a real downloaded model + tokenizer.json, pointed at by
        //        NIQUD_TEST_MODEL_PATH
        // When:  transliterating שכב
        // Then:  the pronunciation matches what the whole pipeline was
        //        validated against throughout development ("sha-khav")
        let model_path = std::env::var("NIQUD_TEST_MODEL_PATH")
            .expect("set NIQUD_TEST_MODEL_PATH to run this test");
        let client = OnnxNiqudClient::load(std::path::Path::new(&model_path))
            .expect("failed to load real niqud model");

        let result = client
            .transliterate_sentence("שכב")
            .expect("transliteration should succeed");

        assert_eq!(result.words.len(), 1);
        assert_eq!(result.words[0].pronunciation, "sha-khav");
    }
}
