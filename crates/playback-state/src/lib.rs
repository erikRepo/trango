//! Playback state machine for trango (mode, cursor, transitions).
//!
//! Provides `PlaybackMode` (Normal vs. sentence-by-sentence) and
//! `PlayerState`, the player's full observable state plus the transitions
//! that mutate it. No I/O, no UI — pure state, so it can be tested without a
//! Slint window or a video file.

mod mode;
mod navigation;
mod playback_speed;
mod seek_command;
mod state;
mod time_format;

pub use mode::PlaybackMode;
pub use playback_speed::{format_speed_label, speed_from_fraction, MAX_SPEED, MIN_SPEED};
pub use seek_command::{PlaySpanCommand, SeekCommand};
pub use state::PlayerState;
pub use time_format::format_time;
