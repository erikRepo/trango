//! Error types for the niqud crate.

use thiserror::Error;

/// Errors that can occur while running the niqud CLI or interpreting its
/// output.
// TODO(pure-rust-onnx-migration): ProcessFailed/InvalidResponse are the
// old CLI-subprocess variants, still used by process_client.rs/
// cli_output.rs until those are removed in favor of ModelLoadFailed/
// InferenceFailed below (see docs/src/developer/specs.md's "Hebrew
// pronunciation" entry).
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

    /// The ONNX model or tokenizer.json couldn't be loaded: missing file,
    /// invalid JSON, or a malformed/incompatible model.
    #[error("failed to load niqud model: {0}")]
    ModelLoadFailed(String),

    /// The ONNX Runtime session failed while running inference.
    #[error("niqud inference failed: {0}")]
    InferenceFailed(String),
}
