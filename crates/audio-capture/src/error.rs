//! Error types for the audio-capture crate.

use thiserror::Error;

/// Errors that can occur while capturing system audio.
#[derive(Debug, Error)]
pub enum AudioCaptureError {
    /// Running or waiting on an external tool (`pactl`, `ffmpeg`) failed —
    /// covers the binary not being found, the process exiting with an
    /// error, or `pactl` reporting no default sink. The message is meant
    /// to be shown to the user as-is, so it should already explain what
    /// went wrong and, where possible, how to fix it.
    #[error("{0}")]
    CaptureFailed(String),

    /// `AudioCapture::start` was called while a capture was already
    /// running.
    #[error("audio capture is already running")]
    AlreadyRunning,

    /// `AudioCapture::stop` was called while no capture was running.
    #[error("audio capture is not running")]
    NotRunning,
}
