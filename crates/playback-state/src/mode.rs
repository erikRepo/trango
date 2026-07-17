//! The `PlaybackMode` enum distinguishing continuous playback from
//! sentence-by-sentence stepping — orthogonal to which source (video file or
//! audio) is currently active, see [`crate::MediaSource`].

/// Whether the player runs continuous playback or stops after each cue,
/// waiting for manual navigation. Independent of [`crate::MediaSource`]:
/// both modes are valid in either source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackMode {
    /// Continuous playback, uninterrupted by cue boundaries.
    Normal,
    /// Playback pauses at the end of each cue, waiting for manual navigation.
    #[default]
    SentenceBySentence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_sentence_by_sentence() {
        // Given: no explicit mode
        // When:  taking the Default value
        // Then:  it is SentenceBySentence — the primary language-learning use
        //        case, so a fresh player starts there rather than in Normal
        assert_eq!(PlaybackMode::default(), PlaybackMode::SentenceBySentence);
    }

    #[test]
    fn test_normal_is_distinct_from_sentence_by_sentence() {
        // Given: the two PlaybackMode variants
        // When:  comparing them
        // Then:  they are not equal
        assert_ne!(PlaybackMode::Normal, PlaybackMode::SentenceBySentence);
    }
}
