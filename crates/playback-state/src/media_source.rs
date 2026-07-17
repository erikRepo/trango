//! The `MediaSource` enum distinguishing which source panel is active —
//! orthogonal to [`crate::PlaybackMode`]'s Normal/Sentence-by-sentence
//! navigation choice, see `TODO.md` Vaihe 25.

/// Which source panel the top bar has selected: a video file played through
/// mpv, or an audio-only source (captured/recorded audio, `TODO.md`
/// Vaihe 26+). Independent of [`crate::PlaybackMode`]: both navigation modes
/// are valid in either source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaSource {
    /// A video file, played and seeked through mpv.
    #[default]
    Video,
    /// No video — cues come from a linked subtitle or, eventually, captured/
    /// recorded audio (`TODO.md` Vaihe 26+) rather than mpv playback.
    Audio,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_media_source_is_video() {
        // Given: no explicit source
        // When:  taking the Default value
        // Then:  it is Video — the app's original behavior before Audio
        //        sources existed
        assert_eq!(MediaSource::default(), MediaSource::Video);
    }

    #[test]
    fn test_video_is_distinct_from_audio() {
        // Given: the two MediaSource variants
        // When:  comparing them
        // Then:  they are not equal
        assert_ne!(MediaSource::Video, MediaSource::Audio);
    }
}
