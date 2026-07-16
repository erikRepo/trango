//! A local Ollama instance (`http://localhost:11434` by default) as the
//! source of word-by-word sentence analysis: listing installed models
//! (`OllamaClient::list_models`, for the model-picker in the Open
//! Subtitles dialog) and analyzing one sentence at a time
//! (`OllamaClient::analyze_sentence`, for both the Ctrl+A popup and the
//! "Analyze all sentences" batch loop).

use serde::{Deserialize, Serialize};

use crate::entry::WordAnalysis;
use crate::error::OllamaError;

/// Talks to a local LLM server to list available models and analyze
/// sentences word-by-word. A trait rather than a concrete type so
/// `crates/app`'s tests can swap in a fixed-response fake instead of
/// depending on a real Ollama installation (mirrors
/// `subtitle::SubtitleGenerator`'s role for whisper-cli).
pub trait OllamaClient {
    /// Lists the names of models currently installed in Ollama (e.g.
    /// `"llama3.1:8b"`), for the user to pick a default from in the UI.
    fn list_models(&self) -> Result<Vec<String>, OllamaError>;

    /// Analyzes `sentence` word-by-word using `model`, asking for
    /// translations/pronunciations into `target_language`.
    fn analyze_sentence(
        &self,
        model: &str,
        sentence: &str,
        target_language: &str,
    ) -> Result<WordAnalysis, OllamaError>;
}

/// Builds the prompt sent to Ollama for `analyze_sentence`: asks the model
/// to split `sentence` into words and, for each, give its `translation`
/// into `target_language` and a `pronunciation` guide, replying with
/// exactly the JSON shape `WordAnalysis` deserializes from. A pure
/// function (no I/O) so it's directly testable without a running Ollama.
pub fn build_prompt(sentence: &str, target_language: &str) -> String {
    format!(
        "You are a language-learning assistant. Break the following sentence \
         into its individual words, in the order they appear. For each word, \
         provide:\n\
         - \"word\": the word exactly as it appears in the sentence\n\
         - \"translation\": its meaning in {target_language}\n\
         - \"pronunciation\": a simple phonetic pronunciation guide readable \
         by a {target_language} speaker\n\n\
         Respond with ONLY valid JSON in exactly this shape, no other text:\n\
         {{\"words\": [{{\"word\": \"...\", \"translation\": \"...\", \"pronunciation\": \"...\"}}]}}\n\n\
         Sentence: \"{sentence}\""
    )
}

/// Extracts a `WordAnalysis` from `raw_text` — the text Ollama's
/// `/api/generate` `response` field carries, expected to be the JSON
/// `build_prompt` asked for. Some local models still wrap their answer in
/// a ```json code fence despite `format: "json"`, so this strips one
/// before parsing (mirrors gemhunter's `call_ollama` fence-stripping).
fn parse_analysis_response(raw_text: &str) -> Result<WordAnalysis, OllamaError> {
    let trimmed = raw_text.trim();
    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let without_fence = without_fence.strip_suffix("```").unwrap_or(without_fence);
    serde_json::from_str(without_fence.trim())
        .map_err(|err| OllamaError::InvalidResponse(err.to_string()))
}

/// `POST /api/generate` request body — `stream: false` and `format:
/// "json"` so the whole answer comes back as a single JSON object with a
/// `response` string, rather than needing to reassemble a streamed NDJSON
/// sequence.
#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
    format: &'a str,
}

/// The subset of `/api/generate`'s response body this crate reads.
#[derive(Debug, Deserialize)]
struct GenerateResponse {
    response: String,
}

/// The subset of `/api/tags`'s response body this crate reads.
#[derive(Debug, Deserialize)]
struct TagsResponse {
    models: Vec<TagsModel>,
}

/// One entry in `/api/tags`'s `models` array.
#[derive(Debug, Deserialize)]
struct TagsModel {
    name: String,
}

/// `OllamaClient` implementation that talks to a real Ollama server over
/// HTTP, using `ureq` for synchronous requests — trango has no async
/// runtime, so calls made through this client are meant to run on a
/// background thread (see `crates/app/src/word_analysis.rs`), the same way
/// `subtitle::WhisperCliGenerator` runs whisper-cli off the UI thread.
pub struct HttpOllamaClient {
    /// Ollama's base URL, e.g. `http://localhost:11434`.
    pub base_url: String,
}

impl HttpOllamaClient {
    /// Builds a client pointed at `base_url` (no trailing slash expected).
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

impl Default for HttpOllamaClient {
    /// Points at Ollama's standard local address.
    fn default() -> Self {
        Self::new("http://localhost:11434")
    }
}

impl OllamaClient for HttpOllamaClient {
    fn list_models(&self) -> Result<Vec<String>, OllamaError> {
        let url = format!("{}/api/tags", self.base_url);
        let body: TagsResponse = ureq::get(&url)
            .call()
            .map_err(map_ureq_error)?
            .body_mut()
            .read_json()
            .map_err(|err| OllamaError::InvalidResponse(err.to_string()))?;
        Ok(body.models.into_iter().map(|model| model.name).collect())
    }

    fn analyze_sentence(
        &self,
        model: &str,
        sentence: &str,
        target_language: &str,
    ) -> Result<WordAnalysis, OllamaError> {
        let url = format!("{}/api/generate", self.base_url);
        let request_body = GenerateRequest {
            model,
            prompt: build_prompt(sentence, target_language),
            stream: false,
            format: "json",
        };
        let response: GenerateResponse = ureq::post(&url)
            .send_json(&request_body)
            .map_err(map_ureq_error)?
            .body_mut()
            .read_json()
            .map_err(|err| OllamaError::InvalidResponse(err.to_string()))?;
        parse_analysis_response(&response.response)
    }
}

/// Maps a `ureq::Error` to `OllamaError`: a non-2xx status becomes
/// `OllamaError::Http`, anything else (connection refused, DNS failure,
/// timeout, ...) becomes `OllamaError::ConnectionFailed`.
fn map_ureq_error(err: ureq::Error) -> OllamaError {
    match err {
        ureq::Error::StatusCode(status) => OllamaError::Http { status },
        other => OllamaError::ConnectionFailed(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;
    use crate::entry::WordEntry;

    #[test]
    fn test_build_prompt_includes_sentence_and_target_language() {
        // Given/When: building a prompt for a sentence and target language
        // Then:  both appear in the resulting prompt text
        let prompt = build_prompt("hola mundo", "English");

        assert!(prompt.contains("hola mundo"));
        assert!(prompt.contains("English"));
        assert!(prompt.contains("\"words\""));
    }

    #[test]
    fn test_parse_analysis_response_accepts_plain_json() {
        // Given: a raw response with no code fence
        // When:  parsing it
        // Then:  it deserializes into the expected WordAnalysis
        let raw = r#"{"words":[{"word":"hola","translation":"hi","pronunciation":"OH-lah"}]}"#;

        let analysis = parse_analysis_response(raw).unwrap();

        assert_eq!(
            analysis,
            WordAnalysis {
                words: vec![WordEntry {
                    word: "hola".to_string(),
                    translation: "hi".to_string(),
                    pronunciation: "OH-lah".to_string(),
                }]
            }
        );
    }

    #[test]
    fn test_parse_analysis_response_strips_json_code_fence() {
        // Given: a raw response wrapped in a ```json code fence, as some
        //        local models still do despite format: "json"
        // When:  parsing it
        // Then:  the fence is stripped and the JSON parses correctly
        let raw = "```json\n{\"words\":[{\"word\":\"hi\",\"translation\":\"hi\",\"pronunciation\":\"hi\"}]}\n```";

        let analysis = parse_analysis_response(raw).unwrap();

        assert_eq!(analysis.words.len(), 1);
    }

    #[test]
    fn test_parse_analysis_response_rejects_invalid_json() {
        // Given: a raw response that isn't valid JSON at all
        // When:  parsing it
        // Then:  an InvalidResponse error comes back, not a panic
        let result = parse_analysis_response("not json at all");

        assert!(matches!(result, Err(OllamaError::InvalidResponse(_))));
    }

    /// Starts a mock HTTP server on a random local port that accepts one
    /// connection, drains the request, and writes back a fixed
    /// `200 OK` JSON response — enough to exercise `HttpOllamaClient`'s
    /// request/response handling without depending on a real Ollama
    /// install. Returns the server's base URL (`http://127.0.0.1:<port>`).
    fn spawn_mock_json_server(response_body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind mock server");
        let addr = listener
            .local_addr()
            .expect("failed to read mock server addr");
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn test_list_models_parses_tags_response_from_mock_server() {
        // Given: a mock server returning a /api/tags-shaped response
        // When:  listing models against it
        // Then:  the model names come back in order
        let base_url =
            spawn_mock_json_server(r#"{"models":[{"name":"llama3.1:8b"},{"name":"gemma2:9b"}]}"#);
        let client = HttpOllamaClient::new(base_url);

        let models = client.list_models().unwrap();

        assert_eq!(
            models,
            vec!["llama3.1:8b".to_string(), "gemma2:9b".to_string()]
        );
    }

    #[test]
    fn test_analyze_sentence_parses_generate_response_from_mock_server() {
        // Given: a mock server returning a /api/generate-shaped response
        //        whose "response" field is the WordAnalysis JSON
        // When:  analyzing a sentence against it
        // Then:  the parsed WordAnalysis comes back
        let base_url = spawn_mock_json_server(
            r#"{"model":"llama3.1:8b","response":"{\"words\":[{\"word\":\"hola\",\"translation\":\"hi\",\"pronunciation\":\"OH-lah\"}]}","done":true}"#,
        );
        let client = HttpOllamaClient::new(base_url);

        let analysis = client
            .analyze_sentence("llama3.1:8b", "hola", "English")
            .unwrap();

        assert_eq!(analysis.words.len(), 1);
        assert_eq!(analysis.words[0].word, "hola");
    }

    #[test]
    fn test_list_models_connection_failure_is_reported() {
        // Given: a base URL nothing is listening on
        // When:  listing models
        // Then:  a ConnectionFailed error comes back, not a panic
        let client = HttpOllamaClient::new("http://127.0.0.1:1");

        let result = client.list_models();

        assert!(matches!(result, Err(OllamaError::ConnectionFailed(_))));
    }
}
