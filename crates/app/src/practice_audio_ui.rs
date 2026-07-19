//! Wires `TODO.md` Vaihe 34's "Generate practice audio" batch loop to the
//! UI: walks every cue in the loaded subtitle, building one pronunciation-
//! practice `.mp3` per sentence (TTS translation + real audio at
//! 50%/75%/100% speed per word, then the whole sentence three times).
//!
//! Runs on a background thread — like `word_analysis::spawn_batch_analyze`,
//! a real `whisper-cli`/`espeak-ng`/`ffmpeg` run per cue can take real
//! time and would freeze the UI thread if run directly on it. Unlike
//! `spawn_batch_analyze`, this does **not** call Ollama itself — it only
//! reads already-cached `word_analysis::WordAnalysis` translations (see
//! `docs/src/developer/specs.md`), skipping any cue with no cached (or
//! empty) analysis rather than triggering a fresh Ollama call as a side
//! effect of a differently-named button.

use std::path::PathBuf;
use std::thread;

use subtitle::{Cue, WhisperCliWordSegmenter};

use crate::{AppWindow, PracticeAudioBatchStatus};

/// Runs `subtitle::WhisperCliWordSegmenter::segment_words` +
/// `practice_audio::PracticeAudioBuilder::build_sentence_practice_audio`
/// for every cue in `cues`, in order, on a background thread. Returns
/// immediately without blocking the calling thread.
///
/// For each cue: looks up `cache_path`'s cached `WordAnalysis` for
/// `cue.index` (skips, logging a `tracing::warn!`, if missing or empty —
/// this function never calls Ollama itself); re-segments that cue's own
/// audio span for word timing; positionally pairs the two word lists up
/// to `min(len)` (logging a `tracing::warn!` if they differ — see this
/// module's doc comment); builds `<output_dir>/<NNNN>.mp3` (`position`,
/// 1-based, zero-padded to `cues.len()`'s digit count). A per-cue
/// failure is logged and recorded, but does not stop the run — the same
/// fail-soft shape `word_analysis::spawn_batch_analyze` already uses.
/// Calls `on_progress(done, total)` after every cue (succeeded, skipped,
/// or failed) and `on_done(result)` once at the end, `Err` only if at
/// least one cue failed or was skipped.
#[allow(clippy::too_many_arguments)]
pub fn spawn_batch_generate(
    segmenter: WhisperCliWordSegmenter,
    builder: practice_audio::PracticeAudioBuilder,
    tts_voice: String,
    media_path: PathBuf,
    cues: Vec<Cue>,
    cache_path: PathBuf,
    output_dir: PathBuf,
    on_progress: impl Fn(usize, usize) + Send + 'static,
    on_done: impl FnOnce(Result<(), String>) + Send + 'static,
) {
    thread::spawn(move || {
        let total = cues.len();
        let cache = ::word_analysis::load_cache(&cache_path);
        let width = digit_count(total);
        let mut last_error: Option<String> = None;

        for (index, cue) in cues.iter().enumerate() {
            let done = index + 1;
            if let Err(err) = generate_one(
                &segmenter,
                &builder,
                &tts_voice,
                &media_path,
                cue,
                &cache,
                &output_dir,
                done,
                width,
            ) {
                tracing::warn!(cue_index = cue.index, %err, "skipping cue in practice-audio batch");
                last_error = Some(err);
            }
            on_progress(done, total);
        }

        on_done(last_error.map_or(Ok(()), Err));
    });
}

/// Builds one cue's practice-audio `.mp3`, or returns an error explaining
/// why it was skipped (no cached analysis, `segment_words` failure, or
/// `build_sentence_practice_audio` failure) — the caller logs this and
/// moves on to the next cue rather than aborting the whole run.
#[allow(clippy::too_many_arguments)]
fn generate_one(
    segmenter: &WhisperCliWordSegmenter,
    builder: &practice_audio::PracticeAudioBuilder,
    tts_voice: &str,
    media_path: &std::path::Path,
    cue: &Cue,
    cache: &::word_analysis::AnalysisCache,
    output_dir: &std::path::Path,
    position: usize,
    width: usize,
) -> Result<(), String> {
    let analysis = cache
        .entries
        .get(&cue.index)
        .filter(|analysis| !analysis.words.is_empty())
        .ok_or_else(|| {
            "no cached word analysis for this sentence — run \"Analyze all sentences\" first"
                .to_string()
        })?;

    let timings = segmenter
        .segment_words(media_path, cue.start, cue.end)
        .map_err(|err| format!("word-timing segmentation failed: {err}"))?;

    if timings.len() != analysis.words.len() {
        tracing::warn!(
            cue_index = cue.index,
            timed_words = timings.len(),
            analyzed_words = analysis.words.len(),
            "word count mismatch between segment_words and cached analysis for this cue; \
             pairing positionally up to the shorter list"
        );
    }

    let words: Vec<practice_audio::WordPracticeSpec> = timings
        .into_iter()
        .zip(analysis.words.iter())
        .map(|(timing, entry)| practice_audio::WordPracticeSpec {
            translation: entry.translation.clone(),
            start: timing.start,
            end: timing.end,
        })
        .collect();

    if words.is_empty() {
        return Err("no words to build practice audio from for this sentence".to_string());
    }

    let output_path = output_dir.join(format!("{position:0width$}.mp3"));
    builder
        .build_sentence_practice_audio(
            &practice_audio::SentencePracticeAudioRequest {
                source_path: media_path,
                words: &words,
                sentence_start: cue.start,
                sentence_end: cue.end,
                voice: tts_voice,
            },
            &output_path,
        )
        .map_err(|err| format!("failed to build practice audio: {err}"))
}

/// How many digits `total` needs (e.g. `66` needs `2`, `100` needs `3`) —
/// used to zero-pad output filenames consistently regardless of the
/// subtitle's cue count. Always at least `4` (`"0001.mp3"`, not
/// `"1.mp3"`) for a consistent look on typically-sized subtitles.
fn digit_count(total: usize) -> usize {
    total.to_string().len().max(4)
}

/// Opens the practice-audio batch in a running state — used right after
/// `spawn_batch_generate` has just been kicked off.
pub fn open_batch_running(window: &AppWindow, total: usize) {
    window.set_practice_audio_batch_status(PracticeAudioBatchStatus::Running);
    window.set_practice_audio_batch_progress_current(0);
    window.set_practice_audio_batch_progress_total(total as i32);
    window.set_practice_audio_batch_error_message("".into());
}

/// Mirrors a running `spawn_batch_generate` call's progress into
/// `window`'s `practice-audio-batch-progress-*` properties. Must be
/// called on the UI thread.
pub fn apply_batch_progress(window: &AppWindow, done: usize, total: usize) {
    window.set_practice_audio_batch_status(PracticeAudioBatchStatus::Running);
    window.set_practice_audio_batch_progress_current(done as i32);
    window.set_practice_audio_batch_progress_total(total as i32);
}

/// Mirrors a finished `spawn_batch_generate` run's `result` into
/// `window`'s `practice-audio-batch-status`/`-error-message` properties.
/// Must be called on the UI thread.
pub fn apply_batch_result(window: &AppWindow, result: Result<(), String>) {
    match result {
        Ok(()) => {
            tracing::info!("practice-audio batch run finished");
            window.set_practice_audio_batch_status(PracticeAudioBatchStatus::Done);
            window.set_practice_audio_batch_error_message("".into());
        }
        Err(message) => {
            tracing::warn!(%message, "practice-audio batch run finished with errors");
            window.set_practice_audio_batch_status(PracticeAudioBatchStatus::Error);
            window.set_practice_audio_batch_error_message(message.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digit_count_is_at_least_four_and_grows_for_large_totals() {
        // Given/When/Then: small totals still pad to 4 digits, larger
        //                   ones grow to fit
        assert_eq!(digit_count(5), 4);
        assert_eq!(digit_count(66), 4);
        assert_eq!(digit_count(12_345), 5);
    }
}
