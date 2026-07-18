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

    /// Analyzes `words` — already split onto exact word boundaries by the
    /// caller, rather than left for the model to decide — using `model`,
    /// asking for a `translation`/`pronunciation`/`parts` breakdown for
    /// each one. Used instead of `analyze_sentence` when the caller
    /// already knows a sentence's exact word boundaries (e.g. Hebrew
    /// sentences use niqud's own whitespace tokenization — see
    /// `crates/app/src/word_analysis.rs`), since getting an LLM to
    /// reproduce an exact word count/order for a sentence it segments
    /// itself is far less reliable than asking it to fill in blanks for a
    /// list it was already given.
    fn analyze_words(
        &self,
        model: &str,
        words: &[String],
        target_language: &str,
    ) -> Result<WordAnalysis, OllamaError>;
}

/// Whether `text` contains Hebrew script (Unicode block U+0590-U+05FF).
/// Mirrors `niqud::contains_hebrew`, duplicated here rather than adding a
/// crate dependency for one predicate — word-analysis and niqud are
/// independent sibling crates, both used directly by crates/app, and
/// this is the same small-duplication tradeoff already made elsewhere
/// (e.g. `run_command`/`last_stderr_line` between `subtitle` and
/// `niqud`).
fn contains_hebrew(text: &str) -> bool {
    text.chars().any(|c| ('\u{0590}'..='\u{05FF}').contains(&c))
}

/// Extra guidance appended to the prompt for Hebrew sentences, asking for
/// a `"parts"` breakdown of single-letter prefix particles (ו/ה/ב/כ/ל/
/// מ/ש, written attached to the following word with no space) rather
/// than splitting them into separate top-level word entries the way an
/// earlier version of this prompt asked for.
///
/// That earlier approach was wrong on two counts, both found through
/// real use: it's not how the word actually sounds (a prefixed word is
/// pronounced as one fused unit in speech, e.g. "לסרטים" as "le-sratim",
/// not two separate sounds) — and it broke
/// `niqud_pronunciation::apply_niqud_pronunciation`'s word-count
/// alignment with the niqud pipeline's own whitespace-based splitting,
/// which only ever sees one whitespace-delimited unit per prefixed word.
/// Keeping `"word"`/`"pronunciation"` as the whole combined form fixes
/// both: it matches what's actually heard, and restores the 1:1
/// alignment niqud correction depends on — while `"parts"` still gives a
/// learner the grammatical breakdown each morpheme deserves.
const HEBREW_PREFIX_GUIDANCE: &str = "\n\nThis sentence is Hebrew. Hebrew attaches single-letter \
     prefix particles — ו (\"and\"), ה (\"the\"), ב (\"in\"), כ (\"as/like\"), \
     ל (\"to\"), מ (\"from\"), ש (\"that\") — directly onto the following word \
     with no space between them. Keep such a word as ONE entry: its \"word\" \
     and \"pronunciation\" must describe the whole combined form exactly as \
     it is written and actually pronounced together in speech (e.g. \
     \"לסרטים\" is pronounced as one fused word — never split \"word\" or \
     \"pronunciation\" across two entries for it). Instead, break down its \
     meaning in a \"parts\" array, one entry per morpheme, e.g. for \
     \"לסרטים\": \"parts\": [{\"word\": \"ל\", \"translation\": \"to\"}, \
     {\"word\": \"סרטים\", \"translation\": \"movies\"}]. For a word with no \
     attached prefix, \"parts\" should be an empty array. Each entry inside \
     \"parts\" is always a flat {\"word\", \"translation\"} pair — never give \
     a \"parts\" entry a \"parts\" field of its own, even for a word like \
     \"מהספרים\" that fuses two prefixes (מ + ה) onto \"ספרים\": list all three \
     morphemes as three flat, sibling entries in that one \"parts\" array, not \
     nested inside each other. These prefix particles can also stack two at \
     a time onto the SAME following word, e.g. \"כשכולם\" (כ+ש+כולם, \"when \
     everyone\"): the whole stack still stays ONE top-level entry — never \
     split the stack itself off as if it were its own top-level word (e.g. \
     never emit \"כש\" and \"כולם\" as two separate top-level entries); its \
     \"parts\" array then has one entry per stacked particle plus the base \
     word, e.g. [{\"word\": \"כ\", ...}, {\"word\": \"ש\", ...}, \
     {\"word\": \"כולם\", ...}].";

/// Builds the prompt sent to Ollama for `analyze_sentence`: asks the model
/// to split `sentence` into words and, for each, give its `translation`
/// into `target_language` and a `pronunciation` guide, replying with
/// exactly the JSON shape `WordAnalysis` deserializes from. Hebrew
/// sentences get extra guidance appended (see
/// [`HEBREW_PREFIX_GUIDANCE`]). A pure function (no I/O) so it's directly
/// testable without a running Ollama.
pub fn build_prompt(sentence: &str, target_language: &str) -> String {
    let hebrew_guidance = if contains_hebrew(sentence) {
        HEBREW_PREFIX_GUIDANCE
    } else {
        ""
    };
    format!(
        "You are a language-learning assistant. Break the following sentence \
         into its individual words, in the order they appear. Output exactly \
         one entry per word token in the sentence, in that exact order and \
         count — subtitle sentences often repeat a word or a whole phrase \
         one or more times, immediately or later in the sentence; when that \
         happens, output a separate entry for every single occurrence, never \
         merging, deduplicating, or silently dropping any of them. Do not add \
         any entry that isn't literally one of the sentence's own words (no \
         punctuation-only entries, no summary or \"end of sentence\" \
         entries). For each word, provide:\n\
         - \"word\": the word exactly as it appears in the sentence\n\
         - \"translation\": its meaning in {target_language}\n\
         - \"pronunciation\": a simple phonetic pronunciation guide readable \
         by a {target_language} speaker{hebrew_guidance}\n\n\
         Respond with ONLY valid JSON in exactly this shape, no other text:\n\
         {{\"words\": [{{\"word\": \"...\", \"translation\": \"...\", \"pronunciation\": \"...\"}}]}}\n\n\
         Sentence: \"{sentence}\""
    )
}

/// Builds the prompt sent to Ollama for `analyze_words`: gives the model
/// `words` — already split onto exact word boundaries by the caller — as
/// a fixed, order-preserving JSON array and asks it to add a
/// `translation`/`pronunciation` (and, for Hebrew, a `parts` breakdown) to
/// each one, without adding, dropping, merging, splitting, or reordering
/// any entry. Word segmentation is by far the least reliable part of
/// `build_prompt`'s free-text prompt (real use has needed repeated
/// wording fixes and still drifts, e.g. over-splitting Hebrew's attached
/// prefix particles) — handing the model a pre-split, count-fixed list
/// turns "get the segmentation right" into "fill in these blanks", which
/// token-based LLMs are far more reliable at. A pure function (no I/O), so
/// it's directly testable without a running Ollama.
pub fn build_prompt_for_words(words: &[String], target_language: &str) -> String {
    let hebrew_guidance = if words.iter().any(|word| contains_hebrew(word)) {
        HEBREW_PREFIX_GUIDANCE
    } else {
        ""
    };
    let words_json = serde_json::to_string(words).unwrap_or_default();
    let count = words.len();
    format!(
        "You are a language-learning assistant. Here is a JSON array of {count} words, \
         already split from a sentence and given in order: {words_json}\n\n\
         For each word in this array, output exactly one entry, in that exact order and \
         count — the output \"words\" array must have exactly {count} entries, one per \
         given word. Never add, drop, merge, split, reorder, or deduplicate any entry, \
         even if a word repeats one or more times in the array. For each word, provide:\n\
         - \"word\": copy the given word exactly, unchanged\n\
         - \"translation\": its meaning in {target_language}\n\
         - \"pronunciation\": a simple phonetic pronunciation guide readable \
         by a {target_language} speaker{hebrew_guidance}\n\n\
         Respond with ONLY valid JSON in exactly this shape, no other text:\n\
         {{\"words\": [{{\"word\": \"...\", \"translation\": \"...\", \"pronunciation\": \"...\"}}]}}"
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
/// sequence. `think: false` disables extended reasoning for models that
/// support it (e.g. the `qwen3` family): without it, a reasoning model can
/// spend its entire generation budget on internal "thinking" tokens and
/// leave `response` empty, which otherwise surfaces as a confusing JSON
/// parse error rather than a clear "model returned nothing" one — the
/// same reasoning gemhunter's `call_ollama` (the sibling project this was
/// modeled on) sets `"think": False` for.
#[derive(Debug, Serialize)]
struct GenerateRequest<'a> {
    model: &'a str,
    prompt: String,
    stream: bool,
    format: &'a str,
    think: bool,
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
        let prompt = build_prompt(sentence, target_language);
        let request_body = GenerateRequest {
            model,
            prompt: prompt.clone(),
            stream: false,
            format: "json",
            think: false,
        };
        tracing::debug!(%model, %prompt, "sending Ollama analyze_sentence request");
        let response: GenerateResponse = ureq::post(&url)
            .send_json(&request_body)
            .map_err(map_ureq_error)?
            .body_mut()
            .read_json()
            .map_err(|err| OllamaError::InvalidResponse(err.to_string()))?;
        tracing::debug!(response = %response.response, "received Ollama analyze_sentence response");
        if response.response.trim().is_empty() {
            return Err(OllamaError::InvalidResponse(
                "Ollama returned an empty response (the model may have spent its whole \
                 budget \"thinking\" with no final answer — try a different model)"
                    .to_string(),
            ));
        }
        parse_analysis_response(&response.response)
    }

    fn analyze_words(
        &self,
        model: &str,
        words: &[String],
        target_language: &str,
    ) -> Result<WordAnalysis, OllamaError> {
        let url = format!("{}/api/generate", self.base_url);
        let prompt = build_prompt_for_words(words, target_language);
        let request_body = GenerateRequest {
            model,
            prompt: prompt.clone(),
            stream: false,
            format: "json",
            think: false,
        };
        tracing::debug!(%model, %prompt, "sending Ollama analyze_words request");
        let response: GenerateResponse = ureq::post(&url)
            .send_json(&request_body)
            .map_err(map_ureq_error)?
            .body_mut()
            .read_json()
            .map_err(|err| OllamaError::InvalidResponse(err.to_string()))?;
        tracing::debug!(response = %response.response, "received Ollama analyze_words response");
        if response.response.trim().is_empty() {
            return Err(OllamaError::InvalidResponse(
                "Ollama returned an empty response (the model may have spent its whole \
                 budget \"thinking\" with no final answer — try a different model)"
                    .to_string(),
            ));
        }
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
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

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
    fn test_build_prompt_omits_hebrew_guidance_for_other_languages() {
        // Given/When: building a prompt for a non-Hebrew sentence
        // Then:  the Hebrew-specific prefix-splitting guidance is absent,
        //        keeping the prompt short for languages it doesn't apply to
        let prompt = build_prompt("hola mundo", "English");

        assert!(!prompt.contains("prefix particles"));
    }

    #[test]
    fn test_build_prompt_adds_hebrew_prefix_guidance_for_hebrew_sentences() {
        // Given/When: building a prompt for a Hebrew sentence
        // Then:  the guidance is included, naming a concrete example the
        //        model can generalize from, and asks for a "parts"
        //        breakdown rather than splitting "word"/"pronunciation"
        //        themselves (see HEBREW_PREFIX_GUIDANCE's doc comment for
        //        why an earlier version did the latter and was wrong)
        let prompt = build_prompt("איך הספרים הופכים לסרטים", "English");

        assert!(prompt.contains("prefix particles"));
        assert!(prompt.contains("לסרטים"));
        assert!(prompt.contains("\"parts\""));
    }

    #[test]
    fn test_build_prompt_warns_that_prefix_particles_can_stack() {
        // Given/When: building a prompt for a Hebrew sentence
        // Then:  the guidance calls out that two single-letter prefixes can
        //        stack onto one word (e.g. כש = כ+ש, "when") and must still
        //        stay ONE top-level entry — found through real use returning
        //        "כש"/"כולם" as two separate top-level entries for "כשכולם",
        //        the same mistake the single-prefix guidance already fixed
        //        for one-letter cases
        let prompt = build_prompt("כשכולם ישנים", "English");

        assert!(prompt.contains("כשכולם"));
        assert!(prompt.contains("stack"));
    }

    #[test]
    fn test_build_prompt_for_words_includes_given_words_and_target_language() {
        // Given/When: building a words-prompt for a fixed word list
        // Then:  the given words and target language appear in the prompt,
        //        as a JSON array rather than free text
        let prompt = build_prompt_for_words(&["hola".to_string(), "mundo".to_string()], "English");

        assert!(prompt.contains(r#"["hola","mundo"]"#));
        assert!(prompt.contains("English"));
        assert!(prompt.contains("\"words\""));
    }

    #[test]
    fn test_build_prompt_for_words_states_the_exact_required_count() {
        // Given/When: building a words-prompt for three given words
        // Then:  the prompt states "3" as the exact number of output
        //        entries required, so the model can't drift the count the
        //        way it can with a free-text sentence to split itself
        let prompt = build_prompt_for_words(
            &["a".to_string(), "b".to_string(), "c".to_string()],
            "English",
        );

        assert!(prompt.contains("exactly 3 entries"));
    }

    #[test]
    fn test_build_prompt_for_words_omits_hebrew_guidance_for_other_languages() {
        // Given/When: building a words-prompt for non-Hebrew words
        // Then:  the Hebrew-specific prefix-splitting guidance is absent
        let prompt = build_prompt_for_words(&["hola".to_string(), "mundo".to_string()], "English");

        assert!(!prompt.contains("prefix particles"));
    }

    #[test]
    fn test_build_prompt_for_words_adds_hebrew_prefix_guidance_when_any_word_is_hebrew() {
        // Given/When: building a words-prompt where one of the given words
        //             is Hebrew
        // Then:  the Hebrew prefix-splitting guidance is included, even
        //        though it only needs to check individual words rather
        //        than a whole free-text sentence
        let prompt = build_prompt_for_words(&["איך".to_string(), "לסרטים".to_string()], "English");

        assert!(prompt.contains("prefix particles"));
        assert!(prompt.contains("\"parts\""));
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
                    parts: Vec::new(),
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
        let (base_url, _request_rx) = spawn_mock_json_server_capturing_request(response_body);
        base_url
    }

    /// Like `spawn_mock_json_server`, but also hands back a receiver
    /// carrying the raw bytes of whatever request the server received —
    /// used to assert on the request body `HttpOllamaClient` actually
    /// sends (e.g. that `analyze_sentence` sets `"think":false`), not just
    /// on how it handles the response.
    fn spawn_mock_json_server_capturing_request(
        response_body: &'static str,
    ) -> (String, mpsc::Receiver<Vec<u8>>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind mock server");
        let addr = listener
            .local_addr()
            .expect("failed to read mock server addr");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                // A single read() can return only the request's headers if
                // the client's headers and body land in separate TCP
                // segments (observed with ureq's real request writes,
                // unlike the small fixed responses this mock server sends
                // back) — loop with a short read timeout instead of
                // assuming one read() captures the whole request.
                let _ = stream.set_read_timeout(Some(Duration::from_millis(300)));
                let mut captured = Vec::new();
                let mut buf = [0u8; 4096];
                loop {
                    match stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => captured.extend_from_slice(&buf[..n]),
                        Err(_) => break,
                    }
                    if captured.len() >= buf.len() {
                        break;
                    }
                }
                let _ = tx.send(captured);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        (format!("http://{addr}"), rx)
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
    fn test_analyze_sentence_sends_think_false() {
        // Given: a mock server capturing the raw request it receives
        // When:  analyzing a sentence
        // Then:  the request body includes "think":false — without this,
        //        reasoning-capable models (e.g. qwen3) can spend their
        //        whole generation budget "thinking" and leave the actual
        //        response empty
        let (base_url, request_rx) = spawn_mock_json_server_capturing_request(
            r#"{"model":"qwen3","response":"{\"words\":[]}","done":true}"#,
        );
        let client = HttpOllamaClient::new(base_url);

        client.analyze_sentence("qwen3", "hola", "English").unwrap();

        let request_bytes = request_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("mock server did not capture a request in time");
        let request_text = String::from_utf8_lossy(&request_bytes);
        let body_text = request_text
            .split_once("\r\n\r\n")
            .map(|(_headers, body)| body)
            .expect("request had no header/body separator");
        let body: serde_json::Value =
            serde_json::from_str(body_text).expect("request body was not valid JSON");
        assert_eq!(body["think"], false, "request body: {body_text}");
    }

    #[test]
    fn test_analyze_sentence_reports_an_empty_response_clearly() {
        // Given: a mock server whose /api/generate response has an empty
        //        "response" field — e.g. a reasoning model that spent its
        //        whole budget "thinking" with nothing left for the answer
        // When:  analyzing a sentence against it
        // Then:  a clear InvalidResponse error comes back, not the raw
        //        "EOF while parsing a value" serde_json message
        let base_url = spawn_mock_json_server(r#"{"model":"qwen3","response":"","done":true}"#);
        let client = HttpOllamaClient::new(base_url);

        let result = client.analyze_sentence("qwen3", "hola", "English");

        match result {
            Err(OllamaError::InvalidResponse(message)) => {
                assert!(message.contains("empty"), "unexpected message: {message}");
            }
            other => panic!("expected InvalidResponse, got {other:?}"),
        }
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
