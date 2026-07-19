//! Assembles one sentence's full practice-audio piece sequence and
//! concatenates it into a single `.mp3` (`TODO.md` Vaihe 34).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::PracticeAudioError;
use crate::pieces::{apply_speed, concat_to_mp3, extract_clip, generate_silence, wav_duration};
use crate::tts::EspeakTtsSynthesizer;

/// One word's translation and its real audio span within the source
/// file — generic (no `subtitle`/`word-analysis` types), built by the
/// `app` crate from `subtitle::WordTiming` (audio span) positionally
/// paired with `word_analysis::WordEntry::translation`.
pub struct WordPracticeSpec {
    /// The word's translation, spoken via TTS before its audio repeats.
    pub translation: String,
    /// Start of this word's real audio within the source file.
    pub start: Duration,
    /// End of this word's real audio within the source file.
    pub end: Duration,
}

/// The three speeds each word's audio is repeated at, twice each, in
/// order — `1.0` is handled specially by
/// [`build_sentence_practice_audio`] (skips `apply_speed` entirely and
/// reuses the freshly-extracted clip, since it needs no tempo change).
const WORD_SPEEDS: [f64; 3] = [0.5, 0.75, 1.0];

/// How many times each word's audio is repeated per speed.
const WORD_REPEATS_PER_SPEED: usize = 2;

/// How many times the whole sentence's audio is repeated at the end.
const SENTENCE_REPEATS: usize = 3;

/// How much extra time [`pause_after`] adds on top of a piece's own
/// duration — long enough to attempt repeating a short word out loud,
/// scaling naturally for longer pieces since it's additive, not fixed.
const PAUSE_MARGIN: Duration = Duration::from_secs(1);

/// Builds one sentence's practice audio and `ffmpeg`/`espeak-ng`
/// binaries to run it with.
pub struct PracticeAudioBuilder {
    /// Path or bare name of the `ffmpeg` binary. [`Default::default`]
    /// uses `"ffmpeg"`, resolved via `PATH`.
    pub ffmpeg_path: PathBuf,
    /// The TTS synthesizer used for each word's translation.
    pub tts: EspeakTtsSynthesizer,
}

impl Default for PracticeAudioBuilder {
    fn default() -> Self {
        Self {
            ffmpeg_path: PathBuf::from("ffmpeg"),
            tts: EspeakTtsSynthesizer::default(),
        }
    }
}

impl PracticeAudioBuilder {
    /// Assembles the full practice-audio sequence for one sentence and
    /// writes it to `output_path` as a `.mp3`:
    ///
    /// for each word in `words`, in order — TTS translation (+ pause),
    /// then that word's real audio at 50%/75%/100% speed, twice each
    /// (+ pause after every repeat) — followed by the whole sentence's
    /// real audio (`[request.sentence_start, request.sentence_end)` of
    /// `request.source_path`) three times (+ pause after each). Every
    /// pause is that piece's own duration plus [`PAUSE_MARGIN`].
    ///
    /// All intermediate piece files are written to a temporary
    /// process-unique directory, deleted on success or failure.
    pub fn build_sentence_practice_audio(
        &self,
        request: &SentencePracticeAudioRequest<'_>,
        output_path: &Path,
    ) -> Result<(), PracticeAudioError> {
        let work_dir = temp_work_dir();
        std::fs::create_dir_all(&work_dir)?;

        let result = self.assemble(&work_dir, request, output_path);

        let _ = std::fs::remove_dir_all(&work_dir);
        result
    }

    /// Does the actual piece-by-piece assembly — split out from
    /// [`Self::build_sentence_practice_audio`] purely so that function
    /// can guarantee `work_dir` cleanup via one `remove_dir_all` call
    /// regardless of where this returns early.
    fn assemble(
        &self,
        work_dir: &Path,
        request: &SentencePracticeAudioRequest<'_>,
        output_path: &Path,
    ) -> Result<(), PracticeAudioError> {
        let mut pieces: Vec<PathBuf> = Vec::new();
        let mut counter: u32 = 0;

        for word in request.words {
            let tts_path = next_piece_path(work_dir, &mut counter, "wav");
            self.tts
                .synthesize(&word.translation, request.voice, &tts_path)?;
            let tts_duration = wav_duration(&tts_path)?;
            push_piece_and_pause(
                &self.ffmpeg_path,
                work_dir,
                &mut counter,
                &mut pieces,
                tts_path,
                tts_duration,
            )?;

            let clip_path = next_piece_path(work_dir, &mut counter, "wav");
            extract_clip(
                &self.ffmpeg_path,
                request.source_path,
                word.start,
                word.end,
                &clip_path,
            )?;
            let clip_duration = word.end.saturating_sub(word.start);

            for speed in WORD_SPEEDS {
                let (variant_path, variant_duration) = if speed == 1.0 {
                    (clip_path.clone(), clip_duration)
                } else {
                    let variant_path = next_piece_path(work_dir, &mut counter, "wav");
                    apply_speed(&self.ffmpeg_path, &clip_path, speed, &variant_path)?;
                    (variant_path, scaled_duration(clip_duration, speed))
                };
                for _ in 0..WORD_REPEATS_PER_SPEED {
                    push_piece_and_pause(
                        &self.ffmpeg_path,
                        work_dir,
                        &mut counter,
                        &mut pieces,
                        variant_path.clone(),
                        variant_duration,
                    )?;
                }
            }
        }

        let sentence_clip_path = next_piece_path(work_dir, &mut counter, "wav");
        extract_clip(
            &self.ffmpeg_path,
            request.source_path,
            request.sentence_start,
            request.sentence_end,
            &sentence_clip_path,
        )?;
        let sentence_duration = request.sentence_end.saturating_sub(request.sentence_start);
        for _ in 0..SENTENCE_REPEATS {
            push_piece_and_pause(
                &self.ffmpeg_path,
                work_dir,
                &mut counter,
                &mut pieces,
                sentence_clip_path.clone(),
                sentence_duration,
            )?;
        }

        concat_to_mp3(&self.ffmpeg_path, &pieces, output_path)
    }
}

/// The inputs [`PracticeAudioBuilder::build_sentence_practice_audio`]
/// needs for one sentence — grouped into one struct purely to keep that
/// function's (and its private `assemble` helper's) argument count
/// reasonable.
pub struct SentencePracticeAudioRequest<'a> {
    /// The video/audio file `words`' and the sentence's own timing are
    /// both relative to.
    pub source_path: &'a Path,
    /// The sentence's words, in order.
    pub words: &'a [WordPracticeSpec],
    /// Start of the whole sentence's real audio within `source_path`.
    pub sentence_start: Duration,
    /// End of the whole sentence's real audio within `source_path`.
    pub sentence_end: Duration,
    /// The `espeak-ng` voice code each word's translation is spoken in.
    pub voice: &'a str,
}

/// Pushes `piece_path` onto `pieces`, then generates and pushes a
/// trailing silence piece sized to `piece_duration` + [`PAUSE_MARGIN`].
fn push_piece_and_pause(
    ffmpeg_path: &Path,
    work_dir: &Path,
    counter: &mut u32,
    pieces: &mut Vec<PathBuf>,
    piece_path: PathBuf,
    piece_duration: Duration,
) -> Result<(), PracticeAudioError> {
    pieces.push(piece_path);
    let silence_path = next_piece_path(work_dir, counter, "wav");
    generate_silence(ffmpeg_path, piece_duration + PAUSE_MARGIN, &silence_path)?;
    pieces.push(silence_path);
    Ok(())
}

/// `original`'s duration after an `atempo=<speed>` change — e.g. at
/// 0.5× tempo, playback takes twice as long.
fn scaled_duration(original: Duration, speed: f64) -> Duration {
    Duration::from_secs_f64(original.as_secs_f64() / speed)
}

/// The next process-unique piece path in `work_dir`, e.g.
/// `work_dir/piece-3.wav`.
fn next_piece_path(work_dir: &Path, counter: &mut u32, extension: &str) -> PathBuf {
    *counter += 1;
    work_dir.join(format!("piece-{counter}.{extension}"))
}

/// A process-unique temporary directory for one sentence's practice-audio
/// pieces, e.g. `/tmp/trango-practice-audio-<pid>-<counter>/`.
fn temp_work_dir() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "trango-practice-audio-{}-{counter}",
        std::process::id()
    ))
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
        let dir = std::env::temp_dir().join(format!("trango-test-sentence-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    /// A fake `ffmpeg` that handles every subcommand this crate issues
    /// (extract/speed/silence/concat) by writing a fixed-size WAV (1
    /// second's worth of the crate's 22050 Hz mono 16-bit format) to
    /// whatever its last argument is, or — for the final `concat_to_mp3`
    /// call, recognized by its `.mp3` output extension — a fake mp3 and
    /// a copy of the concat list file for inspection.
    #[cfg(unix)]
    const FAKE_FFMPEG_SCRIPT: &str = r#"#!/bin/sh
last=""
list=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-i" ] && [ "$list" = "" ]; then
        case "$arg" in
            *.txt) list="$arg" ;;
        esac
    fi
    last="$arg"
    prev="$arg"
done
case "$last" in
    *.mp3)
        if [ -n "$list" ]; then cp "$list" "${last}.list"; fi
        printf 'fake mp3 content' > "$last"
        ;;
    *)
        # 44-byte header + 1 second of 22050 Hz mono 16-bit silence
        head -c 44 /dev/zero > "$last"
        head -c 44100 /dev/zero >> "$last"
        ;;
esac
"#;

    /// A fake `espeak-ng` that writes a fixed-size WAV (0.5s worth) to
    /// its `-w` argument.
    #[cfg(unix)]
    const FAKE_ESPEAK_SCRIPT: &str = r#"#!/bin/sh
out=""
prev=""
for arg in "$@"; do
    if [ "$prev" = "-w" ]; then out="$arg"; fi
    prev="$arg"
done
head -c 44 /dev/zero > "$out"
head -c 22050 /dev/zero >> "$out"
"#;

    #[test]
    #[cfg(unix)]
    fn test_build_sentence_practice_audio_produces_expected_piece_count_and_cleans_up() {
        // Given: a sentence with two words
        // When:  building its practice audio
        // Then:  it succeeds, the final .mp3 exists, the concat list has
        //        the expected number of pieces (per word: TTS+pause,
        //        3 speeds x 2 repeats x (piece+pause) = 1 + 1 + 12 = 14
        //        entries per word; plus the sentence: 3 repeats x
        //        (piece+pause) = 6 entries — for 2 words: 2*14 + 6 = 34),
        //        and no temp work dir is left behind
        let dir = test_dir("piece-count");
        let source_path = dir.join("source.mp4");
        std::fs::write(&source_path, b"").unwrap();
        let ffmpeg_path = write_fake_binary(&dir, "fake-ffmpeg.sh", FAKE_FFMPEG_SCRIPT);
        let espeak_path = write_fake_binary(&dir, "fake-espeak-ng.sh", FAKE_ESPEAK_SCRIPT);
        let builder = crate::PracticeAudioBuilder {
            ffmpeg_path: ffmpeg_path.clone(),
            tts: crate::EspeakTtsSynthesizer {
                binary_path: espeak_path,
            },
        };
        let words = vec![
            WordPracticeSpec {
                translation: "hello".to_string(),
                start: Duration::from_millis(0),
                end: Duration::from_millis(500),
            },
            WordPracticeSpec {
                translation: "world".to_string(),
                start: Duration::from_millis(600),
                end: Duration::from_millis(1200),
            },
        ];
        let output_path = dir.join("0001.mp3");

        builder
            .build_sentence_practice_audio(
                &SentencePracticeAudioRequest {
                    source_path: &source_path,
                    words: &words,
                    sentence_start: Duration::from_millis(0),
                    sentence_end: Duration::from_millis(1200),
                    voice: "en",
                },
                &output_path,
            )
            .unwrap();

        assert!(output_path.is_file());
        let list_contents = std::fs::read_to_string(format!("{}.list", output_path.display()))
            .expect("fake ffmpeg should have copied the concat list");
        let piece_count = list_contents.lines().filter(|l| !l.is_empty()).count();
        assert_eq!(piece_count, 34, "{list_contents}");

        let leftover: Vec<_> = std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(&format!("trango-practice-audio-{}-", std::process::id()))
            })
            .collect();
        assert!(leftover.is_empty(), "{leftover:?}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn test_scaled_duration_halves_speed_doubles_duration() {
        // Given/When/Then: atempo=0.5 doubles duration, atempo=1.0
        //                   leaves it unchanged
        assert_eq!(
            scaled_duration(Duration::from_secs(2), 0.5),
            Duration::from_secs(4)
        );
        assert_eq!(
            scaled_duration(Duration::from_secs(2), 1.0),
            Duration::from_secs(2)
        );
    }
}
