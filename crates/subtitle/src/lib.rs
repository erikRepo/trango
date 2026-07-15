//! Subtitle parsing and data model for trango.
//!
//! Provides the `Cue` data model, `SubtitleError`, and `parse_srt` for
//! parsing `.srt` subtitle files into `Vec<Cue>`.

mod cue;
mod error;
mod srt;

pub use cue::Cue;
pub use error::SubtitleError;
pub use srt::parse_srt;
