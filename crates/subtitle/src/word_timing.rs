//! Per-word audio timing within an already-known sentence span
//! (`TODO.md` Vaihe 31), via a second, narrowly-scoped `whisper-cli` pass
//! over just that span's audio — `-dtw` gives cross-attention-based
//! word-level timestamps, far more accurate than whisper.cpp's default
//! token-timing estimate.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::error::SubtitleError;
use crate::generate::{last_stderr_line, run_command};

/// One word's audio span, absolute within the source file passed to
/// [`WhisperCliWordSegmenter::segment_words`] — already offset by the
/// sentence's own start, so callers don't have to add it back in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordTiming {
    /// The word's text, as transcribed by `whisper-cli` for this span.
    pub word: String,
    /// Time at which the word starts, absolute within the source file.
    pub start: Duration,
    /// Time at which the word ends, absolute within the source file.
    pub end: Duration,
}

/// Derives per-word timing within one sentence's `[cue_start, cue_end)`
/// span by re-running `whisper-cli` against just that span's audio, cut
/// out with `ffmpeg` — not the whole video/audio file, since DTW
/// alignment quality depends on a short, focused clip rather than a long
/// one.
///
/// Mirrors `WhisperCliGenerator`'s external-process fields/pattern
/// (`generate.rs`) rather than sharing a type with it, since this reruns
/// `whisper-cli` with a different flag set (`-ml 1 -sow` for one word per
/// output cue, plus an optional `-dtw` preset) aimed at a different goal
/// (word timing within an already-known sentence, not a full transcript).
///
/// Deliberately has **no** VAD (Voice Activity Detection) support — see
/// `docs/src/developer/specs.md`'s "VAD tried and fully reverted" entry.
/// VAD was tried here too, on the theory that this struct's fixed clip
/// boundary (the caller already knows the sentence's own start/end) made
/// it safe from the segment-redrawing regression found in
/// `WhisperCliGenerator`. It wasn't: `whisper-cli` decodes each VAD
/// speech segment independently, with no acoustic context across
/// segments — if VAD's frame-level speech/silence classifier dips
/// mid-word (plausible for many phonetic structures), one real word gets
/// decoded as two disconnected fragments and `-sow` dutifully splits
/// them into two separate words, breaking exactly the "one clip per real
/// word" guarantee this struct exists to provide.
pub struct WhisperCliWordSegmenter {
    /// Path or bare name of the `whisper-cli` binary. [`Default::default`]
    /// uses `"whisper-cli"`, resolved via `PATH`.
    pub binary_path: PathBuf,
    /// Path or bare name of `ffmpeg`, used to cut the sentence's audio
    /// span out of the source file before handing it to `whisper-cli`.
    /// [`Default::default`] uses `"ffmpeg"`, resolved via `PATH`.
    pub ffmpeg_path: PathBuf,
    /// Path to the ggml/gguf model file to pass via `-m`. `None` omits
    /// the flag, letting `whisper-cli` fall back to its own default.
    pub model_path: Option<PathBuf>,
    /// The `-l`/`--language` value to pass. `None` omits the flag.
    pub language: Option<String>,
    /// The `-dtw` preset to pass (e.g. `"large.v3"` — see
    /// [`dtw_preset_for_model`]). `None` omits `-dtw` entirely, falling
    /// back to whisper.cpp's non-DTW word timestamps rather than failing
    /// outright on an unrecognized model.
    pub dtw_preset: Option<String>,
}

/// `whisper-cli`'s `--dtw` preset tokens (`TODO.md` Vaihe 31), ordered
/// longest/most-specific first so [`dtw_preset_for_model`]'s substring
/// scan prefers e.g. `"large.v3.turbo"` over the less specific
/// `"large.v3"`, and `"medium.en"` over `"medium"`.
const DTW_PRESETS: &[&str] = &[
    "large.v3.turbo",
    "large.v3",
    "large.v2",
    "large.v1",
    "medium.en",
    "medium",
    "small.en",
    "small",
    "base.en",
    "base",
    "tiny.en",
    "tiny",
];

/// The `-dtw` preset to pass `whisper-cli` for `model_path`, derived from
/// its filename, or `None` if no known preset can be confidently
/// inferred — e.g. a custom fine-tune with an unrecognized name.
/// `whisper-cli` hard-errors on an unrecognized `--dtw` value, so `None`
/// means the caller should omit the flag entirely rather than guess
/// wrong (see [`WhisperCliWordSegmenter::dtw_preset`]).
///
/// whisper.cpp model filenames commonly use a dash before the version
/// (e.g. `ggml-large-v3.bin`), while its own `--dtw` preset tokens use a
/// dot (`"large.v3"`) — so the filename's stem is lowercased and has
/// `-`/`_` normalized to `.` before matching against [`DTW_PRESETS`].
/// Mirrors the `app` crate's `model_picker::language_flag`'s
/// filename-convention-sniffing style, just kept in this crate (rather
/// than alongside `language_flag`) since it's pure filename parsing with
/// no UI dependency, and this crate's other `WhisperCliWordSegmenter`
/// fields already need it.
pub fn dtw_preset_for_model(model_path: &Path) -> Option<&'static str> {
    let stem = model_path.file_stem()?.to_str()?.to_lowercase();
    let normalized = stem.replace(['-', '_'], ".");
    DTW_PRESETS
        .iter()
        .find(|preset| normalized.contains(*preset))
        .copied()
}

impl Default for WhisperCliWordSegmenter {
    fn default() -> Self {
        Self {
            binary_path: PathBuf::from("whisper-cli"),
            ffmpeg_path: PathBuf::from("ffmpeg"),
            model_path: None,
            language: None,
            dtw_preset: None,
        }
    }
}

impl WhisperCliWordSegmenter {
    /// Cuts `[cue_start, cue_end)` out of `source_path`'s audio into
    /// `clip_path` as 16kHz mono `pcm_s16le` (the format `whisper-cli`
    /// reads directly) — `-ss`/`-to` given as *output* options (after
    /// `-i`) rather than input-seeking ones, so `-to` is an absolute
    /// source timestamp and the cut is sample-accurate rather than
    /// snapped to the nearest video keyframe.
    fn extract_clip(
        &self,
        source_path: &Path,
        cue_start: Duration,
        cue_end: Duration,
        clip_path: &Path,
    ) -> Result<(), SubtitleError> {
        tracing::debug!(
            ?source_path,
            ?cue_start,
            ?cue_end,
            ?clip_path,
            "cutting sentence audio clip with ffmpeg"
        );
        let output = run_command(
            Command::new(&self.ffmpeg_path)
                .arg("-y")
                .arg("-i")
                .arg(source_path)
                .arg("-ss")
                .arg(format_seconds(cue_start))
                .arg("-to")
                .arg(format_seconds(cue_end))
                .arg("-ar")
                .arg("16000")
                .arg("-ac")
                .arg("1")
                .arg("-c:a")
                .arg("pcm_s16le")
                .arg(clip_path),
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

    /// Runs `whisper-cli` against an already-cut `clip_path`, asking for
    /// one-word-per-cue output (`-ml 1 -sow`) with `-dtw` added only when
    /// `self.dtw_preset` is set — see this struct's `dtw_preset` doc
    /// comment for why an unset preset just omits the flag rather than
    /// erroring.
    fn run_whisper_cli_word_level(
        &self,
        clip_path: &Path,
        output_stem: &Path,
        output_path: &Path,
    ) -> Result<(), SubtitleError> {
        tracing::info!(
            ?clip_path,
            binary = ?self.binary_path,
            model = ?self.model_path,
            language = ?self.language,
            dtw_preset = ?self.dtw_preset,
            "running whisper-cli for word-level timing"
        );
        let mut command = Command::new(&self.binary_path);
        command.arg("-f").arg(clip_path);
        if let Some(model_path) = &self.model_path {
            command.arg("-m").arg(model_path);
        }
        if let Some(language) = &self.language {
            command.arg("-l").arg(language);
        }
        command.arg("-ml").arg("1").arg("-sow");
        if let Some(dtw_preset) = &self.dtw_preset {
            command.arg("-dtw").arg(dtw_preset);
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
                "whisper-cli finished but no subtitle file was found at {} — the clip may \
                have had no detectable speech",
                output_path.display()
            )));
        }

        Ok(())
    }

    /// Derives per-word timing within `[cue_start, cue_end)` of
    /// `source_path`'s audio: cuts that span out via [`Self::extract_clip`],
    /// runs `whisper-cli` against the clip via
    /// [`Self::run_whisper_cli_word_level`], then parses the resulting
    /// `.srt` via [`crate::srt::parse_srt_blocks`] (one word per block, so
    /// no separate JSON/token parsing is needed) and offsets every
    /// timestamp by `cue_start` so callers get timings absolute within
    /// `source_path`, not the clip.
    ///
    /// Uses `parse_srt_blocks` rather than the stricter [`crate::parse_srt`]
    /// because `whisper-cli`'s DTW timestamps can occasionally collapse a
    /// word right at the clip's edge to a zero (or, rarely, negative)
    /// duration — observed in real use on a word landing exactly at a
    /// clip boundary. Such a block is dropped (logged at `warn`) instead
    /// of failing the whole sentence's segmentation over one degenerate
    /// word.
    ///
    /// Always deletes the temporary clip and `.srt`, on success or error.
    pub fn segment_words(
        &self,
        source_path: &Path,
        cue_start: Duration,
        cue_end: Duration,
    ) -> Result<Vec<WordTiming>, SubtitleError> {
        let clip_path = temp_clip_audio_path();
        let output_stem = clip_path.with_extension("");
        let output_path = clip_path.with_extension("srt");

        let result = (|| -> Result<Vec<WordTiming>, SubtitleError> {
            self.extract_clip(source_path, cue_start, cue_end, &clip_path)?;
            self.run_whisper_cli_word_level(&clip_path, &output_stem, &output_path)?;
            let text = std::fs::read_to_string(&output_path)?;
            let blocks = crate::srt::parse_srt_blocks(&text)?;
            Ok(blocks
                .into_iter()
                .filter_map(|(index, start, end, word)| {
                    if end <= start {
                        tracing::warn!(
                            index,
                            ?start,
                            ?end,
                            %word,
                            "dropping zero/negative-duration word from whisper-cli word-level output"
                        );
                        return None;
                    }
                    let word = word.trim().to_string();
                    if word.is_empty() {
                        tracing::warn!(
                            index,
                            ?start,
                            ?end,
                            "dropping empty-text word from whisper-cli word-level output \
                             (e.g. a VAD-detected speech blip with nothing transcribed)"
                        );
                        return None;
                    }
                    Some(WordTiming {
                        word,
                        start: cue_start + start,
                        end: cue_start + end,
                    })
                })
                .collect())
        })();

        let _ = std::fs::remove_file(&clip_path);
        let _ = std::fs::remove_file(&output_path);

        result
    }
}

/// Formats `duration` as `ffmpeg`'s `-ss`/`-to` expect: seconds with
/// millisecond precision (e.g. `"12.345"`), not `HH:MM:SS,mmm`.
fn format_seconds(duration: Duration) -> String {
    format!("{:.3}", duration.as_secs_f64())
}

/// A process-unique temporary WAV path for one sentence's cut audio clip,
/// e.g. `/tmp/trango-word-timing-<pid>-<counter>.wav` — unique per call
/// within the process (via a monotonic counter, not wall-clock time),
/// mirroring `generate.rs`'s `temp_segment_audio_path` scheme.
fn temp_clip_audio_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "trango-word-timing-{}-{counter}.wav",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes an executable POSIX shell script standing in for an
    /// external tool at `dir.join(name)` and returns its path — mirrors
    /// `generate.rs`'s `write_fake_binary`, duplicated here rather than
    /// exposed across files for one small test helper.
    #[cfg(unix)]
    fn write_fake_binary(dir: &Path, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script_path = dir.join(name);
        std::fs::write(&script_path, script).expect("failed to write fake binary script");
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("failed to make fake binary script executable");
        script_path
    }

    /// A fresh temp dir for one test, named after it.
    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-word-timing-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    /// A fake `ffmpeg` that logs its argv to `"<last arg>.args"` and
    /// writes fixed content to its output path (the last argument).
    #[cfg(unix)]
    const FAKE_FFMPEG_SCRIPT: &str = r#"#!/bin/sh
last=""
for arg in "$@"; do
    last="$arg"
done
echo "$@" > "${last}.args"
printf 'fake clip audio' > "$last"
"#;

    /// A fake `whisper-cli` that writes a fixed two-word `.srt` to
    /// `"<-of value>.srt"` and logs its argv to `last-invocation.args`
    /// *next to the script itself* (`dirname "$0"`) rather than next to
    /// `-of`'s value — `-of` is `segment_words`'s own unpredictable temp
    /// path (system temp dir + a monotonic counter), so tests that need
    /// to inspect the logged argv (e.g. checking `-dtw`) read it from the
    /// script's own, test-controlled directory instead.
    #[cfg(unix)]
    const FAKE_WHISPER_CLI_SCRIPT: &str = r#"#!/bin/sh
script_dir=$(dirname "$0")
of=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-of" ]; then
        of="$arg"
    fi
    prev="$arg"
done
echo "$@" > "$script_dir/last-invocation.args"
printf '1\n00:00:00,000 --> 00:00:00,500\nhello\n\n2\n00:00:00,600 --> 00:00:01,200\nworld\n' > "${of}.srt"
"#;

    #[test]
    fn test_dtw_preset_for_model_matches_known_presets_by_normalized_filename() {
        // Given/When/Then: dash-separated whisper.cpp filenames resolve to
        //                   their dot-separated --dtw preset token, the
        //                   more specific ".en"/"turbo" variants win over
        //                   their shorter prefixes, and a real local
        //                   fine-tune's name ("ggml-large-v3-turbo-ivrit.bin")
        //                   still resolves to its base architecture's preset
        assert_eq!(
            dtw_preset_for_model(Path::new("/models/ggml-base.en.bin")),
            Some("base.en")
        );
        assert_eq!(
            dtw_preset_for_model(Path::new("/models/ggml-large-v3.bin")),
            Some("large.v3")
        );
        assert_eq!(
            dtw_preset_for_model(Path::new("/models/ggml-medium.bin")),
            Some("medium")
        );
        assert_eq!(
            dtw_preset_for_model(Path::new("/models/ggml-large-v3-turbo-ivrit.bin")),
            Some("large.v3.turbo")
        );
    }

    #[test]
    fn test_dtw_preset_for_model_returns_none_for_unrecognized_filename() {
        // Given: a model filename with no whisper.cpp size/version token
        // When:  deriving its dtw preset
        // Then:  None, so the caller omits -dtw rather than guessing wrong
        assert_eq!(
            dtw_preset_for_model(Path::new("/models/my-custom-model.bin")),
            None
        );
    }

    #[test]
    #[cfg(unix)]
    fn test_extract_clip_runs_ffmpeg_with_ss_and_to_flags() {
        // Given: a fake ffmpeg logging its argv
        // When:  cutting a [1.5s, 3.25s) clip out of a source file
        // Then:  ffmpeg is invoked with -i <source>, -ss 1.500, -to 3.250,
        //        and the usual 16kHz mono pcm_s16le flags
        let dir = test_dir("extract-clip-flags");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let clip_path = dir.join("clip.wav");
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);
        let segmenter = WhisperCliWordSegmenter {
            ffmpeg_path,
            ..WhisperCliWordSegmenter::default()
        };

        segmenter
            .extract_clip(
                &source_path,
                Duration::from_millis(1500),
                Duration::from_millis(3250),
                &clip_path,
            )
            .unwrap();

        let logged_args = std::fs::read_to_string(format!("{}.args", clip_path.display())).unwrap();
        assert!(logged_args.contains(&format!("-i {}", source_path.display())));
        assert!(logged_args.contains("-ss 1.500"));
        assert!(logged_args.contains("-to 3.250"));
        assert!(logged_args.contains("-ar 16000"));
        assert!(logged_args.contains("-ac 1"));
        assert!(logged_args.contains("pcm_s16le"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_segment_words_passes_dtw_preset_when_set_and_omits_it_when_none() {
        // Given: a segmenter with dtw_preset set, and one without (each
        //        with its own fake whisper-cli, so their logged argv
        //        don't overwrite each other)
        // When:  segmenting words (fake ffmpeg/whisper-cli, no real audio)
        // Then:  -dtw <preset> is passed only when dtw_preset is Some;
        //        -ml 1 -sow are always passed
        let _guard = TEMP_WORD_TIMING_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = test_dir("dtw-preset-flag");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);

        let with_dtw_dir = dir.join("with-dtw");
        std::fs::create_dir_all(&with_dtw_dir).unwrap();
        let with_dtw_binary = write_fake_binary(
            &with_dtw_dir,
            "fake-whisper-cli.sh",
            FAKE_WHISPER_CLI_SCRIPT,
        );
        let with_dtw = WhisperCliWordSegmenter {
            binary_path: with_dtw_binary,
            ffmpeg_path: ffmpeg_path.clone(),
            dtw_preset: Some("large.v3".to_string()),
            ..WhisperCliWordSegmenter::default()
        };
        with_dtw
            .segment_words(&source_path, Duration::from_secs(0), Duration::from_secs(2))
            .unwrap();
        let with_dtw_args =
            std::fs::read_to_string(with_dtw_dir.join("last-invocation.args")).unwrap();
        assert!(with_dtw_args.contains("-dtw large.v3"), "{with_dtw_args}");
        assert!(with_dtw_args.contains("-ml 1"), "{with_dtw_args}");
        assert!(with_dtw_args.contains("-sow"), "{with_dtw_args}");

        let without_dtw_dir = dir.join("without-dtw");
        std::fs::create_dir_all(&without_dtw_dir).unwrap();
        let without_dtw_binary = write_fake_binary(
            &without_dtw_dir,
            "fake-whisper-cli.sh",
            FAKE_WHISPER_CLI_SCRIPT,
        );
        let without_dtw = WhisperCliWordSegmenter {
            binary_path: without_dtw_binary,
            ffmpeg_path,
            dtw_preset: None,
            ..WhisperCliWordSegmenter::default()
        };
        without_dtw
            .segment_words(&source_path, Duration::from_secs(0), Duration::from_secs(2))
            .unwrap();
        let without_dtw_args =
            std::fs::read_to_string(without_dtw_dir.join("last-invocation.args")).unwrap();
        assert!(!without_dtw_args.contains("-dtw"), "{without_dtw_args}");
        assert!(without_dtw_args.contains("-ml 1"), "{without_dtw_args}");
        assert!(without_dtw_args.contains("-sow"), "{without_dtw_args}");

        sweep_temp_word_timing_files();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_segment_words_maps_srt_cues_to_word_timings_offset_by_cue_start() {
        // Given: a fake whisper-cli producing a fixed two-word .srt, and
        //        a sentence starting 10s into the source file
        // When:  segmenting words for that sentence
        // Then:  each WordTiming's word/start/end come from the .srt,
        //        with start/end shifted by the sentence's own start (10s)
        let _guard = TEMP_WORD_TIMING_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = test_dir("maps-cues-to-word-timings");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);
        let binary_path = write_fake_binary(&dir, "fake-whisper-cli.sh", FAKE_WHISPER_CLI_SCRIPT);
        let segmenter = WhisperCliWordSegmenter {
            binary_path,
            ffmpeg_path,
            dtw_preset: Some("large.v3".to_string()),
            ..WhisperCliWordSegmenter::default()
        };

        let words = segmenter
            .segment_words(
                &source_path,
                Duration::from_secs(10),
                Duration::from_secs(12),
            )
            .unwrap();

        assert_eq!(words.len(), 2);
        assert_eq!(words[0].word, "hello");
        assert_eq!(words[0].start, Duration::from_millis(10_000));
        assert_eq!(words[0].end, Duration::from_millis(10_500));
        assert_eq!(words[1].word, "world");
        assert_eq!(words[1].start, Duration::from_millis(10_600));
        assert_eq!(words[1].end, Duration::from_millis(11_200));

        sweep_temp_word_timing_files();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A fake `whisper-cli` that writes a `.srt` with a leading
    /// zero-duration cue (mirrors a real `whisper-cli`/DTW quirk observed
    /// in manual testing: a word right at a clip's edge occasionally gets
    /// `start == end`) followed by one normal cue, to `"<-of value>.srt"`.
    #[cfg(unix)]
    const FAKE_WHISPER_CLI_SCRIPT_WITH_DEGENERATE_CUE: &str = r#"#!/bin/sh
of=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-of" ]; then
        of="$arg"
    fi
    prev="$arg"
done
printf '1\n00:00:00,000 --> 00:00:00,000\nghost\n\n2\n00:00:00,600 --> 00:00:01,200\nworld\n' > "${of}.srt"
"#;

    /// A fake `whisper-cli` that writes a `.srt` with one cue whose text
    /// is empty (mirrors a real quirk observed with VAD enabled: a
    /// detected speech blip with a valid, non-zero duration but nothing
    /// actually transcribed) followed by one normal cue.
    #[cfg(unix)]
    const FAKE_WHISPER_CLI_SCRIPT_WITH_EMPTY_TEXT_CUE: &str = r#"#!/bin/sh
of=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-of" ]; then
        of="$arg"
    fi
    prev="$arg"
done
printf '1\n00:00:00,690 --> 00:00:00,790\n\n\n2\n00:00:00,600 --> 00:00:01,200\nworld\n' > "${of}.srt"
"#;

    #[test]
    #[cfg(unix)]
    fn test_segment_words_drops_empty_text_words_instead_of_returning_blank_rows() {
        // Given: a fake whisper-cli whose .srt has one cue with valid
        //        timing but empty text ahead of a normal one
        // When:  segmenting words
        // Then:  it succeeds, returning only the normal word — the
        //        empty-text one is dropped rather than becoming a blank
        //        row in the popup
        let _guard = TEMP_WORD_TIMING_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = test_dir("drops-empty-text-words");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);
        let binary_path = write_fake_binary(
            &dir,
            "fake-whisper-cli.sh",
            FAKE_WHISPER_CLI_SCRIPT_WITH_EMPTY_TEXT_CUE,
        );
        let segmenter = WhisperCliWordSegmenter {
            binary_path,
            ffmpeg_path,
            ..WhisperCliWordSegmenter::default()
        };

        let words = segmenter
            .segment_words(&source_path, Duration::from_secs(0), Duration::from_secs(2))
            .unwrap();

        assert_eq!(words.len(), 1);
        assert_eq!(words[0].word, "world");

        sweep_temp_word_timing_files();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_segment_words_drops_zero_duration_words_instead_of_failing() {
        // Given: a fake whisper-cli whose .srt has one zero-duration cue
        //        (start == end) ahead of a normal one
        // When:  segmenting words
        // Then:  it succeeds, returning only the normal word — the
        //        degenerate one is dropped rather than the whole call
        //        failing with SubtitleError::InvalidTiming
        let _guard = TEMP_WORD_TIMING_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = test_dir("drops-zero-duration-words");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);
        let binary_path = write_fake_binary(
            &dir,
            "fake-whisper-cli.sh",
            FAKE_WHISPER_CLI_SCRIPT_WITH_DEGENERATE_CUE,
        );
        let segmenter = WhisperCliWordSegmenter {
            binary_path,
            ffmpeg_path,
            ..WhisperCliWordSegmenter::default()
        };

        let words = segmenter
            .segment_words(&source_path, Duration::from_secs(0), Duration::from_secs(2))
            .unwrap();

        assert_eq!(words.len(), 1);
        assert_eq!(words[0].word, "world");

        sweep_temp_word_timing_files();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Serializes every test below that calls `segment_words` (and
    /// therefore `temp_clip_audio_path`, which writes into the shared,
    /// process-wide system temp dir) against every other one — the two
    /// cleanup tests scan that same directory for *any*
    /// `trango-word-timing-<this pid>-*` file, which would otherwise
    /// race against another concurrently-running test's own in-flight
    /// temp clip/srt (mirrors `generate.rs`'s `TEMP_SEGMENT_TEST_LOCK`,
    /// extended here to all `segment_words` callers rather than just the
    /// two cleanup tests, since more tests here share that one temp-dir
    /// naming scheme).
    static TEMP_WORD_TIMING_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    #[cfg(unix)]
    fn test_segment_words_cleans_up_temp_clip_and_srt_on_success() {
        // Given: a fake ffmpeg/whisper-cli pair that succeed
        // When:  segmenting words
        // Then:  no trango-word-timing-* file is left in the system temp
        //        dir afterward
        let _guard = TEMP_WORD_TIMING_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = test_dir("cleans-up-on-success");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);
        let binary_path = write_fake_binary(&dir, "fake-whisper-cli.sh", FAKE_WHISPER_CLI_SCRIPT);
        let segmenter = WhisperCliWordSegmenter {
            binary_path,
            ffmpeg_path,
            ..WhisperCliWordSegmenter::default()
        };

        segmenter
            .segment_words(&source_path, Duration::from_secs(0), Duration::from_secs(1))
            .unwrap();

        assert!(temp_word_timing_files().is_empty());
        sweep_temp_word_timing_files();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_segment_words_cleans_up_temp_clip_and_srt_when_whisper_cli_fails() {
        // Given: a fake ffmpeg that succeeds and a fake whisper-cli that
        //        exits non-zero
        // When:  segmenting words
        // Then:  GenerationFailed is returned and no trango-word-timing-*
        //        file is left behind
        let _guard = TEMP_WORD_TIMING_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let dir = test_dir("cleans-up-on-failure");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);
        let binary_path = write_fake_binary(
            &dir,
            "fake-whisper-cli.sh",
            "#!/bin/sh\necho 'boom' >&2\nexit 1\n",
        );
        let segmenter = WhisperCliWordSegmenter {
            binary_path,
            ffmpeg_path,
            ..WhisperCliWordSegmenter::default()
        };

        let result =
            segmenter.segment_words(&source_path, Duration::from_secs(0), Duration::from_secs(1));

        assert!(matches!(result, Err(SubtitleError::GenerationFailed(_))));
        assert!(temp_word_timing_files().is_empty());
        sweep_temp_word_timing_files();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Filenames in the system temp dir matching `temp_clip_audio_path`'s
    /// `.wav`/`.srt` naming scheme for *this process* — used to check
    /// `segment_words` doesn't leave either behind. Excludes the fake
    /// `ffmpeg`'s own `<clip>.wav.args` log (a test-only side effect, not
    /// something real `ffmpeg` produces), mirroring `generate.rs`'s
    /// `temp_segment_files` helper.
    fn temp_word_timing_files() -> Vec<PathBuf> {
        let pid_prefix = format!("trango-word-timing-{}-", std::process::id());
        std::fs::read_dir(std::env::temp_dir())
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                let matches_prefix = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with(&pid_prefix));
                let matches_ext = path
                    .extension()
                    .is_some_and(|ext| ext == "wav" || ext == "srt");
                matches_prefix && matches_ext
            })
            .collect()
    }

    /// Removes every leftover file in the system temp dir matching
    /// `temp_clip_audio_path`'s naming scheme for *this process*,
    /// including the fake `ffmpeg`'s test-only `.args` log —
    /// `temp_word_timing_files` deliberately ignores that file for the
    /// cleanup *assertion*, but it should still not be left behind after
    /// the test itself finishes.
    fn sweep_temp_word_timing_files() {
        let pid_prefix = format!("trango-word-timing-{}-", std::process::id());
        for entry in std::fs::read_dir(std::env::temp_dir())
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&pid_prefix))
            {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}
