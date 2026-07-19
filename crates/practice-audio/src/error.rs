//! Error types for the practice-audio crate.

use thiserror::Error;

/// Errors that can occur while assembling a sentence's practice audio.
#[derive(Debug, Error)]
pub enum PracticeAudioError {
    /// Reading or writing a file failed.
    #[error("failed to read/write file: {0}")]
    IoError(#[from] std::io::Error),

    /// Running an external tool (`ffmpeg`, `espeak-ng`) failed — covers
    /// the binary not being found, the process exiting with an error, or
    /// it finishing without producing the expected output file. The
    /// message is meant to be shown to the user as-is, so it should
    /// already explain what went wrong and, where possible, how to fix
    /// it.
    #[error("{0}")]
    GenerationFailed(String),
}
