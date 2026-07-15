//! Subtitle parsing and data model for trango.
//!
//! Provides the `Cue` data model, `SubtitleError`, `parse_srt` for parsing
//! `.srt` subtitle files into `Vec<Cue>`, and `merge_translation` for
//! attaching a second (translation) subtitle track to an already-parsed
//! `Vec<Cue>`.

mod cue;
mod error;
mod merge;
mod srt;

pub use cue::Cue;
pub use error::SubtitleError;
pub use merge::merge_translation;
pub use srt::parse_srt;
