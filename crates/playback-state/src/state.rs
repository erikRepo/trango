//! The `PlayerState` struct: the player's full observable state and the
//! transitions that mutate it (mode toggling, loading cues, translation
//! visibility).

use subtitle::Cue;

use crate::mode::PlaybackMode;

/// The player's full observable state: current mode, loaded cues, cursor
/// position within them, and whether translations are shown.
#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    /// Current playback mode.
    pub mode: PlaybackMode,
    /// The subtitle cues currently loaded for this player.
    pub cues: Vec<Cue>,
    /// Index into `cues` of the cue currently in focus, if any.
    pub current_cue_index: Option<usize>,
    /// Whether the translation text should be shown alongside the original.
    pub show_translation: bool,
}

impl PlayerState {
    /// Builds a fresh `PlayerState`: `Normal` mode, no cues, no cursor,
    /// translation hidden.
    pub fn new() -> Self {
        Self::default()
    }

    /// Switches between `Normal` and `SentenceBySentence` mode.
    pub fn toggle_mode(&mut self) {
        self.mode = match self.mode {
            PlaybackMode::Normal => PlaybackMode::SentenceBySentence,
            PlaybackMode::SentenceBySentence => PlaybackMode::Normal,
        };
    }

    /// Replaces the loaded cues and resets the cursor to the first cue, or
    /// to `None` if `cues` is empty.
    pub fn set_cues(&mut self, cues: Vec<Cue>) {
        self.current_cue_index = if cues.is_empty() { None } else { Some(0) };
        self.cues = cues;
    }

    /// Flips whether translation text is shown.
    pub fn toggle_translation(&mut self) {
        self.show_translation = !self.show_translation;
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn cue(index: u32, start_ms: u64, end_ms: u64, text: &str) -> Cue {
        Cue::new(
            index,
            Duration::from_millis(start_ms),
            Duration::from_millis(end_ms),
            text,
        )
        .unwrap()
    }

    #[test]
    fn test_new_state_starts_in_normal_mode_with_no_cues() {
        // Given: nothing
        // When:  building a fresh PlayerState
        // Then:  mode is Normal, cues are empty, cursor is None, translation hidden
        let state = PlayerState::new();
        assert_eq!(state.mode, PlaybackMode::Normal);
        assert!(state.cues.is_empty());
        assert_eq!(state.current_cue_index, None);
        assert!(!state.show_translation);
    }

    #[test]
    fn test_toggle_mode_switches_normal_to_sentence_by_sentence() {
        // Given: a fresh state in Normal mode
        // When:  toggling the mode once
        // Then:  it becomes SentenceBySentence
        let mut state = PlayerState::new();
        state.toggle_mode();
        assert_eq!(state.mode, PlaybackMode::SentenceBySentence);
    }

    #[test]
    fn test_toggle_mode_twice_returns_to_normal() {
        // Given: a fresh state in Normal mode
        // When:  toggling the mode twice
        // Then:  it is back to Normal
        let mut state = PlayerState::new();
        state.toggle_mode();
        state.toggle_mode();
        assert_eq!(state.mode, PlaybackMode::Normal);
    }

    #[test]
    fn test_set_cues_resets_cursor_to_first_cue() {
        // Given: a fresh state and a non-empty list of cues
        // When:  setting the cues
        // Then:  the cues are stored and the cursor points at index 0
        let mut state = PlayerState::new();
        let cues = vec![cue(1, 0, 1_000, "Hello"), cue(2, 1_000, 2_000, "World")];

        state.set_cues(cues.clone());

        assert_eq!(state.cues, cues);
        assert_eq!(state.current_cue_index, Some(0));
    }

    #[test]
    fn test_set_cues_with_empty_vec_clears_cursor() {
        // Given: a state with cues already loaded
        // When:  setting an empty cue list
        // Then:  the cursor becomes None
        let mut state = PlayerState::new();
        state.set_cues(vec![cue(1, 0, 1_000, "Hello")]);

        state.set_cues(vec![]);

        assert!(state.cues.is_empty());
        assert_eq!(state.current_cue_index, None);
    }

    #[test]
    fn test_toggle_translation_flips_the_flag() {
        // Given: a fresh state with translations hidden
        // When:  toggling translation visibility twice
        // Then:  it becomes visible, then hidden again
        let mut state = PlayerState::new();
        state.toggle_translation();
        assert!(state.show_translation);
        state.toggle_translation();
        assert!(!state.show_translation);
    }
}
