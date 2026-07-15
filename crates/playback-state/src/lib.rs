//! Playback state machine for trango (mode, cursor, transitions).
//!
//! Provides `PlaybackMode` (Normal vs. sentence-by-sentence) and
//! `PlayerState`, the player's full observable state plus the transitions
//! that mutate it. No I/O, no UI — pure state, so it can be tested without a
//! Slint window or a video file.

mod mode;
mod navigation;
mod seek_command;
mod state;

pub use mode::PlaybackMode;
pub use seek_command::SeekCommand;
pub use state::PlayerState;
