//! Cue navigation: `next_cue`, `previous_cue`, `jump_to_cue` (always land
//! paused — no mode autoplays on its own, see `docs/src/specs/`) and
//! `repeat_current_cue` (Space's "play/replay this cue" directive), as pure
//! logic. These methods only move `PlayerState`'s cursor and report what
//! the player should do next — they never touch mpv themselves.

use crate::mode::PlaybackMode;
use crate::seek_command::{PlaySpanCommand, SeekCommand};
use crate::state::PlayerState;

impl PlayerState {
    /// Advances the cursor to the next cue and returns the seek command to
    /// land the playhead at its start (paused — see [`SeekCommand`]'s doc
    /// comment), or `None` if there is no cues loaded or the cursor is
    /// already on the last cue.
    pub fn next_cue(&mut self) -> Option<SeekCommand> {
        let next_index = self.current_cue_index? + 1;
        let cue = self.cues.get(next_index)?;
        self.current_cue_index = Some(next_index);
        Some(SeekCommand { start: cue.start })
    }

    /// Moves the cursor to the previous cue and returns the seek command to
    /// land the playhead at its start, or `None` if there are no cues
    /// loaded or the cursor is already on the first cue.
    pub fn previous_cue(&mut self) -> Option<SeekCommand> {
        let previous_index = self.current_cue_index?.checked_sub(1)?;
        let cue = self.cues.get(previous_index)?;
        self.current_cue_index = Some(previous_index);
        Some(SeekCommand { start: cue.start })
    }

    /// Returns the command to play the current cue's span, without moving
    /// the cursor. Calling this repeatedly always yields the same command
    /// for the same cue. `None` in `Normal` mode — bounding playback to one
    /// cue's span is a `SentenceBySentence`-only concept, so `Normal` mode
    /// ignores `current_cue_index` here even if a subtitle happens to be
    /// loaded, letting the caller fall back to a plain, unbounded play/pause
    /// toggle instead (see `crates/app/src/main.rs`'s `repeat-cue` handler)
    /// — otherwise `Normal` mode playback would auto-pause at the end of
    /// whatever cue is currently in focus instead of continuing. Also
    /// `None` if no cue is in focus. Whether this actually starts playback
    /// (versus pausing an already-playing span early) is decided by the
    /// caller against live mpv state — see [`PlaySpanCommand`]'s doc
    /// comment.
    pub fn repeat_current_cue(&self) -> Option<PlaySpanCommand> {
        if self.mode != PlaybackMode::SentenceBySentence {
            return None;
        }
        let cue = self.cues.get(self.current_cue_index?)?;
        Some(PlaySpanCommand {
            start: cue.start,
            end: cue.end,
        })
    }

    /// Moves the cursor directly to `index` and returns the seek command to
    /// land the playhead at that cue's start — the same command shape
    /// `next_cue`/`previous_cue` return, so a sentence list row click
    /// behaves exactly like arrow navigation. `None`, leaving the cursor
    /// untouched, if `index` is out of range for the loaded cues.
    pub fn jump_to_cue(&mut self, index: usize) -> Option<SeekCommand> {
        let cue = self.cues.get(index)?;
        self.current_cue_index = Some(index);
        Some(SeekCommand { start: cue.start })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use subtitle::Cue;

    fn cue(index: u32, start_ms: u64, end_ms: u64, text: &str) -> Cue {
        Cue::new(
            index,
            Duration::from_millis(start_ms),
            Duration::from_millis(end_ms),
            text,
        )
        .unwrap()
    }

    fn three_cues() -> Vec<Cue> {
        vec![
            cue(1, 0, 1_000, "one"),
            cue(2, 1_000, 2_000, "two"),
            cue(3, 2_000, 3_000, "three"),
        ]
    }

    #[test]
    fn test_next_cue_on_empty_state_returns_none() {
        // Given: a fresh state with no cues loaded
        // When:  calling next_cue
        // Then:  it returns None and the cursor stays None
        let mut state = PlayerState::new();
        assert_eq!(state.next_cue(), None);
        assert_eq!(state.current_cue_index, None);
    }

    #[test]
    fn test_next_cue_advances_cursor_and_returns_seek_command() {
        // Given: a state with three cues, cursor on the first
        // When:  calling next_cue
        // Then:  the cursor moves to the second cue and the command seeks
        //        to its start
        let mut state = PlayerState::new();
        state.set_cues(three_cues());

        let command = state.next_cue();

        assert_eq!(state.current_cue_index, Some(1));
        assert_eq!(
            command,
            Some(SeekCommand {
                start: Duration::from_millis(1_000),
            })
        );
    }

    #[test]
    fn test_next_cue_on_last_cue_returns_none_and_does_not_move() {
        // Given: a state with three cues, cursor on the last one
        // When:  calling next_cue
        // Then:  it returns None and the cursor stays on the last cue
        let mut state = PlayerState::new();
        state.set_cues(three_cues());
        state.next_cue();
        state.next_cue();

        let command = state.next_cue();

        assert_eq!(command, None);
        assert_eq!(state.current_cue_index, Some(2));
    }

    #[test]
    fn test_previous_cue_on_empty_state_returns_none() {
        // Given: a fresh state with no cues loaded
        // When:  calling previous_cue
        // Then:  it returns None and the cursor stays None
        let mut state = PlayerState::new();
        assert_eq!(state.previous_cue(), None);
        assert_eq!(state.current_cue_index, None);
    }

    #[test]
    fn test_previous_cue_on_first_cue_returns_none_and_does_not_move() {
        // Given: a state with three cues, cursor on the first
        // When:  calling previous_cue
        // Then:  it returns None and the cursor stays on the first cue
        let mut state = PlayerState::new();
        state.set_cues(three_cues());

        let command = state.previous_cue();

        assert_eq!(command, None);
        assert_eq!(state.current_cue_index, Some(0));
    }

    #[test]
    fn test_previous_cue_moves_cursor_back_and_returns_seek_command() {
        // Given: a state with three cues, cursor on the second
        // When:  calling previous_cue
        // Then:  the cursor moves to the first cue and the command seeks
        //        to its start
        let mut state = PlayerState::new();
        state.set_cues(three_cues());
        state.next_cue();

        let command = state.previous_cue();

        assert_eq!(state.current_cue_index, Some(0));
        assert_eq!(
            command,
            Some(SeekCommand {
                start: Duration::from_millis(0),
            })
        );
    }

    #[test]
    fn test_repeat_current_cue_on_empty_state_returns_none() {
        // Given: a fresh state with no cues loaded
        // When:  calling repeat_current_cue
        // Then:  it returns None
        let state = PlayerState::new();
        assert_eq!(state.repeat_current_cue(), None);
    }

    #[test]
    fn test_repeat_current_cue_does_not_move_cursor() {
        // Given: a state with three cues, cursor on the second
        // When:  calling repeat_current_cue
        // Then:  the cursor stays put and the command covers the current
        //        cue's full span (start and end)
        let mut state = PlayerState::new();
        state.set_cues(three_cues());
        state.next_cue();

        let command = state.repeat_current_cue();

        assert_eq!(state.current_cue_index, Some(1));
        assert_eq!(
            command,
            Some(PlaySpanCommand {
                start: Duration::from_millis(1_000),
                end: Duration::from_millis(2_000),
            })
        );
    }

    #[test]
    fn test_repeat_current_cue_in_normal_mode_returns_none() {
        // Given: a state in Normal mode with cues loaded (e.g. a subtitle
        //        linked while the user happens to be in Normal mode)
        // When:  calling repeat_current_cue
        // Then:  it returns None, even though a cue is technically in
        //        focus — bounding Space to one cue's span only makes sense
        //        in SentenceBySentence mode, so the caller (main.rs) can
        //        fall back to a plain, unbounded play/pause toggle instead
        //        of auto-pausing Normal-mode playback at a sentence's end
        let mut state = PlayerState::new();
        state.set_cues(three_cues());
        state.toggle_mode();
        assert_eq!(state.mode, PlaybackMode::Normal);

        assert_eq!(state.repeat_current_cue(), None);
    }

    #[test]
    fn test_jump_to_cue_on_empty_state_returns_none() {
        // Given: a fresh state with no cues loaded
        // When:  jumping to any index
        // Then:  it returns None and the cursor stays None
        let mut state = PlayerState::new();
        assert_eq!(state.jump_to_cue(0), None);
        assert_eq!(state.current_cue_index, None);
    }

    #[test]
    fn test_jump_to_cue_moves_cursor_and_returns_seek_command() {
        // Given: a state with three cues, cursor on the first
        // When:  jumping directly to the third cue
        // Then:  the cursor moves there and the command seeks to its start
        let mut state = PlayerState::new();
        state.set_cues(three_cues());

        let command = state.jump_to_cue(2);

        assert_eq!(state.current_cue_index, Some(2));
        assert_eq!(
            command,
            Some(SeekCommand {
                start: Duration::from_millis(2_000),
            })
        );
    }

    #[test]
    fn test_jump_to_cue_out_of_range_returns_none_and_does_not_move() {
        // Given: a state with three cues, cursor on the second
        // When:  jumping to an index past the end of the cue list
        // Then:  it returns None and the cursor stays where it was
        let mut state = PlayerState::new();
        state.set_cues(three_cues());
        state.next_cue();

        let command = state.jump_to_cue(99);

        assert_eq!(command, None);
        assert_eq!(state.current_cue_index, Some(1));
    }

    #[test]
    fn test_repeat_current_cue_called_repeatedly_always_returns_same_command() {
        // Given: a state with three cues, cursor on the last one
        // When:  calling repeat_current_cue multiple times
        // Then:  every call returns the identical command for that same cue
        let mut state = PlayerState::new();
        state.set_cues(three_cues());
        state.next_cue();
        state.next_cue();

        let first = state.repeat_current_cue();
        let second = state.repeat_current_cue();
        let third = state.repeat_current_cue();

        assert_eq!(first, second);
        assert_eq!(second, third);
        assert_eq!(state.current_cue_index, Some(2));
    }
}
