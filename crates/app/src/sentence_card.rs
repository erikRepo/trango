//! Mirrors `PlayerState`'s current cue into the Slint window's current-
//! sentence card properties (`sentence-label`, `sentence-text`,
//! `has-current-sentence`) — the "Sentence N / M" header, original-language
//! text, and whether either has anything to show yet.

use playback_state::PlayerState;

use crate::AppWindow;

/// The current-sentence card's display state, computed from `PlayerState`
/// without touching Slint — kept separate from [`update_sentence_card`] so
/// it can be unit-tested without an `AppWindow`.
struct SentenceCardDisplay {
    label: String,
    text: String,
    has_sentence: bool,
}

/// Computes the current-sentence card's display state from `state`'s cursor
/// and loaded cues. Falls back to a placeholder label/text when no cue is in
/// focus (no cues loaded yet, or the cursor is `None`).
fn sentence_card_display(state: &PlayerState) -> SentenceCardDisplay {
    match state
        .current_cue_index
        .and_then(|index| state.cues.get(index).map(|cue| (index, cue)))
    {
        Some((index, cue)) => SentenceCardDisplay {
            label: format!("Sentence {} / {}", index + 1, state.cues.len()),
            text: cue.text.clone(),
            has_sentence: true,
        },
        None => SentenceCardDisplay {
            label: "Sentence – / –".to_string(),
            text: "No sentence loaded.".to_string(),
            has_sentence: false,
        },
    }
}

/// Sets `window`'s current-sentence card properties from `state`. Called
/// once after cues are (re)loaded, and again after every cue-sync driven by
/// mpv's `time-pos` polling in `SentenceBySentence` mode (see
/// `video_player.rs`).
pub fn update_sentence_card(window: &AppWindow, state: &PlayerState) {
    let display = sentence_card_display(state);
    window.set_sentence_label(display.label.into());
    window.set_sentence_text(display.text.into());
    window.set_has_current_sentence(display.has_sentence);
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use subtitle::Cue;

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
    fn test_sentence_card_display_with_no_cues_is_placeholder() {
        // Given: a fresh PlayerState with no cues loaded
        // When:  computing the sentence card display
        // Then:  it falls back to the placeholder label/text
        let state = PlayerState::new();

        let display = sentence_card_display(&state);

        assert_eq!(display.label, "Sentence – / –");
        assert_eq!(display.text, "No sentence loaded.");
        assert!(!display.has_sentence);
    }

    #[test]
    fn test_sentence_card_display_reflects_current_cue() {
        // Given: a state with cues loaded, cursor on the second (1-based: 2 / 3)
        // When:  computing the sentence card display
        // Then:  the label counts from 1 and the text is the cue's own text
        let mut state = PlayerState::new();
        state.set_cues(vec![
            cue(1, 0, 1_000, "one"),
            cue(2, 1_000, 2_000, "two"),
            cue(3, 2_000, 3_000, "three"),
        ]);
        state.sync_cue_to_time(Duration::from_millis(1_500));

        let display = sentence_card_display(&state);

        assert_eq!(display.label, "Sentence 2 / 3");
        assert_eq!(display.text, "two");
        assert!(display.has_sentence);
    }
}
