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
}
