//! Subtitle parsing and data model for trango.
//!
//! Provides the `Cue` data model, `SubtitleError`, `parse_srt` for parsing
//! `.srt` subtitle files into `Vec<Cue>`, `merge_translation` for
//! attaching a second (translation) subtitle track to an already-parsed
//! `Vec<Cue>`, the `SubtitleGenerator` trait for generating a subtitle
//! file from a video — `StubSubtitleGenerator` (a fixed-text placeholder)
//! and `WhisperCliGenerator` (drives the external `whisper-cli` binary) —
//! and `WhisperCliWordSegmenter` for deriving per-word audio timing
//! within an already-known sentence span.

mod cue;
mod error;
mod generate;
mod merge;
mod srt;
mod word_timing;

pub use cue::Cue;
pub use error::SubtitleError;
pub use generate::{StubSubtitleGenerator, SubtitleGenerator, WhisperCliGenerator};
pub use merge::merge_translation;
pub use srt::parse_srt;
pub use word_timing::{dtw_preset_for_model, WhisperCliWordSegmenter, WordTiming};
