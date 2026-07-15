//! Subtitle generation: turning a video file into an original-language
//! subtitle track via speech-to-text.

use std::io;
use std::path::{Path, PathBuf};

use crate::error::SubtitleError;

/// Generates an original-language `.srt` file for a video.
///
/// Implementations write the subtitle file to disk and return its path.
/// The real speech-to-text backend (e.g. a local Whisper model, `TODO.md`
/// Vaihe 20) is a separate, later step — for now only
/// [`StubSubtitleGenerator`] exists, so the "Generate subtitles" UI flow
/// (`Idle -> Generating -> Done`/`Error`) can be built and tested first.
pub trait SubtitleGenerator {
    /// Generates a subtitle file for `video_path`, returning the path it
    /// was written to. Returns `SubtitleError::IoError` if `video_path`
    /// doesn't exist or the subtitle file can't be written.
    fn generate(&self, video_path: &Path) -> Result<PathBuf, SubtitleError>;
}

/// A placeholder [`SubtitleGenerator`] that writes a single fixed-text cue
/// spanning the first five seconds of the video, rather than running real
/// speech-to-text. Always writes to `video_path` with its extension
/// replaced by `.srt`, the same same-stem convention
/// `open_video_dialog::matching_subtitle_path` looks for — so a generated
/// file is picked up as the video's linked original subtitle the next time
/// the Open Subtitles dialog opens.
pub struct StubSubtitleGenerator;

impl SubtitleGenerator for StubSubtitleGenerator {
    fn generate(&self, video_path: &Path) -> Result<PathBuf, SubtitleError> {
        if !video_path.is_file() {
            return Err(SubtitleError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                format!("video file not found: {}", video_path.display()),
            )));
        }

        let output_path = video_path.with_extension("srt");
        let contents = "1\n\
            00:00:00,000 --> 00:00:05,000\n\
            [Stub subtitle — real speech-to-text is not wired in yet]\n";
        std::fs::write(&output_path, contents)?;
        Ok(output_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh temp dir with a fake (empty) `some_video.mp4` inside it —
    /// `StubSubtitleGenerator` only checks that the video path exists, so
    /// an empty file stands in without needing a real video fixture.
    fn video_fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-generate-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        let video_path = dir.join("some_video.mp4");
        std::fs::write(&video_path, b"").expect("failed to write fixture video file");
        video_path
    }

    #[test]
    fn test_stub_generator_writes_same_stem_srt_and_returns_its_path() {
        // Given: a fake video file in a temp dir
        // When:  generating a subtitle for it
        // Then:  a same-stem .srt file is written and its path returned,
        //        and the written file parses back into one cue
        let video_path = video_fixture("writes-same-stem-srt");
        let expected_output = video_path.with_extension("srt");

        let output_path = StubSubtitleGenerator.generate(&video_path).unwrap();

        assert_eq!(output_path, expected_output);
        let cues = crate::parse_srt(&std::fs::read_to_string(&output_path).unwrap()).unwrap();
        assert_eq!(cues.len(), 1);

        std::fs::remove_dir_all(video_path.parent().unwrap())
            .expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_generate_errors_when_video_file_does_not_exist() {
        // Given: a video path that doesn't exist on disk
        // When:  generating a subtitle for it
        // Then:  it returns SubtitleError::IoError rather than writing
        //        anything
        let result = StubSubtitleGenerator.generate(Path::new("/no/such/video.mp4"));

        assert!(matches!(result, Err(SubtitleError::IoError(_))));
    }
}
