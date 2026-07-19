//! Wires `TODO.md` Vaihe 32's Ctrl+W word-timing popup to the UI: derives
//! per-word audio timing for the sentence currently shown in the
//! current-sentence card via `subtitle::WhisperCliWordSegmenter::segment_words`
//! and shows the result as a list, each row playable individually.
//!
//! Runs on a background thread, like `subtitle_generation::spawn_generate`
//! and `word_analysis::spawn_analyze_sentence` — a real `whisper-cli`/
//! `ffmpeg` run can take real time and would freeze the UI thread if run
//! directly on it.

use std::path::PathBuf;
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use slint::VecModel;
use subtitle::{SubtitleError, WhisperCliWordSegmenter, WordTiming};

use crate::{AppWindow, WordTimingRow, WordTimingStatus};

/// Runs `segmenter.segment_words(&source_path, cue_start, cue_end)` on a
/// background thread, calling `on_done` with its result once finished.
/// Returns immediately without blocking the calling thread — mirrors
/// `subtitle_generation::spawn_generate`'s shape exactly.
pub fn spawn_segment_words(
    segmenter: WhisperCliWordSegmenter,
    source_path: PathBuf,
    cue_start: Duration,
    cue_end: Duration,
    on_done: impl FnOnce(Result<Vec<WordTiming>, SubtitleError>) + Send + 'static,
) {
    thread::spawn(move || {
        on_done(segmenter.segment_words(&source_path, cue_start, cue_end));
    });
}

/// Opens the Ctrl+W popup in a loading state — used right after
/// `spawn_segment_words` has been kicked off for the current sentence.
pub fn open_popup_loading(window: &AppWindow) {
    window.set_word_timing_status(WordTimingStatus::Loading);
    window.set_word_timing_rows(Rc::new(VecModel::from(Vec::<WordTimingRow>::new())).into());
    window.set_word_timing_error_message("".into());
    window.set_is_word_timing_popup_open(true);
}

/// Applies a finished `spawn_segment_words` call's `result` to the
/// already-open (loading) popup. Must be called on the UI thread.
pub fn apply_result(window: &AppWindow, result: Result<Vec<WordTiming>, SubtitleError>) {
    match result {
        Ok(words) => {
            window.set_word_timing_status(WordTimingStatus::Done);
            window.set_word_timing_rows(Rc::new(VecModel::from(word_timing_rows(&words))).into());
            window.set_word_timing_error_message("".into());
        }
        Err(err) => {
            tracing::warn!(%err, "word timing segmentation failed");
            window.set_word_timing_status(WordTimingStatus::Error);
            window.set_word_timing_error_message(err.to_string().into());
        }
    }
}

/// Maps `segment_words`' result into the popup's Slint row model,
/// formatting each word's `start`/`end` via [`format_timestamp`].
fn word_timing_rows(words: &[WordTiming]) -> Vec<WordTimingRow> {
    words
        .iter()
        .map(|word| WordTimingRow {
            word: word.word.clone().into(),
            start_label: format_timestamp(word.start).into(),
            end_label: format_timestamp(word.end).into(),
            start_seconds: word.start.as_secs_f32(),
            end_seconds: word.end.as_secs_f32(),
        })
        .collect()
}

/// Formats `duration` as `"M:SS.mmm"` (e.g. `Duration::from_millis(83_456)`
/// → `"1:23.456"`) for display in the popup — minutes are not
/// zero-padded, seconds and milliseconds always are, matching how a
/// stopwatch reads at a glance.
fn format_timestamp(duration: Duration) -> String {
    let total_millis = duration.as_millis();
    let minutes = total_millis / 60_000;
    let seconds = (total_millis / 1_000) % 60;
    let millis = total_millis % 1_000;
    format!("{minutes}:{seconds:02}.{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_timestamp_formats_minutes_seconds_millis() {
        // Given/When/Then: a handful of boundary values format as
        //                   "M:SS.mmm", seconds/millis zero-padded but
        //                   minutes not
        assert_eq!(format_timestamp(Duration::ZERO), "0:00.000");
        assert_eq!(format_timestamp(Duration::from_millis(500)), "0:00.500");
        assert_eq!(format_timestamp(Duration::from_millis(59_999)), "0:59.999");
        assert_eq!(format_timestamp(Duration::from_millis(60_000)), "1:00.000");
        assert_eq!(format_timestamp(Duration::from_millis(83_456)), "1:23.456");
        assert_eq!(
            format_timestamp(Duration::from_millis(600_789)),
            "10:00.789"
        );
    }

    #[test]
    fn test_word_timing_rows_maps_words_and_formats_timestamps() {
        // Given: two WordTimings
        // When:  mapping them into Slint rows
        // Then:  word/label/seconds all come from the source WordTiming,
        //        with start/end labels formatted via format_timestamp
        let words = vec![
            WordTiming {
                word: "hello".to_string(),
                start: Duration::from_millis(1_000),
                end: Duration::from_millis(1_500),
            },
            WordTiming {
                word: "world".to_string(),
                start: Duration::from_millis(1_600),
                end: Duration::from_millis(2_200),
            },
        ];

        let rows = word_timing_rows(&words);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].word, "hello");
        assert_eq!(rows[0].start_label, "0:01.000");
        assert_eq!(rows[0].end_label, "0:01.500");
        assert_eq!(rows[0].start_seconds, 1.0);
        assert_eq!(rows[0].end_seconds, 1.5);
        assert_eq!(rows[1].word, "world");
        assert_eq!(rows[1].start_label, "0:01.600");
        assert_eq!(rows[1].end_label, "0:02.200");
    }
}
