//! Mirrors `PlayerState`'s current cue into the Slint window's current-
//! sentence card properties (`sentence-label`, `sentence-text`,
//! `has-current-sentence`, `translation-text`) — the "Sentence N / M"
//! header, original-language text, whether either has anything to show
//! yet, and the current cue's translation (if any merged in via
//! `subtitle::merge_translation`). The card's `show-translation` toggle
//! is a separate, independently-wired property — see `main.rs`'s
//! `wire_player_state` — since it reflects `PlayerState.show_translation`
//! rather than anything cue-specific.

use playback_state::PlayerState;

use crate::AppWindow;

/// The current-sentence card's display state, computed from `PlayerState`
/// without touching Slint — kept separate from [`update_sentence_card`] so
/// it can be unit-tested without an `AppWindow`.
struct SentenceCardDisplay {
    label: String,
    text: String,
    has_sentence: bool,
    translation_text: String,
}

/// Computes the current-sentence card's display state from `state`'s cursor
/// and loaded cues. Falls back to a placeholder label/text when no cue is in
/// focus (no cues loaded yet, or the cursor is `None`). `translation_text`
/// is empty when the current cue has no merged translation, regardless of
/// whether the translation toggle is currently on — visibility is handled
/// separately in Slint via the `show-translation` property.
fn sentence_card_display(state: &PlayerState) -> SentenceCardDisplay {
    match state
        .current_cue_index
        .and_then(|index| state.cues.get(index).map(|cue| (index, cue)))
    {
        Some((index, cue)) => SentenceCardDisplay {
            label: format!("Sentence {} / {}", index + 1, state.cues.len()),
            text: cue.text.clone(),
            has_sentence: true,
            translation_text: cue.translation.clone().unwrap_or_default(),
        },
        None => SentenceCardDisplay {
            label: "Sentence – / –".to_string(),
            text: "No sentence loaded.".to_string(),
            has_sentence: false,
            translation_text: String::new(),
        },
    }
}

/// Sets `window`'s current-sentence card properties from `state`. Called
/// once after cues are (re)loaded, and again after every cue-sync driven by
/// mpv's `time-pos` polling in `SentenceBySentence` mode (see
/// `video_player.rs`). Does not touch `show-translation`, which is wired
/// directly to the toggle callback in `main.rs`.
pub fn update_sentence_card(window: &AppWindow, state: &PlayerState) {
    let display = sentence_card_display(state);
    window.set_sentence_label(display.label.into());
    window.set_sentence_text(display.text.into());
    window.set_has_current_sentence(display.has_sentence);
    window.set_translation_text(display.translation_text.into());
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
        assert_eq!(display.translation_text, "");
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
        assert_eq!(display.translation_text, "");
    }

    #[test]
    fn test_sentence_card_display_includes_merged_translation() {
        // Given: a state whose current cue has a translation merged in
        // When:  computing the sentence card display
        // Then:  translation_text carries the translated text
        let mut original = cue(1, 0, 1_000, "Hello");
        original.translation = Some("Hei".to_string());
        let mut state = PlayerState::new();
        state.set_cues(vec![original]);

        let display = sentence_card_display(&state);

        assert_eq!(display.translation_text, "Hei");
    }
}
