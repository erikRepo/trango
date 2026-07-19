//! Individual audio-piece generation via the external `ffmpeg` binary:
//! cutting a clip out of the source file, adjusting its speed, and
//! generating silence — all normalized to one shared format (22050 Hz
//! mono 16-bit PCM WAV, matching `espeak-ng`'s own native output rate)
//! so [`concat_to_mp3`] never needs to resample anything.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::error::PracticeAudioError;
use crate::process::{last_stderr_line, run_command};

/// Sample rate every piece this crate produces uses — matches
/// `espeak-ng`'s own default WAV output rate exactly, so TTS pieces and
/// `ffmpeg`-extracted clips never need resampling before concatenation.
pub(crate) const SAMPLE_RATE: u32 = 22050;

/// Every piece is mono.
const CHANNELS: u32 = 1;

/// Every piece is 16-bit PCM.
const BYTES_PER_SAMPLE: u32 = 2;

/// The canonical PCM WAV header size (`RIFF`+size+`WAVE`+`fmt `+16+
/// format+channels+rate+byte_rate+block_align+bits+`data`+size = 44
/// bytes) — both `ffmpeg`'s own default `.wav` muxer and this crate's
/// own [`generate_silence`]/[`extract_clip`] output use exactly this,
/// with no extra chunks, so [`wav_duration`] can compute a duration from
/// plain file size instead of parsing the header.
const WAV_HEADER_BYTES: u64 = 44;

/// Cuts `[start, end)` out of `source_path`'s audio into `out_path` as
/// 22050 Hz mono `pcm_s16le` — `-ss`/`-to` given as *output* options
/// (after `-i`), the same sample-accurate approach
/// `subtitle::WhisperCliWordSegmenter::extract_clip` uses.
pub(crate) fn extract_clip(
    ffmpeg_path: &Path,
    source_path: &Path,
    start: Duration,
    end: Duration,
    out_path: &Path,
) -> Result<(), PracticeAudioError> {
    tracing::debug!(
        ?source_path,
        ?start,
        ?end,
        ?out_path,
        "cutting practice-audio clip"
    );
    run_ffmpeg(
        ffmpeg_path,
        Command::new(ffmpeg_path)
            .arg("-y")
            .arg("-i")
            .arg(source_path)
            .arg("-ss")
            .arg(format_seconds(start))
            .arg("-to")
            .arg(format_seconds(end))
            .arg("-ar")
            .arg(SAMPLE_RATE.to_string())
            .arg("-ac")
            .arg(CHANNELS.to_string())
            .arg("-c:a")
            .arg("pcm_s16le")
            .arg(out_path),
    )
}

/// Applies an `atempo` speed change to `clip_path`, writing the result
/// to `out_path` — `atempo` supports 0.5–100× in a single filter
/// application, covering the 0.5/0.75 speeds this crate needs (1.0×
/// pieces skip this entirely and reuse the original clip — see
/// `sentence::build_sentence_practice_audio`).
pub(crate) fn apply_speed(
    ffmpeg_path: &Path,
    clip_path: &Path,
    speed: f64,
    out_path: &Path,
) -> Result<(), PracticeAudioError> {
    tracing::debug!(
        ?clip_path,
        speed,
        ?out_path,
        "applying practice-audio speed change"
    );
    run_ffmpeg(
        ffmpeg_path,
        Command::new(ffmpeg_path)
            .arg("-y")
            .arg("-i")
            .arg(clip_path)
            .arg("-af")
            .arg(format!("atempo={speed}"))
            .arg("-ar")
            .arg(SAMPLE_RATE.to_string())
            .arg("-ac")
            .arg(CHANNELS.to_string())
            .arg("-c:a")
            .arg("pcm_s16le")
            .arg(out_path),
    )
}

/// Generates `duration` of silence into `out_path`, same 22050 Hz mono
/// format as every other piece.
pub(crate) fn generate_silence(
    ffmpeg_path: &Path,
    duration: Duration,
    out_path: &Path,
) -> Result<(), PracticeAudioError> {
    tracing::debug!(?duration, ?out_path, "generating practice-audio silence");
    run_ffmpeg(
        ffmpeg_path,
        Command::new(ffmpeg_path)
            .arg("-y")
            .arg("-f")
            .arg("lavfi")
            .arg("-i")
            .arg(format!("anullsrc=r={SAMPLE_RATE}:cl=mono"))
            .arg("-t")
            .arg(format_seconds(duration))
            .arg("-c:a")
            .arg("pcm_s16le")
            .arg(out_path),
    )
}

/// Concatenates `pieces` (in order) into one `.mp3` at `out_path`, via
/// `ffmpeg`'s `concat` demuxer (a temp list file, `file '<path>'` per
/// line) — correctly concatenates separate self-contained WAV files
/// before re-encoding, unlike the raw-stream `-c copy` variant of the
/// same demuxer.
pub(crate) fn concat_to_mp3(
    ffmpeg_path: &Path,
    pieces: &[PathBuf],
    out_path: &Path,
) -> Result<(), PracticeAudioError> {
    let list_path = out_path.with_extension("concat.txt");
    let list_contents = pieces
        .iter()
        .map(|piece| {
            format!(
                "file '{}'\n",
                piece.display().to_string().replace('\'', "'\\''")
            )
        })
        .collect::<String>();
    std::fs::write(&list_path, list_contents)?;

    let result = run_ffmpeg(
        ffmpeg_path,
        Command::new(ffmpeg_path)
            .arg("-y")
            .arg("-f")
            .arg("concat")
            .arg("-safe")
            .arg("0")
            .arg("-i")
            .arg(&list_path)
            .arg("-c:a")
            .arg("libmp3lame")
            .arg("-q:a")
            .arg("4")
            .arg(out_path),
    );

    let _ = std::fs::remove_file(&list_path);
    result
}

/// `duration`'s length as `ffmpeg`'s `-ss`/`-to`/`-t` expect: seconds
/// with millisecond precision (e.g. `"1.500"`).
fn format_seconds(duration: Duration) -> String {
    format!("{:.3}", duration.as_secs_f64())
}

/// The duration of a WAV file this crate produced — every such file is
/// the same fixed 22050 Hz mono 16-bit PCM format with a standard
/// 44-byte header (see [`WAV_HEADER_BYTES`]), so duration is plain
/// arithmetic on the file size rather than needing an `ffprobe`
/// subprocess. Used for pieces whose duration isn't already known in
/// advance (TTS output — depends on text length/speech rate).
pub(crate) fn wav_duration(path: &Path) -> Result<Duration, PracticeAudioError> {
    let file_size = std::fs::metadata(path)?.len();
    let data_bytes = file_size.saturating_sub(WAV_HEADER_BYTES);
    let bytes_per_second =
        u64::from(SAMPLE_RATE) * u64::from(CHANNELS) * u64::from(BYTES_PER_SAMPLE);
    Ok(Duration::from_secs_f64(
        data_bytes as f64 / bytes_per_second as f64,
    ))
}

/// Runs an already-built `ffmpeg` command, translating a missing-binary
/// or failed-run outcome into a [`PracticeAudioError::GenerationFailed`]
/// with an actionable message — shared tail end of every function above.
fn run_ffmpeg(ffmpeg_path: &Path, command: &mut Command) -> Result<(), PracticeAudioError> {
    let output = run_command(command).map_err(|err| {
        if err.kind() == io::ErrorKind::NotFound {
            PracticeAudioError::GenerationFailed(format!(
                "ffmpeg not found (looked for \"{}\"). Install ffmpeg and make sure it's on \
                PATH, or set TRANGO_FFMPEG_PATH to its location — see docs/src/usage.",
                ffmpeg_path.display()
            ))
        } else {
            PracticeAudioError::GenerationFailed(format!("failed to run ffmpeg: {err}"))
        }
    })?;

    if !output.status.success() {
        return Err(PracticeAudioError::GenerationFailed(format!(
            "ffmpeg exited with {}: {}",
            output.status,
            last_stderr_line(&output.stderr)
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn write_fake_binary(dir: &Path, name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let script_path = dir.join(name);
        std::fs::write(&script_path, script).expect("failed to write fake binary script");
        std::fs::set_permissions(&script_path, std::fs::Permissions::from_mode(0o755))
            .expect("failed to make fake binary script executable");
        script_path
    }

    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-pieces-{name}"));
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
printf 'fake wav content' > "$last"
"#;

    #[test]
    #[cfg(unix)]
    fn test_extract_clip_runs_ffmpeg_with_expected_flags() {
        let dir = test_dir("extract-clip-flags");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let out_path = dir.join("clip.wav");
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);

        extract_clip(
            &ffmpeg_path,
            &source_path,
            Duration::from_millis(500),
            Duration::from_millis(1250),
            &out_path,
        )
        .unwrap();

        let logged_args = std::fs::read_to_string(format!("{}.args", out_path.display())).unwrap();
        assert!(logged_args.contains(&format!("-i {}", source_path.display())));
        assert!(logged_args.contains("-ss 0.500"));
        assert!(logged_args.contains("-to 1.250"));
        assert!(logged_args.contains("-ar 22050"));
        assert!(logged_args.contains("-ac 1"));
        assert!(logged_args.contains("pcm_s16le"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_apply_speed_runs_ffmpeg_with_atempo_filter() {
        let dir = test_dir("apply-speed-flags");
        let clip_path = dir.join("clip.wav");
        std::fs::write(&clip_path, b"").unwrap();
        let out_path = dir.join("clip-slow.wav");
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);

        apply_speed(&ffmpeg_path, &clip_path, 0.5, &out_path).unwrap();

        let logged_args = std::fs::read_to_string(format!("{}.args", out_path.display())).unwrap();
        assert!(logged_args.contains("atempo=0.5"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_generate_silence_runs_ffmpeg_with_anullsrc_and_duration() {
        let dir = test_dir("generate-silence-flags");
        let out_path = dir.join("silence.wav");
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);

        generate_silence(&ffmpeg_path, Duration::from_millis(1500), &out_path).unwrap();

        let logged_args = std::fs::read_to_string(format!("{}.args", out_path.display())).unwrap();
        assert!(logged_args.contains("anullsrc=r=22050:cl=mono"));
        assert!(logged_args.contains("-t 1.500"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_concat_to_mp3_writes_list_file_with_all_pieces_in_order() {
        let dir = test_dir("concat-flags");
        let piece1 = dir.join("piece1.wav");
        let piece2 = dir.join("piece2.wav");
        std::fs::write(&piece1, b"").unwrap();
        std::fs::write(&piece2, b"").unwrap();
        let out_path = dir.join("final.mp3");
        let binary_path = write_fake_binary(
            &dir,
            "fake-ffmpeg.sh",
            r#"#!/bin/sh
list=""
last=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-i" ]; then list="$arg"; fi
    last="$arg"
    prev="$arg"
done
cp "$list" "${last}.list"
printf 'fake mp3 content' > "$last"
"#,
        );

        concat_to_mp3(&binary_path, &[piece1.clone(), piece2.clone()], &out_path).unwrap();

        assert!(out_path.is_file());
        let list_contents =
            std::fs::read_to_string(format!("{}.list", out_path.display())).unwrap();
        assert_eq!(
            list_contents,
            format!("file '{}'\nfile '{}'\n", piece1.display(), piece2.display())
        );
        // concat's own temp list file is cleaned up after the run
        assert!(!out_path.with_extension("concat.txt").exists());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn test_extract_clip_errors_clearly_when_ffmpeg_is_missing() {
        let dir = test_dir("missing-ffmpeg");
        let result = extract_clip(
            &dir.join("no-such-ffmpeg"),
            &dir.join("source.mp4"),
            Duration::ZERO,
            Duration::from_secs(1),
            &dir.join("out.wav"),
        );

        let Err(PracticeAudioError::GenerationFailed(message)) = result else {
            panic!("expected GenerationFailed, got {result:?}");
        };
        assert!(message.contains("ffmpeg not found"), "{message}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_wav_duration_computes_from_file_size() {
        // Given: a fake WAV file with a 44-byte header followed by
        //        exactly 1 second of 22050 Hz mono 16-bit audio data
        //        (44100 bytes)
        // When:  computing its duration
        // Then:  it's 1 second
        let dir = test_dir("wav-duration");
        let path = dir.join("one-second.wav");
        let data = vec![0u8; 44 + 44_100];
        std::fs::write(&path, &data).unwrap();

        let duration = wav_duration(&path).unwrap();

        assert_eq!(duration, Duration::from_secs(1));

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
