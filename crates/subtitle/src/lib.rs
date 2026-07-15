//! Subtitle parsing and data model for trango.
//!
//! Currently provides the `Cue` data model and `SubtitleError`; `.srt`
//! parsing is added in a later development step (see `TODO.md`).

mod cue;
mod error;

pub use cue::Cue;
pub use error::SubtitleError;
