//! The `PlaybackMode` enum distinguishing continuous playback,
//! sentence-by-sentence stepping, and cue-only operation without a video.

/// Whether the player runs continuous playback, stops after each cue, or has
/// no video loaded at all (subtitle-only, see `TODO.md` Vaihe 25).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackMode {
    /// Continuous playback, uninterrupted by cue boundaries.
    Normal,
    /// Playback pauses at the end of each cue, waiting for manual navigation.
    #[default]
    SentenceBySentence,
    /// No video loaded — cues come from a linked subtitle or, eventually,
    /// live recording (Vaihe 26+) rather than mpv playback.
    NoVideo,
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
    fn test_no_video_is_distinct_from_the_other_two_modes() {
        // Given: the three PlaybackMode variants
        // When:  comparing NoVideo against Normal and SentenceBySentence
        // Then:  none of them are equal
        assert_ne!(PlaybackMode::NoVideo, PlaybackMode::Normal);
        assert_ne!(PlaybackMode::NoVideo, PlaybackMode::SentenceBySentence);
    }
}
