//! The `PlaybackMode` enum distinguishing continuous playback from
//! sentence-by-sentence stepping.

/// Whether the player runs continuous playback or stops after each cue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackMode {
    /// Continuous playback, uninterrupted by cue boundaries.
    #[default]
    Normal,
    /// Playback pauses at the end of each cue, waiting for manual navigation.
    SentenceBySentence,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_mode_is_normal() {
        // Given: no explicit mode
        // When:  taking the Default value
        // Then:  it is Normal
        assert_eq!(PlaybackMode::default(), PlaybackMode::Normal);
    }
}
