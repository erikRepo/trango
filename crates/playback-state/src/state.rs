//! The `PlayerState` struct: the player's full observable state and the
//! transitions that mutate it (mode toggling, loading cues, translation
//! visibility).

use std::time::Duration;

use subtitle::Cue;

use crate::media_source::MediaSource;
use crate::mode::PlaybackMode;

/// The player's full observable state: current mode, active source, loaded
/// cues, cursor position within them, and whether translations are shown.
#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    /// Current playback mode.
    pub mode: PlaybackMode,
    /// Which source panel (video or audio) is currently active — orthogonal
    /// to `mode`.
    pub media_source: MediaSource,
    /// The subtitle cues currently loaded for this player.
    pub cues: Vec<Cue>,
    /// Index into `cues` of the cue currently in focus, if any.
    pub current_cue_index: Option<usize>,
    /// Whether the translation text should be shown alongside the original.
    pub show_translation: bool,
}

impl PlayerState {
    /// Builds a fresh `PlayerState`: `SentenceBySentence` mode (see
    /// `PlaybackMode`'s default), `Video` source (see `MediaSource`'s
    /// default), no cues, no cursor, translation hidden.
    pub fn new() -> Self {
        Self::default()
    }

    /// Switches to `mode` directly — used by the top bar's Normal/Sentence-
    /// by-sentence toggle, which names its target mode explicitly rather
    /// than cycling through the pair.
    pub fn set_mode(&mut self, mode: PlaybackMode) {
        self.mode = mode;
    }

    /// Switches to `source` directly — used by the top bar's Video/Audio
    /// source buttons, independent of `set_mode` above.
    pub fn set_media_source(&mut self, source: MediaSource) {
        self.media_source = source;
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

    /// Appends live-transcribed cues (`TODO.md` Vaihe 28) to the end of the
    /// already-loaded ones, re-indexing them to continue the existing
    /// sequence, and moves the cursor onto the last newly appended cue —
    /// the most recently transcribed sentence becomes the one shown as
    /// current, since the Audio source has no mpv position to sync from.
    /// Does nothing if `new_cues` is empty.
    pub fn push_cues(&mut self, new_cues: Vec<Cue>) {
        if new_cues.is_empty() {
            return;
        }
        let first_index = self.cues.len() as u32 + 1;
        for (offset, mut cue) in new_cues.into_iter().enumerate() {
            cue.index = first_index + offset as u32;
            self.cues.push(cue);
        }
        self.current_cue_index = Some(self.cues.len() - 1);
    }

    /// Updates `current_cue_index` to the cue whose start timestamp is the
    /// latest one at or before `time` — i.e. the sentence currently
    /// playing, or the most recently started one if `time` falls in a gap
    /// between cues. Leaves the cursor at `None` if `time` is before the
    /// first cue's start, or no cues are loaded. Returns whether the cursor
    /// actually changed, so callers polling this on a timer (see
    /// `video_player.rs`'s `sync_current_sentence_normal_mode`) can skip
    /// re-rendering the sentence list when it didn't.
    ///
    /// Callers must only invoke this in `Normal` mode — see
    /// `docs/src/developer/specs.md`'s "`sync_current_sentence` removed
    /// entirely" for why a near-identical mechanism, called unconditionally
    /// on every poll tick regardless of mode, previously broke
    /// `SentenceBySentence` mode's Space replay behavior. This method
    /// itself doesn't check `self.mode` — it's pure cursor-from-timestamp
    /// math, agnostic of why the caller wants it.
    pub fn sync_cue_to_time(&mut self, time: Duration) -> bool {
        let started_count = self.cues.partition_point(|cue| cue.start <= time);
        let new_index = started_count.checked_sub(1);
        let changed = new_index != self.current_cue_index;
        self.current_cue_index = new_index;
        changed
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
    fn test_new_state_starts_in_sentence_by_sentence_mode_with_no_cues() {
        // Given: nothing
        // When:  building a fresh PlayerState
        // Then:  mode is SentenceBySentence, source is Video, cues are
        //        empty, cursor is None, translation hidden
        let state = PlayerState::new();
        assert_eq!(state.mode, PlaybackMode::SentenceBySentence);
        assert_eq!(state.media_source, MediaSource::Video);
        assert!(state.cues.is_empty());
        assert_eq!(state.current_cue_index, None);
        assert!(!state.show_translation);
    }

    #[test]
    fn test_set_mode_switches_sentence_by_sentence_to_normal() {
        // Given: a fresh state (defaults to SentenceBySentence)
        // When:  setting the mode to Normal
        // Then:  it becomes Normal
        let mut state = PlayerState::new();
        state.set_mode(PlaybackMode::Normal);
        assert_eq!(state.mode, PlaybackMode::Normal);
    }

    #[test]
    fn test_set_media_source_switches_video_to_audio() {
        // Given: a fresh state (defaults to Video)
        // When:  setting the source to Audio
        // Then:  it becomes Audio
        let mut state = PlayerState::new();
        state.set_media_source(MediaSource::Audio);
        assert_eq!(state.media_source, MediaSource::Audio);
    }

    #[test]
    fn test_set_media_source_back_to_video() {
        // Given: a state switched to Audio
        // When:  setting the source back to Video
        // Then:  it is Video again
        let mut state = PlayerState::new();
        state.set_media_source(MediaSource::Audio);
        state.set_media_source(MediaSource::Video);
        assert_eq!(state.media_source, MediaSource::Video);
    }

    #[test]
    fn test_media_source_and_playback_mode_are_independent() {
        // Given: a fresh state
        // When:  switching the source to Audio and the mode to Normal
        // Then:  both changes stick independently of each other
        let mut state = PlayerState::new();
        state.set_media_source(MediaSource::Audio);
        state.set_mode(PlaybackMode::Normal);
        assert_eq!(state.media_source, MediaSource::Audio);
        assert_eq!(state.mode, PlaybackMode::Normal);
    }

    #[test]
    fn test_set_mode_back_to_sentence_by_sentence() {
        // Given: a state switched away to Normal
        // When:  setting the mode back to SentenceBySentence
        // Then:  it is SentenceBySentence again
        let mut state = PlayerState::new();
        state.set_mode(PlaybackMode::Normal);
        state.set_mode(PlaybackMode::SentenceBySentence);
        assert_eq!(state.mode, PlaybackMode::SentenceBySentence);
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
    fn test_push_cues_onto_empty_state_selects_last_cue() {
        // Given: a fresh state with no cues
        // When:  pushing two newly transcribed cues
        // Then:  they're stored, re-indexed from 1, and the cursor selects
        //        the last one
        let mut state = PlayerState::new();

        state.push_cues(vec![cue(1, 0, 1_000, "one"), cue(1, 1_000, 2_000, "two")]);

        assert_eq!(state.cues.len(), 2);
        assert_eq!(state.cues[0].index, 1);
        assert_eq!(state.cues[1].index, 2);
        assert_eq!(state.current_cue_index, Some(1));
    }

    #[test]
    fn test_push_cues_appends_after_existing_cues_and_continues_indexing() {
        // Given: a state with one cue already loaded (cursor on it)
        // When:  pushing one more live-transcribed cue
        // Then:  the original cue is untouched, the new one is appended
        //        with the next index, and the cursor moves onto it
        let mut state = PlayerState::new();
        state.set_cues(vec![cue(1, 0, 1_000, "one")]);

        state.push_cues(vec![cue(1, 1_000, 2_000, "two")]);

        assert_eq!(state.cues.len(), 2);
        assert_eq!(state.cues[0].text, "one");
        assert_eq!(state.cues[1].index, 2);
        assert_eq!(state.cues[1].text, "two");
        assert_eq!(state.current_cue_index, Some(1));
    }

    #[test]
    fn test_push_cues_with_empty_vec_does_nothing() {
        // Given: a state with one cue loaded
        // When:  pushing an empty vec of new cues
        // Then:  cues and cursor are unchanged
        let mut state = PlayerState::new();
        state.set_cues(vec![cue(1, 0, 1_000, "one")]);

        state.push_cues(vec![]);

        assert_eq!(state.cues.len(), 1);
        assert_eq!(state.current_cue_index, Some(0));
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

    #[test]
    fn test_sync_cue_to_time_on_empty_cues_is_none() {
        // Given: a fresh state with no cues loaded
        // When:  syncing to any timestamp
        // Then:  the cursor stays None and nothing "changed"
        let mut state = PlayerState::new();
        assert!(!state.sync_cue_to_time(Duration::from_millis(500)));
        assert_eq!(state.current_cue_index, None);
    }

    #[test]
    fn test_sync_cue_to_time_before_first_cue_clears_cursor() {
        // Given: a state with cues loaded (cursor defaults to the first cue)
        // When:  syncing to a time before the first cue's start
        // Then:  the cursor becomes None, since no sentence has started yet,
        //        and the change is reported
        let mut state = PlayerState::new();
        state.set_cues(vec![
            cue(1, 1_000, 2_000, "one"),
            cue(2, 2_000, 3_000, "two"),
        ]);

        let changed = state.sync_cue_to_time(Duration::from_millis(500));

        assert!(changed);
        assert_eq!(state.current_cue_index, None);
    }

    #[test]
    fn test_sync_cue_to_time_at_exact_cue_start_selects_that_cue() {
        // Given: a state with two cues
        // When:  syncing exactly to the second cue's start
        // Then:  the cursor selects the second cue
        let mut state = PlayerState::new();
        state.set_cues(vec![cue(1, 0, 1_000, "one"), cue(2, 1_000, 2_000, "two")]);

        state.sync_cue_to_time(Duration::from_millis(1_000));

        assert_eq!(state.current_cue_index, Some(1));
    }

    #[test]
    fn test_sync_cue_to_time_within_cue_span_selects_that_cue() {
        // Given: a state with two cues
        // When:  syncing to a time inside the first cue's span
        // Then:  the cursor selects the first cue
        let mut state = PlayerState::new();
        state.set_cues(vec![cue(1, 0, 1_000, "one"), cue(2, 1_000, 2_000, "two")]);

        state.sync_cue_to_time(Duration::from_millis(500));

        assert_eq!(state.current_cue_index, Some(0));
    }

    #[test]
    fn test_sync_cue_to_time_in_gap_keeps_most_recently_started_cue() {
        // Given: a state with two cues that have a silent gap between them
        // When:  syncing to a time inside that gap
        // Then:  the cursor selects the cue that most recently started
        let mut state = PlayerState::new();
        state.set_cues(vec![cue(1, 0, 1_000, "one"), cue(2, 1_500, 2_000, "two")]);

        state.sync_cue_to_time(Duration::from_millis(1_200));

        assert_eq!(state.current_cue_index, Some(0));
    }

    #[test]
    fn test_sync_cue_to_time_after_last_cue_selects_last_cue() {
        // Given: a state with two cues
        // When:  syncing to a time after the last cue's end
        // Then:  the cursor stays on the last cue
        let mut state = PlayerState::new();
        state.set_cues(vec![cue(1, 0, 1_000, "one"), cue(2, 1_000, 2_000, "two")]);

        state.sync_cue_to_time(Duration::from_millis(5_000));

        assert_eq!(state.current_cue_index, Some(1));
    }

    #[test]
    fn test_sync_cue_to_time_returns_false_when_cursor_is_unchanged() {
        // Given: a state already synced onto the first cue
        // When:  syncing again to a different time within that same cue's
        //        span
        // Then:  the cursor doesn't move and no change is reported —
        //        callers (video_player.rs) use this to skip rebuilding the
        //        sentence list on every poll tick
        let mut state = PlayerState::new();
        state.set_cues(vec![cue(1, 0, 1_000, "one"), cue(2, 1_000, 2_000, "two")]);
        state.sync_cue_to_time(Duration::from_millis(100));

        let changed = state.sync_cue_to_time(Duration::from_millis(400));

        assert!(!changed);
        assert_eq!(state.current_cue_index, Some(0));
    }
}
