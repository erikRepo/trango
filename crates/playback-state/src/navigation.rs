//! Cue navigation: `next_cue`, `previous_cue`, and `repeat_current_cue`,
//! implementing the README's Right/Left/Space rules as pure logic. These
//! methods only move `PlayerState`'s cursor and report what the player
//! should do next — they never touch mpv themselves.

use subtitle::Cue;

use crate::seek_command::SeekCommand;
use crate::state::PlayerState;

/// Builds the seek command that plays a cue's full span and pauses at its end.
fn seek_command_for(cue: &Cue) -> SeekCommand {
    SeekCommand {
        start: cue.start,
        end: cue.end,
        then_pause: true,
    }
}

impl PlayerState {
    /// Advances the cursor to the next cue and returns the seek command to
    /// play through it, or `None` if there is no cues loaded or the cursor
    /// is already on the last cue.
    pub fn next_cue(&mut self) -> Option<SeekCommand> {
        let next_index = self.current_cue_index? + 1;
        let cue = self.cues.get(next_index)?;
        self.current_cue_index = Some(next_index);
        Some(seek_command_for(cue))
    }

    /// Moves the cursor to the previous cue and returns the seek command to
    /// play through it, or `None` if there are no cues loaded or the cursor
    /// is already on the first cue.
    pub fn previous_cue(&mut self) -> Option<SeekCommand> {
        let previous_index = self.current_cue_index?.checked_sub(1)?;
        let cue = self.cues.get(previous_index)?;
        self.current_cue_index = Some(previous_index);
        Some(seek_command_for(cue))
    }

    /// Returns the seek command to replay the current cue's span, without
    /// moving the cursor. Calling this repeatedly always yields the same
    /// command for the same cue. `None` if no cue is in focus.
    pub fn repeat_current_cue(&self) -> Option<SeekCommand> {
        let cue = self.cues.get(self.current_cue_index?)?;
        Some(seek_command_for(cue))
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
        // Then:  the cursor moves to the second cue and the command covers its span
        let mut state = PlayerState::new();
        state.set_cues(three_cues());

        let command = state.next_cue();

        assert_eq!(state.current_cue_index, Some(1));
        assert_eq!(
            command,
            Some(SeekCommand {
                start: Duration::from_millis(1_000),
                end: Duration::from_millis(2_000),
                then_pause: true,
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
        // Then:  the cursor moves to the first cue and the command covers its span
        let mut state = PlayerState::new();
        state.set_cues(three_cues());
        state.next_cue();

        let command = state.previous_cue();

        assert_eq!(state.current_cue_index, Some(0));
        assert_eq!(
            command,
            Some(SeekCommand {
                start: Duration::from_millis(0),
                end: Duration::from_millis(1_000),
                then_pause: true,
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
        // Then:  the cursor stays put and the command covers the current cue's span
        let mut state = PlayerState::new();
        state.set_cues(three_cues());
        state.next_cue();

        let command = state.repeat_current_cue();

        assert_eq!(state.current_cue_index, Some(1));
        assert_eq!(
            command,
            Some(SeekCommand {
                start: Duration::from_millis(1_000),
                end: Duration::from_millis(2_000),
                then_pause: true,
            })
        );
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
