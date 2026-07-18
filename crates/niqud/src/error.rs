//! Error types for the niqud crate.

use thiserror::Error;

/// Errors that can occur while loading or running the niqud ONNX model.
#[derive(Debug, Error)]
pub enum NiqudError {
    /// The ONNX model or tokenizer.json couldn't be loaded: missing file,
    /// invalid JSON, a malformed/incompatible model, or (for
    /// `Option<OnnxNiqudClient>`) no model path configured at all.
    #[error("failed to load niqud model: {0}")]
    ModelLoadFailed(String),

    /// The ONNX Runtime session failed while running inference.
    #[error("niqud inference failed: {0}")]
    InferenceFailed(String),
}
