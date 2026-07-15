//! The `SeekCommand` directive returned by navigation functions, describing
//! what the video player should do without actually driving mpv.

use std::time::Duration;

/// A directive telling the player to seek to `start`, play through to `end`,
/// and pause afterward if `then_pause` is set. Produced by
/// [`crate::PlayerState`] navigation methods; interpreting it against a real
/// mpv instance is the caller's responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeekCommand {
    /// Timestamp to seek the playhead to.
    pub start: Duration,
    /// Timestamp at which playback should stop if `then_pause` is set.
    pub end: Duration,
    /// Whether the player should pause once `end` is reached.
    pub then_pause: bool,
}
