//! Error types for the subtitle crate.

use std::time::Duration;

use thiserror::Error;

/// Errors that can occur while parsing or validating subtitle data.
#[derive(Debug, Error)]
pub enum SubtitleError {
    /// The subtitle content did not match the expected format.
    #[error("invalid subtitle format: {0}")]
    InvalidFormat(String),

    /// Reading a subtitle file from disk failed.
    #[error("failed to read subtitle file: {0}")]
    IoError(#[from] std::io::Error),

    /// A cue's end time was not strictly after its start time.
    #[error("cue {index}: end time ({end:?}) must be after start time ({start:?})")]
    InvalidTiming {
        /// Index of the offending cue.
        index: u32,
        /// The cue's start time.
        start: Duration,
        /// The cue's end time.
        end: Duration,
    },

    /// Running an external speech-to-text tool (e.g. `whisper-cli`, see
    /// `WhisperCliGenerator`) failed — covers the binary not being found,
    /// the process exiting with an error, or it finishing without
    /// producing the expected output file. The message is meant to be
    /// shown to the user as-is, so it should already explain what went
    /// wrong and, where possible, how to fix it.
    #[error("{0}")]
    GenerationFailed(String),
}
