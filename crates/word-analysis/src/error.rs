//! Error types for the word-analysis crate.

use thiserror::Error;

/// Errors that can occur while talking to a local Ollama instance or
/// interpreting its response.
#[derive(Debug, Error)]
pub enum OllamaError {
    /// The HTTP request to Ollama could not be sent at all (e.g. Ollama
    /// isn't running, or the configured `base_url` is unreachable).
    #[error("failed to reach Ollama: {0}")]
    ConnectionFailed(String),

    /// Ollama responded, but with a non-success HTTP status.
    #[error("Ollama returned HTTP {status}")]
    Http {
        /// The HTTP status code Ollama responded with.
        status: u16,
    },

    /// Ollama's response body couldn't be parsed as the expected JSON shape
    /// — either the outer `/api/generate`/`/api/tags` envelope, or (for
    /// `analyze_sentence`) the `WordAnalysis` JSON the prompt asked the
    /// model to produce.
    #[error("failed to parse Ollama response: {0}")]
    InvalidResponse(String),
}
