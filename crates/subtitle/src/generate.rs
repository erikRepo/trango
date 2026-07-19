//! Subtitle generation: turning a video file into an original-language
//! subtitle track via speech-to-text.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::error::SubtitleError;
use crate::Cue;

/// Generates an original-language `.srt` file for a video or, for the Audio
/// source (`TODO.md` Vaihe 29), an already-audio-only recording.
///
/// Implementations write the subtitle file to disk and return its path.
/// [`StubSubtitleGenerator`] is a placeholder used to build and test the
/// "Generate subtitles" UI flow (`Idle -> Generating -> Done`/`Error`,
/// `TODO.md` Vaihe 20) before a real speech-to-text backend existed;
/// [`WhisperCliGenerator`] (`TODO.md` Vaihe 21.5) is the real
/// implementation, driving the external `whisper-cli` binary.
pub trait SubtitleGenerator {
    /// Generates a subtitle file for `video_path` (a video, or — see this
    /// trait's doc comment — an Audio-source `.wav`), returning the path it
    /// was written to. Returns `SubtitleError::IoError` if `video_path`
    /// doesn't exist or the subtitle file can't be written.
    fn generate(&self, video_path: &Path) -> Result<PathBuf, SubtitleError>;
}

/// A placeholder [`SubtitleGenerator`] that writes a single fixed-text cue
/// spanning the first five seconds of the video, rather than running real
/// speech-to-text. Always writes to `video_path` with its extension
/// replaced by `.srt`, the same same-stem convention
/// `open_media_dialog::matching_subtitle_path` looks for — so a generated
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
/// `whisper-cli` only reads a handful of audio container formats (`flac`,
/// `mp3`, `ogg`, `wav` — notably *not* `.mp4`/`.mkv`/other video
/// containers, and it exits successfully even when it silently failed to
/// read an unsupported file), so `generate` first extracts the video's
/// audio to a temporary 16kHz mono WAV file via `ffmpeg` (also an external
/// process, also not a Cargo dependency) before handing that to
/// `whisper-cli` — unless the input is already a `.wav` (an Audio-source
/// recording/open, `TODO.md` Vaihe 29), in which case that step is skipped
/// and `whisper-cli` reads it directly (see [`is_wav`]).
///
/// `whisper-cli` is asked to write straight to a same-stem `.srt` next to
/// `video_path` via its `-of`/`-osrt` flags (`-of` takes the output path
/// *without* an extension — `whisper-cli` appends `.srt` itself when
/// `-osrt` is set), matching [`StubSubtitleGenerator`]'s convention and
/// the one `open_media_dialog::matching_subtitle_path` looks for, so no
/// raw-text parsing is needed here.
pub struct WhisperCliGenerator {
    /// Path or bare name of the `whisper-cli` binary to run.
    /// [`Default::default`] uses `"whisper-cli"`, resolved via `PATH`.
    pub binary_path: PathBuf,
    /// Path or bare name of the `ffmpeg` binary used to extract audio
    /// before handing it to `whisper-cli`. [`Default::default`] uses
    /// `"ffmpeg"`, resolved via `PATH`.
    pub ffmpeg_path: PathBuf,
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
            ffmpeg_path: PathBuf::from("ffmpeg"),
            model_path: None,
            language: None,
        }
    }
}

impl WhisperCliGenerator {
    /// Runs `ffmpeg` to extract `video_path`'s audio into a 16kHz mono
    /// 16-bit PCM WAV file at `audio_path` — the format whisper.cpp's own
    /// examples recommend, and one `whisper-cli` reads directly without
    /// needing its own (limited) container/codec support.
    fn extract_audio(&self, video_path: &Path, audio_path: &Path) -> Result<(), SubtitleError> {
        tracing::debug!(?video_path, ?audio_path, ffmpeg = ?self.ffmpeg_path, "extracting audio with ffmpeg");
        let output = run_command(
            Command::new(&self.ffmpeg_path)
                .arg("-y")
                .arg("-i")
                .arg(video_path)
                .arg("-ar")
                .arg("16000")
                .arg("-ac")
                .arg("1")
                .arg("-c:a")
                .arg("pcm_s16le")
                .arg(audio_path),
        )
        .map_err(|err| {
            if err.kind() == io::ErrorKind::NotFound {
                SubtitleError::GenerationFailed(format!(
                    "ffmpeg not found (looked for \"{}\"). Install ffmpeg and make sure \
                        it's on PATH, or set TRANGO_FFMPEG_PATH to its location — see \
                        docs/src/usage.",
                    self.ffmpeg_path.display()
                ))
            } else {
                SubtitleError::GenerationFailed(format!("failed to run ffmpeg: {err}"))
            }
        })?;

        if !output.status.success() {
            return Err(SubtitleError::GenerationFailed(format!(
                "ffmpeg exited with {}: {}",
                output.status,
                last_stderr_line(&output.stderr)
            )));
        }
        Ok(())
    }

    /// Runs `whisper-cli` against an already-extracted `audio_path`,
    /// writing an `.srt` to `output_path` via `-of output_stem -osrt` (see
    /// this struct's doc comment for why `-of` needs the extension-less
    /// stem).
    fn run_whisper_cli(
        &self,
        audio_path: &Path,
        output_stem: &Path,
        output_path: &Path,
    ) -> Result<PathBuf, SubtitleError> {
        tracing::info!(
            ?audio_path,
            binary = ?self.binary_path,
            model = ?self.model_path,
            language = ?self.language,
            "running whisper-cli"
        );
        let mut command = Command::new(&self.binary_path);
        command.arg("-f").arg(audio_path);
        if let Some(model_path) = &self.model_path {
            command.arg("-m").arg(model_path);
        }
        if let Some(language) = &self.language {
            command.arg("-l").arg(language);
        }
        command.arg("-of").arg(output_stem).arg("-osrt");

        let output = run_command(&mut command).map_err(|err| {
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
            return Err(SubtitleError::GenerationFailed(format!(
                "whisper-cli exited with {}: {}",
                output.status,
                last_stderr_line(&output.stderr)
            )));
        }

        if !output_path.is_file() {
            return Err(SubtitleError::GenerationFailed(format!(
                "whisper-cli finished but no subtitle file was found at {} — the audio may \
                have had no detectable speech",
                output_path.display()
            )));
        }

        Ok(output_path.to_path_buf())
    }

    /// Transcribes one already-VAD-segmented speech chunk (`TODO.md` Vaihe
    /// 28): writes `samples` to a temporary WAV, runs `whisper-cli` against
    /// it directly (no `ffmpeg` extraction step needed — the samples are
    /// already 16kHz mono PCM, straight from `audio_capture::AudioCapture`),
    /// parses the resulting `.srt`, and offsets every cue's timestamps by
    /// `segment_start` (the segment's position within the overall
    /// recording). Both the temporary WAV and `.srt` are deleted before
    /// returning, successful or not — the same "nothing but the final
    /// transcript persists" principle as `generate`'s own cleanup, just
    /// per-segment instead of per-video.
    pub fn transcribe_segment(
        &self,
        samples: &[i16],
        segment_start: Duration,
    ) -> Result<Vec<Cue>, SubtitleError> {
        let audio_path = temp_segment_audio_path();
        let output_stem = audio_path.with_extension("");
        let output_path = audio_path.with_extension("srt");

        let result = (|| -> Result<Vec<Cue>, SubtitleError> {
            write_pcm_wav(&audio_path, samples)?;
            let srt_path = self.run_whisper_cli(&audio_path, &output_stem, &output_path)?;
            let text = std::fs::read_to_string(srt_path)?;
            crate::parse_srt(&text)
        })();

        let _ = std::fs::remove_file(&audio_path);
        let _ = std::fs::remove_file(&output_path);

        let mut cues = result?;
        for cue in &mut cues {
            cue.start += segment_start;
            cue.end += segment_start;
        }
        Ok(cues)
    }
}

/// Writes `samples` (16kHz mono 16-bit PCM) to `path` as a canonical PCM WAV
/// file. `whisper-cli` reads WAV directly, so unlike `extract_audio` this
/// skips `ffmpeg` entirely — the samples already come out of
/// `audio_capture::AudioCapture` in the exact format needed.
fn write_pcm_wav(path: &Path, samples: &[i16]) -> io::Result<()> {
    const SAMPLE_RATE: u32 = 16_000;
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;
    let data_len = (samples.len() * 2) as u32;
    let byte_rate = SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE) / 8;
    let block_align = CHANNELS * BITS_PER_SAMPLE / 8;

    let mut file = std::fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + data_len).to_le_bytes())?;
    file.write_all(b"WAVE")?;
    file.write_all(b"fmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&CHANNELS.to_le_bytes())?;
    file.write_all(&SAMPLE_RATE.to_le_bytes())?;
    file.write_all(&byte_rate.to_le_bytes())?;
    file.write_all(&block_align.to_le_bytes())?;
    file.write_all(&BITS_PER_SAMPLE.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&data_len.to_le_bytes())?;
    for sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    Ok(())
}

/// A process-unique temporary WAV path for one speech segment, e.g.
/// `/tmp/trango-segment-<pid>-<counter>.wav` — unique per call within the
/// process (via a monotonic counter, not wall-clock time), mirroring
/// `temp_audio_path`'s scheme so concurrent per-segment transcriptions never
/// collide.
fn temp_segment_audio_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "trango-segment-{}-{counter}.wav",
        std::process::id()
    ))
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

        // Audio source recordings/opens are always already a WAV
        // (`open_media_dialog`'s AUDIO_EXTENSIONS, `main.rs`'s CurrentMedia
        // doc comment) — TODO.md Vaihe 29 runs "Generate subtitles" on that
        // file directly, so extract_audio's ffmpeg step (needed only to
        // pull audio out of a video container) is skipped for it.
        if is_wav(video_path) {
            return self.run_whisper_cli(video_path, &output_stem, &output_path);
        }

        let audio_path = temp_audio_path(video_path);
        let result = self
            .extract_audio(video_path, &audio_path)
            .and_then(|()| self.run_whisper_cli(&audio_path, &output_stem, &output_path));
        let _ = std::fs::remove_file(&audio_path);
        result
    }
}

/// Whether `path` has a (case-insensitive) `.wav` extension — the format
/// both `audio_capture::AudioCapture` recordings and the Audio source's
/// "Open…" dialog are restricted to, so it doubles as "this is already
/// audio, not a video container" for [`WhisperCliGenerator::generate`].
fn is_wav(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
}

/// A process-unique temporary WAV path for `video_path`'s extracted audio,
/// e.g. `/tmp/trango-whisper-<pid>-<counter>-<video stem>.wav` — unique
/// per call within the process (via a monotonic counter, not wall-clock
/// time) so concurrent or repeated `generate` calls never collide.
fn temp_audio_path(video_path: &Path) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let stem = video_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("audio");
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "trango-whisper-{}-{counter}-{stem}.wav",
        std::process::id()
    ))
}

/// Runs `command`, retrying briefly (up to 4 times, 20ms apart) if the OS
/// reports `ExecutableFileBusy` (errno `ETXTBSY`) — a transient race that
/// can happen if the target binary was written to disk moments earlier
/// (its write handle not fully released yet when exec is attempted; this
/// crate's own tests hit it occasionally, writing a fresh fake binary
/// immediately before running it) rather than an installed system binary
/// that's been sitting on disk unchanged.
pub(crate) fn run_command(command: &mut Command) -> io::Result<std::process::Output> {
    for attempt in 0..5 {
        match command.output() {
            Err(err) if attempt < 4 && err.kind() == io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            result => return result,
        }
    }
    unreachable!()
}

/// The last non-empty line of `stderr` — external tools' real error tends
/// to be the final line after loader/setup chatter, so showing just that
/// keeps `GenerationFailed`'s message readable instead of dumping the
/// whole log.
pub(crate) fn last_stderr_line(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no error output")
        .trim()
        .to_string()
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

    /// A fresh temp dir with a fake (empty) `some_recording.wav` inside it —
    /// stands in for an Audio-source recording/open (`TODO.md` Vaihe 29),
    /// where `generate` is expected to skip `extract_audio` entirely.
    fn audio_fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-generate-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        let audio_path = dir.join("some_recording.wav");
        std::fs::write(&audio_path, b"").expect("failed to write fixture audio file");
        audio_path
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
        //        ffmpeg or whisper-cli
        let generator = WhisperCliGenerator::default();

        let result = generator.generate(Path::new("/no/such/video.mp4"));

        assert!(matches!(result, Err(SubtitleError::IoError(_))));
    }

    #[test]
    fn test_temp_audio_path_is_unique_per_call_and_ends_with_wav() {
        // Given/When: two temp audio paths for the same video
        // Then:  they differ (the monotonic counter, not wall-clock time,
        //        guarantees this even if called back-to-back) and both
        //        end in .wav
        let video_path = Path::new("/videos/some_video.mp4");

        let first = temp_audio_path(video_path);
        let second = temp_audio_path(video_path);

        assert_ne!(first, second);
        assert!(first.extension().is_some_and(|ext| ext == "wav"));
        assert!(second.extension().is_some_and(|ext| ext == "wav"));
    }

    /// Writes an executable POSIX shell script standing in for an external
    /// tool at `dir.join(name)` and returns its path — real ffmpeg/
    /// whisper-cli behavior isn't something CI/dev machines can rely on
    /// having installed, so these tests exercise `WhisperCliGenerator`'s
    /// actual `Command` plumbing (argument passing, exit status,
    /// stdout/stderr handling) against fake binaries instead.
    #[cfg(unix)]
    fn write_fake_binary(dir: &Path, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script_path = dir.join(name);
        std::fs::write(&script_path, script).expect("failed to write fake binary script");
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("failed to make fake binary script executable");
        script_path
    }

    /// A fake `whisper-cli` that writes `"<-of value>.srt"` (one cue) and
    /// logs its argv to `"<-of value>.args"` — used directly (bypassing
    /// `extract_audio`, since these tests are only concerned with
    /// `run_whisper_cli`'s own argument-building and error handling) with
    /// `audio_fixture` standing in for whatever `extract_audio` would
    /// otherwise have produced.
    #[cfg(unix)]
    const FAKE_WHISPER_CLI_SCRIPT: &str = r#"#!/bin/sh
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
"#;

    #[test]
    #[cfg(unix)]
    fn test_run_whisper_cli_writes_same_stem_srt_with_expected_flags() {
        // Given: a fake whisper-cli (see FAKE_WHISPER_CLI_SCRIPT) plus a
        //        model_path and language
        // When:  running it directly against an arbitrary "audio" fixture
        //        file (standing in for extract_audio's output)
        // Then:  the output path matches the same-stem convention, its
        //        contents parse, and -f/-m/-l/-of/-osrt were all passed as
        //        expected (whisper-cli appends ".srt" itself, so -of must
        //        be the stem, not the final .srt path)
        let audio_path = video_fixture("run-whisper-cli-flags");
        let dir = audio_path.parent().unwrap();
        let binary_path = write_fake_binary(dir, "fake-whisper-cli.sh", FAKE_WHISPER_CLI_SCRIPT);
        let model_path = dir.join("ggml-fake-model.bin");
        let generator = WhisperCliGenerator {
            binary_path,
            model_path: Some(model_path.clone()),
            language: Some("auto".to_string()),
            ..WhisperCliGenerator::default()
        };
        let output_stem = audio_path.with_extension("");
        let output_path = audio_path.with_extension("srt");

        let result = generator.run_whisper_cli(&audio_path, &output_stem, &output_path);

        let output_path = result.unwrap();
        assert_eq!(output_path, audio_path.with_extension("srt"));
        let cues = crate::parse_srt(&std::fs::read_to_string(&output_path).unwrap()).unwrap();
        assert_eq!(cues.len(), 1);

        let logged_args = std::fs::read_to_string(output_stem.with_extension("args"))
            .expect("fake whisper-cli should have logged its args");
        assert!(logged_args.contains(&format!("-f {}", audio_path.display())));
        assert!(logged_args.contains(&format!("-m {}", model_path.display())));
        assert!(logged_args.contains("-l auto"));
        assert!(logged_args.contains("-osrt"));

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_run_whisper_cli_errors_clearly_when_binary_is_missing() {
        // Given: a binary_path naming a whisper-cli that isn't installed
        // When:  running it
        // Then:  GenerationFailed explains the binary is missing, not a
        //        generic I/O error
        let audio_path = video_fixture("missing-binary");
        let dir = audio_path.parent().unwrap();
        let generator = WhisperCliGenerator {
            binary_path: dir.join("no-such-whisper-cli-binary"),
            ..WhisperCliGenerator::default()
        };

        let result = generator.run_whisper_cli(
            &audio_path,
            &audio_path.with_extension(""),
            &audio_path.with_extension("srt"),
        );

        let Err(SubtitleError::GenerationFailed(message)) = result else {
            panic!("expected GenerationFailed, got {result:?}");
        };
        assert!(message.contains("whisper-cli not found"), "{message}");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_run_whisper_cli_reports_stderr_when_process_fails() {
        // Given: a fake whisper-cli that exits non-zero with a stderr
        //        message
        // When:  running it
        // Then:  GenerationFailed carries that stderr message, not a
        //        generic failure
        let audio_path = video_fixture("process-fails-whisper-cli");
        let dir = audio_path.parent().unwrap();
        let binary_path = write_fake_binary(
            dir,
            "fake-whisper-cli.sh",
            "#!/bin/sh\necho 'failed to load model: bad file' >&2\nexit 1\n",
        );
        let generator = WhisperCliGenerator {
            binary_path,
            ..WhisperCliGenerator::default()
        };

        let result = generator.run_whisper_cli(
            &audio_path,
            &audio_path.with_extension(""),
            &audio_path.with_extension("srt"),
        );

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
    fn test_run_whisper_cli_errors_when_process_succeeds_without_output_file() {
        // Given: a fake whisper-cli that exits 0 but never writes an .srt
        //        (whisper-cli does this for real when it silently fails to
        //        read an unsupported input file — see this module's doc
        //        comment)
        // When:  running it
        // Then:  GenerationFailed explains no subtitle file was produced,
        //        rather than reporting success with a nonexistent path
        let audio_path = video_fixture("no-output-file-whisper-cli");
        let dir = audio_path.parent().unwrap();
        let binary_path = write_fake_binary(dir, "fake-whisper-cli.sh", "#!/bin/sh\nexit 0\n");
        let generator = WhisperCliGenerator {
            binary_path,
            ..WhisperCliGenerator::default()
        };

        let result = generator.run_whisper_cli(
            &audio_path,
            &audio_path.with_extension(""),
            &audio_path.with_extension("srt"),
        );

        let Err(SubtitleError::GenerationFailed(message)) = result else {
            panic!("expected GenerationFailed, got {result:?}");
        };
        assert!(message.contains("no subtitle file was found"), "{message}");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_extract_audio_runs_ffmpeg_with_expected_flags() {
        // Given: a fake ffmpeg that logs its argv to "<output>.args" and
        //        writes some content to the output path
        // When:  extracting audio from a fixture video
        // Then:  it succeeds, and ffmpeg was invoked with -i <video>,
        //        16kHz mono PCM flags, and the given output path
        let video_path = video_fixture("extract-audio-flags");
        let dir = video_path.parent().unwrap();
        let audio_path = dir.join("extracted.wav");
        let ffmpeg_path = write_fake_binary(
            dir,
            "fake-ffmpeg.sh",
            r#"#!/bin/sh
last=""
for arg in "$@"; do
    last="$arg"
done
echo "$@" > "${last}.args"
printf 'fake wav content' > "$last"
"#,
        );
        let generator = WhisperCliGenerator {
            ffmpeg_path,
            ..WhisperCliGenerator::default()
        };

        generator.extract_audio(&video_path, &audio_path).unwrap();

        let logged_args = std::fs::read_to_string(format!("{}.args", audio_path.display()))
            .expect("fake ffmpeg should have logged its args");
        assert!(logged_args.contains(&format!("-i {}", video_path.display())));
        assert!(logged_args.contains("-ar 16000"));
        assert!(logged_args.contains("-ac 1"));
        assert!(logged_args.contains("pcm_s16le"));
        assert_eq!(
            std::fs::read_to_string(&audio_path).unwrap(),
            "fake wav content"
        );

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_extract_audio_errors_clearly_when_ffmpeg_is_missing() {
        // Given: an ffmpeg_path naming a binary that isn't installed
        // When:  extracting audio
        // Then:  GenerationFailed explains ffmpeg is missing
        let video_path = video_fixture("missing-ffmpeg");
        let dir = video_path.parent().unwrap();
        let generator = WhisperCliGenerator {
            ffmpeg_path: dir.join("no-such-ffmpeg-binary"),
            ..WhisperCliGenerator::default()
        };

        let result = generator.extract_audio(&video_path, &dir.join("out.wav"));

        let Err(SubtitleError::GenerationFailed(message)) = result else {
            panic!("expected GenerationFailed, got {result:?}");
        };
        assert!(message.contains("ffmpeg not found"), "{message}");

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    /// Filenames in the system temp dir matching `temp_segment_audio_path`'s
    /// `.wav`/`.srt` naming scheme — used to check `transcribe_segment`
    /// doesn't leave either behind. Excludes the fake whisper-cli's own
    /// `.args` log (a test-only side effect, not something real
    /// `whisper-cli` produces) so that file doesn't skew the count.
    fn temp_segment_files() -> Vec<PathBuf> {
        std::fs::read_dir(std::env::temp_dir())
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                let matches_prefix = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("trango-segment-"));
                let matches_ext = path
                    .extension()
                    .is_some_and(|ext| ext == "wav" || ext == "srt");
                matches_prefix && matches_ext
            })
            .collect()
    }

    /// Serializes the two `transcribe_segment` cleanup tests below against
    /// each other — both count `trango-segment-*` files across the whole
    /// (process-wide) system temp dir, which races if `cargo test` runs
    /// them concurrently on separate threads within the same process (and
    /// therefore the same `std::process::id()`-based filename prefix).
    static TEMP_SEGMENT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    #[cfg(unix)]
    fn test_transcribe_segment_offsets_cue_timestamps_and_cleans_up_temp_files() {
        // Given: a fake whisper-cli that writes one fixed 0-5s cue, and a
        //        segment starting 10s into the overall recording
        // When:  transcribing a short silent segment
        // Then:  the returned cue's timestamps are shifted by the segment's
        //        start, and no temp WAV/SRT is left in the system temp dir
        //        afterward
        let _guard = TEMP_SEGMENT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = video_fixture("transcribe-segment");
        let dir = dir.parent().unwrap();
        let binary_path = write_fake_binary(dir, "fake-whisper-cli.sh", FAKE_WHISPER_CLI_SCRIPT);
        let generator = WhisperCliGenerator {
            binary_path,
            ..WhisperCliGenerator::default()
        };
        let samples = vec![0i16; 1_600]; // 100ms of silence at 16kHz

        let before = temp_segment_files().len();
        let cues = generator
            .transcribe_segment(&samples, Duration::from_secs(10))
            .unwrap();
        let after = temp_segment_files().len();

        assert_eq!(before, after);
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].start, Duration::from_secs(10));
        assert_eq!(cues[0].end, Duration::from_secs(15));

        let pid_prefix = format!("trango-segment-{}-", std::process::id());
        for entry in std::fs::read_dir(std::env::temp_dir()).unwrap().flatten() {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&pid_prefix))
            {
                let _ = std::fs::remove_file(path);
            }
        }
        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_transcribe_segment_errors_and_still_cleans_up_when_whisper_cli_fails() {
        // Given: a fake whisper-cli that exits non-zero
        // When:  transcribing a segment
        // Then:  GenerationFailed is returned and no temp WAV/SRT is left
        //        behind
        let _guard = TEMP_SEGMENT_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = video_fixture("transcribe-segment-error");
        let dir = dir.parent().unwrap();
        let binary_path = write_fake_binary(
            dir,
            "fake-whisper-cli.sh",
            "#!/bin/sh\necho 'boom' >&2\nexit 1\n",
        );
        let generator = WhisperCliGenerator {
            binary_path,
            ..WhisperCliGenerator::default()
        };

        let before = temp_segment_files().len();
        let result = generator.transcribe_segment(&[0i16; 100], Duration::from_secs(0));
        let after = temp_segment_files().len();

        assert!(matches!(result, Err(SubtitleError::GenerationFailed(_))));
        assert_eq!(before, after);

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_generate_extracts_audio_before_running_whisper_cli() {
        // Given: a fake ffmpeg that writes a marker string as its "audio"
        //        output, and a fake whisper-cli that only succeeds if the
        //        file it's given via -f contains that exact marker
        // When:  generating a subtitle for a fixture video (the real,
        //        public SubtitleGenerator::generate entry point)
        // Then:  it succeeds — proving generate() actually feeds
        //        extract_audio's output into whisper-cli, not the raw
        //        video file, without needing to predict the temp audio
        //        path's exact (process-unique) name
        let video_path = video_fixture("generate-extracts-audio-first");
        let dir = video_path.parent().unwrap();
        let ffmpeg_path = write_fake_binary(
            dir,
            "fake-ffmpeg.sh",
            r#"#!/bin/sh
last=""
for arg in "$@"; do
    last="$arg"
done
printf 'FFMPEG_OUTPUT_MARKER' > "$last"
"#,
        );
        let binary_path = write_fake_binary(
            dir,
            "fake-whisper-cli.sh",
            r#"#!/bin/sh
of=""
f=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-of" ]; then of="$arg"; fi
    if [ "$prev" = "-f" ]; then f="$arg"; fi
    prev="$arg"
done
content=$(cat "$f")
if [ "$content" != "FFMPEG_OUTPUT_MARKER" ]; then
    echo "input was not ffmpeg's output: $content" >&2
    exit 1
fi
printf '1\n00:00:00,000 --> 00:00:05,000\n[fake whisper-cli output]\n' > "${of}.srt"
"#,
        );
        let generator = WhisperCliGenerator {
            binary_path,
            ffmpeg_path,
            ..WhisperCliGenerator::default()
        };

        let output_path = generator.generate(&video_path).unwrap();

        assert_eq!(output_path, video_path.with_extension("srt"));
        let cues = crate::parse_srt(&std::fs::read_to_string(&output_path).unwrap()).unwrap();
        assert_eq!(cues.len(), 1);

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }

    #[test]
    #[cfg(unix)]
    fn test_generate_skips_ffmpeg_extraction_for_wav_input() {
        // Given: a WAV fixture (already audio, as Audio-source recordings/
        //        opens always are, TODO.md Vaihe 29) and an ffmpeg_path
        //        naming a binary that doesn't exist
        // When:  generating a subtitle for it (the real, public
        //        SubtitleGenerator::generate entry point)
        // Then:  it still succeeds — proving generate() fed the WAV
        //        straight to whisper-cli without ever invoking ffmpeg
        let audio_path = audio_fixture("skips-ffmpeg-for-wav");
        let dir = audio_path.parent().unwrap();
        let binary_path = write_fake_binary(dir, "fake-whisper-cli.sh", FAKE_WHISPER_CLI_SCRIPT);
        let generator = WhisperCliGenerator {
            binary_path,
            ffmpeg_path: dir.join("no-such-ffmpeg-binary"),
            ..WhisperCliGenerator::default()
        };

        let output_path = generator.generate(&audio_path).unwrap();

        assert_eq!(output_path, audio_path.with_extension("srt"));
        let cues = crate::parse_srt(&std::fs::read_to_string(&output_path).unwrap()).unwrap();
        assert_eq!(cues.len(), 1);

        std::fs::remove_dir_all(dir).expect("failed to clean up temp test dir");
    }
}
