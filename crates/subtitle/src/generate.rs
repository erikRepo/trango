//! Subtitle generation: turning a video file into an original-language
//! subtitle track via speech-to-text.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::SubtitleError;

/// Generates an original-language `.srt` file for a video.
///
/// Implementations write the subtitle file to disk and return its path.
/// [`StubSubtitleGenerator`] is a placeholder used to build and test the
/// "Generate subtitles" UI flow (`Idle -> Generating -> Done`/`Error`,
/// `TODO.md` Vaihe 20) before a real speech-to-text backend existed;
/// [`WhisperCliGenerator`] (`TODO.md` Vaihe 21.5) is the real
/// implementation, driving the external `whisper-cli` binary.
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

/// A [`SubtitleGenerator`] that runs [whisper.cpp](https://github.com/ggml-org/whisper.cpp)'s
/// `whisper-cli` binary as an external process (`TODO.md` Vaihe 21.5) — not
/// a Cargo dependency, since it's a separate tool the user installs
/// themselves (see `docs/src/usage/` for install instructions per
/// platform).
///
/// `whisper-cli` is asked to write straight to a same-stem `.srt` next to
/// `video_path` via its `-of`/`-osrt` flags (`-of` takes the output path
/// *without* an extension — `whisper-cli` appends `.srt` itself when
/// `-osrt` is set), matching [`StubSubtitleGenerator`]'s convention and
/// the one `open_video_dialog::matching_subtitle_path` looks for, so no
/// raw-text parsing is needed here.
pub struct WhisperCliGenerator {
    /// Path or bare name of the `whisper-cli` binary to run.
    /// [`Default::default`] uses `"whisper-cli"`, resolved via `PATH`.
    pub binary_path: PathBuf,
    /// Path to the ggml/gguf model file to pass via `-m`. `None` omits
    /// the flag, letting `whisper-cli` fall back to its own default model
    /// lookup.
    pub model_path: Option<PathBuf>,
    /// The `-l`/`--language` value to pass, e.g. `"en"` or `"auto"`. `None`
    /// omits the flag, letting `whisper-cli` fall back to its own default
    /// (`"en"`, regardless of which model is loaded) — callers transcribing
    /// anything other than English should pass an explicit value (the
    /// `app` crate's `model_picker::language_flag` derives one from the
    /// selected model's filename).
    pub language: Option<String>,
}

impl Default for WhisperCliGenerator {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("whisper-cli"),
            model_path: None,
            language: None,
        }
    }
}

impl SubtitleGenerator for WhisperCliGenerator {
    fn generate(&self, video_path: &Path) -> Result<PathBuf, SubtitleError> {
        if !video_path.is_file() {
            return Err(SubtitleError::IoError(io::Error::new(
                io::ErrorKind::NotFound,
                format!("video file not found: {}", video_path.display()),
            )));
        }

        let output_stem = video_path.with_extension("");
        let output_path = video_path.with_extension("srt");

        let mut command = Command::new(&self.binary_path);
        command.arg("-f").arg(video_path);
        if let Some(model_path) = &self.model_path {
            command.arg("-m").arg(model_path);
        }
        if let Some(language) = &self.language {
            command.arg("-l").arg(language);
        }
        command.arg("-of").arg(&output_stem).arg("-osrt");

        let output = command.output().map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                SubtitleError::GenerationFailed(format!(
                    "whisper-cli not found (looked for \"{}\"). Install whisper.cpp and make \
                    sure whisper-cli is on PATH, or set TRANGO_WHISPER_CLI_PATH to its \
                    location — see docs/src/usage.",
                    self.binary_path.display()
                ))
            } else {
                SubtitleError::GenerationFailed(format!("failed to run whisper-cli: {err}"))
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            // whisper-cli's stderr is typically several lines of loader
            // chatter followed by the actual error last — showing only
            // that line keeps the UI's error message readable instead of
            // dumping the whole log.
            let last_line = stderr.lines().rev().find(|line| !line.trim().is_empty());
            return Err(SubtitleError::GenerationFailed(format!(
                "whisper-cli exited with {}: {}",
                output.status,
                last_line.unwrap_or("no error output").trim()
            )));
        }

        if !output_path.is_file() {
            return Err(SubtitleError::GenerationFailed(format!(
                "whisper-cli finished but no subtitle file was found at {}",
                output_path.display()
            )));
        }

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

    #[test]
    fn test_whisper_cli_generator_errors_when_video_file_does_not_exist() {
        // Given: a video path that doesn't exist on disk
        // When:  generating a subtitle for it
        // Then:  it returns SubtitleError::IoError without ever spawning
        //        whisper-cli (whose binary path here doesn't exist either)
        let generator = WhisperCliGenerator::default();

        let result = generator.generate(Path::new("/no/such/video.mp4"));

        assert!(matches!(result, Err(SubtitleError::IoError(_))));
    }

    #[test]
    fn test_whisper_cli_generator_errors_clearly_when_binary_is_missing() {
        // Given: a video file that exists, but a binary_path naming a
        //        whisper-cli that isn't installed
        // When:  generating a subtitle for it
        // Then:  it returns GenerationFailed with a message that explains
        //        the binary is missing, not a generic I/O error
        let video_path = video_fixture("missing-binary");
        let generator = WhisperCliGenerator {
            binary_path: video_path
                .parent()
                .unwrap()
                .join("no-such-whisper-cli-binary"),
            model_path: None,
            language: None,
        };

        let result = generator.generate(&video_path);

        let Err(SubtitleError::GenerationFailed(message)) = result else {
            panic!("expected GenerationFailed, got {result:?}");
        };
        assert!(message.contains("whisper-cli not found"), "{message}");

        std::fs::remove_dir_all(video_path.parent().unwrap())
            .expect("failed to clean up temp test dir");
    }

    /// Writes an executable POSIX shell script standing in for `whisper-cli`
    /// at `dir.join(name)` and returns its path — real speech-to-text isn't
    /// installed on CI/dev machines (`TODO.md` Vaihe 21.5's note), so these
    /// tests exercise `WhisperCliGenerator`'s actual `Command` plumbing
    /// (argument passing, exit status, stdout/stderr handling) against a
    /// fake binary that mimics whisper-cli's `-of`/`-osrt` contract instead.
    #[cfg(unix)]
    fn write_fake_whisper_cli(dir: &Path, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script_path = dir.join(name);
        std::fs::write(&script_path, script).expect("failed to write fake whisper-cli script");
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("failed to make fake whisper-cli script executable");
        script_path
    }

    #[test]
    #[cfg(unix)]
    fn test_whisper_cli_generator_writes_same_stem_srt_via_of_and_osrt_flags() {
        // Given: a fake whisper-cli that writes "<-of value>.srt" and logs
        //        its argv to "<-of value>.args", plus a model_path
        // When:  generating a subtitle for a fixture video
        // Then:  the output path matches StubSubtitleGenerator's same-stem
        //        convention, its contents parse, and -f/-m/-of/-osrt were
        //        all passed as expected (whisper-cli appends ".srt" itself,
        //        so -of must be the stem, not the final .srt path)
        let video_path = video_fixture("writes-same-stem-srt-whisper-cli");
        let dir = video_path.parent().unwrap();
        let binary_path = write_fake_whisper_cli(
            dir,
            "fake-whisper-cli.sh",
            r#"#!/bin/sh
of=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-of" ]; then
        of="$arg"
    fi
    prev="$arg"
done
echo "$@" > "${of}.args"
printf '1\n00:00:00,000 --> 00:00:05,000\n[fake whisper-cli output]\n' > "${of}.srt"
"#,
        );
        let model_path = dir.join("ggml-fake-model.bin");
        let generator = WhisperCliGenerator {
            binary_path,
            model_path: Some(model_path.clone()),
            language: Some("auto".to_string()),
        };
        let expected_output = video_path.with_extension("srt");

        let output_path = generator.generate(&video_path).unwrap();

        assert_eq!(output_path, expected_output);
        let cues = crate::parse_srt(&std::fs::read_to_string(&output_path).unwrap()).unwrap();
        assert_eq!(cues.len(), 1);

        let logged_args =
            std::fs::read_to_string(video_path.with_extension("").with_extension("args"))
                .expect("fake whisper-cli should have logged its args");
        assert!(logged_args.contains(&format!("-f {}", video_path.display())));
        assert!(logged_args.contains(&format!("-m {}", model_path.display())));
        assert!(logged_args.contains("-l auto"));
        assert!(logged_args.contains("-osrt"));

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_whisper_cli_generator_reports_stderr_when_process_fails() {
        // Given: a fake whisper-cli that exits non-zero with a stderr message
        // When:  generating a subtitle for a fixture video
        // Then:  GenerationFailed carries that stderr message, not a
        //        generic failure
        let video_path = video_fixture("process-fails-whisper-cli");
        let dir = video_path.parent().unwrap();
        let binary_path = write_fake_whisper_cli(
            dir,
            "fake-whisper-cli.sh",
            "#!/bin/sh\necho 'failed to load model: bad file' >&2\nexit 1\n",
        );
        let generator = WhisperCliGenerator {
            binary_path,
            model_path: None,
            language: None,
        };

        let result = generator.generate(&video_path);

        let Err(SubtitleError::GenerationFailed(message)) = result else {
            panic!("expected GenerationFailed, got {result:?}");
        };
        assert!(
            message.contains("failed to load model: bad file"),
            "{message}"
        );

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_whisper_cli_generator_errors_when_process_succeeds_without_output_file() {
        // Given: a fake whisper-cli that exits 0 but never writes an .srt
        // When:  generating a subtitle for a fixture video
        // Then:  GenerationFailed explains no subtitle file was produced,
        //        rather than reporting success with a nonexistent path
        let video_path = video_fixture("no-output-file-whisper-cli");
        let dir = video_path.parent().unwrap();
        let binary_path = write_fake_whisper_cli(dir, "fake-whisper-cli.sh", "#!/bin/sh\nexit 0\n");
        let generator = WhisperCliGenerator {
            binary_path,
            model_path: None,
            language: None,
        };

        let result = generator.generate(&video_path);

        let Err(SubtitleError::GenerationFailed(message)) = result else {
            panic!("expected GenerationFailed, got {result:?}");
        };
        assert!(message.contains("no subtitle file was found"), "{message}");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }
}
