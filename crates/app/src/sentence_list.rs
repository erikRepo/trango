//! Mirrors `PlayerState`'s loaded cues into the Slint window's sentence-list
//! model (`sentence-list-rows`/`sentence-list-current-index`) — the
//! scrollable "index · text" list underneath the current-sentence card, with
//! the current cue highlighted. Called after cues are (re)loaded and after
//! every cursor-changing navigation (arrow keys, row clicks, mpv `time-pos`
//! sync — see `main.rs` and `video_player.rs`).

use std::rc::Rc;

use playback_state::PlayerState;
use slint::VecModel;

use crate::{AppWindow, SentenceListRow};

/// Builds one row per loaded cue: `"index · text"`, with `is_current` set on
/// the cue at `state.current_cue_index`.
fn sentence_list_rows(state: &PlayerState) -> Vec<SentenceListRow> {
    state
        .cues
        .iter()
        .enumerate()
        .map(|(index, cue)| SentenceListRow {
            label: format!("{} · {}", cue.index, cue.text).into(),
            is_current: state.current_cue_index == Some(index),
        })
        .collect()
}

/// Sets `window`'s sentence-list model and current-index property from
/// `state`.
pub fn update_sentence_list(window: &AppWindow, state: &PlayerState) {
    let rows = sentence_list_rows(state);
    window.set_sentence_list_rows(Rc::new(VecModel::from(rows)).into());
    let current_index = state
        .current_cue_index
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1);
    window.set_sentence_list_current_index(current_index);
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
    fn test_sentence_list_rows_with_no_cues_is_empty() {
        // Given: a fresh PlayerState with no cues loaded
        // When:  computing the sentence list rows
        // Then:  the row list is empty
        let state = PlayerState::new();

        let rows = sentence_list_rows(&state);

        assert!(rows.is_empty());
    }

    #[test]
    fn test_sentence_list_rows_marks_current_cue() {
        // Given: a state with three cues, cursor on the second
        // When:  computing the sentence list rows
        // Then:  each row's label is "index · text" and only the current
        //        cue's row has is_current set
        let mut state = PlayerState::new();
        state.set_cues(vec![
            cue(1, 0, 1_000, "one"),
            cue(2, 1_000, 2_000, "two"),
            cue(3, 2_000, 3_000, "three"),
        ]);
        state.jump_to_cue(1);

        let rows = sentence_list_rows(&state);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].label, "1 · one");
        assert!(!rows[0].is_current);
        assert_eq!(rows[1].label, "2 · two");
        assert!(rows[1].is_current);
        assert_eq!(rows[2].label, "3 · three");
        assert!(!rows[2].is_current);
    }
}
