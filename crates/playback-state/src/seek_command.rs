//! The `SeekCommand`/`PlaySpanCommand` directives returned by navigation
//! functions, describing what the video player should do without actually
//! driving mpv.

use std::time::Duration;

/// A directive telling the player to seek the playhead to `start` and stay
/// paused there — produced by [`crate::PlayerState`]'s `next_cue`/
/// `previous_cue`/`jump_to_cue`. No mode starts playback on its own (see
/// `docs/src/developer/specs.md`); only [`PlaySpanCommand`] (Space) does that.
/// Interpreting it against a real mpv instance is the caller's
/// responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeekCommand {
    /// Timestamp to seek the playhead to.
    pub start: Duration,
}

/// A directive telling the player to play from `start` through to `end` and
/// pause there — produced by [`crate::PlayerState::repeat_current_cue`]
/// (Space). Whether to actually start this playback, versus pausing
/// immediately because a previous span is already mid-play, depends on live
/// mpv state `PlayerState` doesn't have — that decision belongs to the
/// caller (`crates/app/src/video_player.rs`'s `toggle_play_span`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaySpanCommand {
    /// Timestamp to start playback from.
    pub start: Duration,
    /// Timestamp at which playback should pause.
    pub end: Duration,
}
