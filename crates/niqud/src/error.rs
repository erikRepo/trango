//! Error types for the niqud crate.

use thiserror::Error;

/// Errors that can occur while running the niqud CLI or interpreting its
/// output.
#[derive(Debug, Error)]
pub enum NiqudError {
    /// The CLI process couldn't be run at all, exited with a non-zero
    /// status, or otherwise failed — the message is ready to show as-is
    /// (mirrors `subtitle::SubtitleError::GenerationFailed`'s single-variant
    /// convention for external-process errors).
    #[error("{0}")]
    ProcessFailed(String),

    /// The CLI's stdout wasn't the expected `{"words": [{"word",
    /// "niqud"}]}` JSON shape.
    #[error("failed to parse niqud CLI output: {0}")]
    InvalidResponse(String),
}
