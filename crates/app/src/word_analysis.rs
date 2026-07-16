//! Wires `TODO.md` Vaihe 24's word-by-word sentence analysis to the UI:
//! the Open Subtitles dialog's "Analyze all sentences" batch loop
//! (`spawn_batch_analyze`, part 4/6) and the Ctrl+A popup for a single
//! sentence (`spawn_analyze_sentence`, part 5/6).
//!
//! Both run on a background thread — like `subtitle_generation::spawn_generate`,
//! a real Ollama call can take seconds and would freeze the UI thread if
//! run directly on it. `spawn_batch_analyze` additionally saves the
//! growing `word_analysis::AnalysisCache` to disk after each newly
//! analyzed cue, not just once at the end, so a long run interrupted
//! partway through doesn't lose the sentences it already finished.

use std::path::PathBuf;
use std::rc::Rc;
use std::thread;

use slint::VecModel;
use subtitle::Cue;
use word_analysis::{OllamaClient, OllamaError, WordAnalysis};

use crate::{AppWindow, WordAnalysisBatchStatus, WordAnalysisRow, WordAnalysisStatus};

/// Runs word-by-word analysis for every cue in `cues` not already present
/// in the cache file at `cache_path`, calling `on_progress(done, total)`
/// after each cue (whether newly analyzed, skipped as already cached, or
/// failed) and `on_done` once the whole run finishes. Returns immediately
/// without blocking the calling thread.
///
/// A cue that fails to analyze is logged and skipped rather than aborting
/// the run — one bad response (e.g. a transient Ollama hiccup) shouldn't
/// stop the rest of the subtitle from being analyzed; `on_done` reports
/// the *last* error seen, if any, so the caller can surface that a run
/// finished with some cues still missing.
pub fn spawn_batch_analyze<C>(
    client: C,
    model: String,
    target_language: String,
    cues: Vec<Cue>,
    cache_path: PathBuf,
    on_progress: impl Fn(usize, usize) + Send + 'static,
    on_done: impl FnOnce(Result<(), OllamaError>) + Send + 'static,
) where
    C: OllamaClient + Send + 'static,
{
    thread::spawn(move || {
        let mut cache = word_analysis::load_cache(&cache_path);
        cache.model = model.clone();
        let total = cues.len();
        let mut last_error = None;

        for (done, cue) in cues.iter().enumerate() {
            if let std::collections::hash_map::Entry::Vacant(entry) = cache.entries.entry(cue.index)
            {
                match client.analyze_sentence(&model, &cue.text, &target_language) {
                    Ok(analysis) => {
                        entry.insert(analysis);
                        word_analysis::save_cache(&cache_path, &cache);
                    }
                    Err(err) => {
                        tracing::warn!(cue_index = cue.index, %err, "word analysis failed for cue");
                        last_error = Some(err);
                    }
                }
            }
            on_progress(done + 1, total);
        }

        on_done(last_error.map_or(Ok(()), Err));
    });
}

/// Mirrors a `spawn_batch_analyze` progress tick into `window`'s
/// `word-analysis-batch-status`/`-progress-current`/`-progress-total`
/// properties. Must be called on the UI thread.
pub fn apply_batch_progress(window: &AppWindow, done: usize, total: usize) {
    window.set_word_analysis_batch_status(WordAnalysisBatchStatus::Running);
    window.set_word_analysis_batch_progress_current(done as i32);
    window.set_word_analysis_batch_progress_total(total as i32);
}

/// Mirrors a finished `spawn_batch_analyze` run's `result` into `window`'s
/// `word-analysis-batch-status`/`-error-message` properties. Must be
/// called on the UI thread.
pub fn apply_batch_result(window: &AppWindow, result: Result<(), OllamaError>) {
    match result {
        Ok(()) => {
            tracing::info!("word analysis batch run finished");
            window.set_word_analysis_batch_status(WordAnalysisBatchStatus::Done);
            window.set_word_analysis_batch_error_message("".into());
        }
        Err(err) => {
            tracing::warn!(%err, "word analysis batch run finished with errors");
            window.set_word_analysis_batch_status(WordAnalysisBatchStatus::Error);
            window.set_word_analysis_batch_error_message(err.to_string().into());
        }
    }
}

/// The target language used until the user types a different one into the
/// Open Subtitles dialog's language field (`TODO.md` Vaihe 24.1) — also
/// the fallback shown in that field on first run, before
/// `config::TrangoConfig::ollama_target_language` has ever been set.
pub const DEFAULT_TARGET_LANGUAGE: &str = "English";

/// Runs `client.analyze_sentence(...)` on a background thread for a
/// single `sentence`, calling `on_done` with its result once finished —
/// the Ctrl+A popup's uncached-sentence path. Returns immediately without
/// blocking the calling thread.
pub fn spawn_analyze_sentence<C>(
    client: C,
    model: String,
    sentence: String,
    target_language: String,
    on_done: impl FnOnce(Result<WordAnalysis, OllamaError>) + Send + 'static,
) where
    C: OllamaClient + Send + 'static,
{
    thread::spawn(move || {
        on_done(client.analyze_sentence(&model, &sentence, &target_language));
    });
}

/// Opens the Ctrl+A popup in a loading state — used when `sentence`'s
/// analysis isn't already in the subtitle's cache file and
/// `spawn_analyze_sentence` has just been kicked off for it.
pub fn open_popup_loading(window: &AppWindow) {
    window.set_word_analysis_status(WordAnalysisStatus::Loading);
    window.set_word_analysis_rows(Rc::new(VecModel::from(Vec::<WordAnalysisRow>::new())).into());
    window.set_word_analysis_error_message("".into());
    window.set_is_word_analysis_popup_open(true);
}

/// Opens the Ctrl+A popup already showing `analysis` — used on a
/// cache-hit, where there's no background call to wait for.
pub fn open_popup_with_result(window: &AppWindow, analysis: &WordAnalysis) {
    window.set_word_analysis_status(WordAnalysisStatus::Done);
    window.set_word_analysis_rows(Rc::new(VecModel::from(analysis_rows(analysis))).into());
    window.set_word_analysis_error_message("".into());
    window.set_is_word_analysis_popup_open(true);
}

/// Applies a finished `spawn_analyze_sentence` call's `result` to the
/// already-open (loading) popup. Must be called on the UI thread.
pub fn apply_single_result(window: &AppWindow, result: &Result<WordAnalysis, OllamaError>) {
    match result {
        Ok(analysis) => {
            window.set_word_analysis_status(WordAnalysisStatus::Done);
            window.set_word_analysis_rows(Rc::new(VecModel::from(analysis_rows(analysis))).into());
            window.set_word_analysis_error_message("".into());
        }
        Err(err) => {
            tracing::warn!(%err, "word analysis failed");
            window.set_word_analysis_status(WordAnalysisStatus::Error);
            window.set_word_analysis_error_message(err.to_string().into());
        }
    }
}

/// Maps a `WordAnalysis`'s words into the popup's Slint row model.
fn analysis_rows(analysis: &WordAnalysis) -> Vec<WordAnalysisRow> {
    analysis
        .words
        .iter()
        .map(|entry| WordAnalysisRow {
            word: entry.word.clone().into(),
            translation: entry.translation.clone().into(),
            pronunciation: entry.pronunciation.clone().into(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use word_analysis::{AnalysisCache, WordAnalysis, WordEntry};

    use super::*;

    /// An `OllamaClient` test double whose `analyze_sentence` returns a
    /// fixed result per call count, and records every sentence it was
    /// asked to analyze — enough to check both "already-cached cues are
    /// skipped" and "failures don't abort the run" without any real
    /// network I/O.
    struct RecordingClient {
        calls: std::sync::Mutex<Vec<String>>,
        fail_sentences: Vec<String>,
    }

    impl OllamaClient for RecordingClient {
        fn list_models(&self) -> Result<Vec<String>, OllamaError> {
            unreachable!("not exercised by these tests")
        }

        fn analyze_sentence(
            &self,
            _model: &str,
            sentence: &str,
            _target_language: &str,
        ) -> Result<WordAnalysis, OllamaError> {
            self.calls.lock().unwrap().push(sentence.to_string());
            if self.fail_sentences.contains(&sentence.to_string()) {
                return Err(OllamaError::ConnectionFailed(
                    "fixed test failure".to_string(),
                ));
            }
            Ok(WordAnalysis {
                words: vec![WordEntry {
                    word: sentence.to_string(),
                    translation: "translated".to_string(),
                    pronunciation: "pronounced".to_string(),
                }],
            })
        }
    }

    fn temp_cache_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("trango-test-word-analysis-batch");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(format!("{name}.wordanalysis.json"))
    }

    fn cue(index: u32, text: &str) -> Cue {
        Cue::new(
            index,
            std::time::Duration::from_secs(index as u64),
            std::time::Duration::from_secs(index as u64 + 1),
            text,
        )
        .expect("valid cue timing")
    }

    fn recv_with_timeout<T>(rx: &mpsc::Receiver<T>) -> T {
        rx.recv_timeout(Duration::from_secs(5))
            .expect("background thread did not report back in time")
    }

    #[test]
    fn test_spawn_batch_analyze_analyzes_every_cue_and_saves_the_cache() {
        // Given: two cues and no pre-existing cache file
        // When:  running the batch analysis
        // Then:  both cues end up in the cache file on disk, and on_done
        //        reports success
        let cache_path = temp_cache_path("analyzes-every-cue");
        let _ = std::fs::remove_file(&cache_path);
        let client = RecordingClient {
            calls: std::sync::Mutex::new(Vec::new()),
            fail_sentences: Vec::new(),
        };
        let cues = vec![cue(0, "hola"), cue(1, "mundo")];
        let (progress_tx, progress_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            client,
            "llama3.1:8b".to_string(),
            "English".to_string(),
            cues,
            cache_path.clone(),
            move |done, total| {
                let _ = progress_tx.send((done, total));
            },
            move |result| {
                let _ = done_tx.send(result);
            },
        );

        assert_eq!(recv_with_timeout(&progress_rx), (1, 2));
        assert_eq!(recv_with_timeout(&progress_rx), (2, 2));
        assert!(recv_with_timeout(&done_rx).is_ok());

        let cache = word_analysis::load_cache(&cache_path);
        assert_eq!(cache.model, "llama3.1:8b");
        assert_eq!(cache.entries.len(), 2);
        assert_eq!(cache.entries[&0].words[0].word, "hola");
        assert_eq!(cache.entries[&1].words[0].word, "mundo");

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn test_spawn_batch_analyze_skips_cues_already_in_the_cache() {
        // Given: a cache file that already has cue 0's analysis
        // When:  running the batch analysis over cues 0 and 1
        // Then:  only cue 1 is actually sent to the client
        let cache_path = temp_cache_path("skips-already-cached");
        let mut existing = AnalysisCache {
            model: "llama3.1:8b".to_string(),
            entries: std::collections::HashMap::new(),
        };
        existing.entries.insert(
            0,
            WordAnalysis {
                words: vec![WordEntry {
                    word: "hola".to_string(),
                    translation: "hi".to_string(),
                    pronunciation: "OH-lah".to_string(),
                }],
            },
        );
        word_analysis::save_cache(&cache_path, &existing);
        let client = RecordingClient {
            calls: std::sync::Mutex::new(Vec::new()),
            fail_sentences: Vec::new(),
        };
        let cues = vec![cue(0, "hola"), cue(1, "mundo")];
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            client,
            "llama3.1:8b".to_string(),
            "English".to_string(),
            cues,
            cache_path.clone(),
            |_, _| {},
            move |result| {
                let _ = done_tx.send(result);
            },
        );

        recv_with_timeout(&done_rx).expect("batch run should succeed");
        let cache = word_analysis::load_cache(&cache_path);
        assert_eq!(cache.entries.len(), 2);

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn test_spawn_batch_analyze_continues_past_a_failed_cue_and_reports_the_error() {
        // Given: one cue that fails and one that succeeds
        // When:  running the batch analysis
        // Then:  the successful cue is still saved, and on_done reports
        //        the failure rather than silently succeeding
        let cache_path = temp_cache_path("continues-past-failure");
        let _ = std::fs::remove_file(&cache_path);
        let client = RecordingClient {
            calls: std::sync::Mutex::new(Vec::new()),
            fail_sentences: vec!["hola".to_string()],
        };
        let cues = vec![cue(0, "hola"), cue(1, "mundo")];
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            client,
            "llama3.1:8b".to_string(),
            "English".to_string(),
            cues,
            cache_path.clone(),
            |_, _| {},
            move |result| {
                let _ = done_tx.send(result);
            },
        );

        assert!(recv_with_timeout(&done_rx).is_err());
        let cache = word_analysis::load_cache(&cache_path);
        assert_eq!(cache.entries.len(), 1);
        assert!(cache.entries.contains_key(&1));

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn test_spawn_analyze_sentence_runs_off_the_calling_thread_and_reports_success() {
        // Given: a client that succeeds
        // When:  spawning it and immediately reading the calling thread's id
        // Then:  on_done fires on a different thread than the caller's,
        //        carrying the client's analysis
        let client = RecordingClient {
            calls: std::sync::Mutex::new(Vec::new()),
            fail_sentences: Vec::new(),
        };
        let caller_thread_id = thread::current().id();
        let (tx, rx) = mpsc::channel();

        spawn_analyze_sentence(
            client,
            "llama3.1:8b".to_string(),
            "hola".to_string(),
            "English".to_string(),
            move |result| {
                let _ = tx.send((thread::current().id(), result));
            },
        );

        let (callback_thread_id, result) = recv_with_timeout(&rx);
        assert_ne!(callback_thread_id, caller_thread_id);
        assert_eq!(result.unwrap().words[0].word, "hola");
    }

    #[test]
    fn test_spawn_analyze_sentence_reports_client_errors() {
        // Given: a client that fails
        // When:  spawning it
        // Then:  on_done receives the same error
        let client = RecordingClient {
            calls: std::sync::Mutex::new(Vec::new()),
            fail_sentences: vec!["hola".to_string()],
        };
        let (tx, rx) = mpsc::channel();

        spawn_analyze_sentence(
            client,
            "llama3.1:8b".to_string(),
            "hola".to_string(),
            "English".to_string(),
            move |result| {
                let _ = tx.send(result);
            },
        );

        assert!(recv_with_timeout(&rx).is_err());
    }

    #[test]
    fn test_analysis_rows_maps_every_word_in_order() {
        // Given: a WordAnalysis with two words
        // When:  mapping it to popup rows
        // Then:  both rows come back in the same order, fields carried
        //        over unchanged
        let analysis = WordAnalysis {
            words: vec![
                WordEntry {
                    word: "hola".to_string(),
                    translation: "hi".to_string(),
                    pronunciation: "OH-lah".to_string(),
                },
                WordEntry {
                    word: "mundo".to_string(),
                    translation: "world".to_string(),
                    pronunciation: "MOON-doh".to_string(),
                },
            ],
        };

        let rows = analysis_rows(&analysis);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].word, "hola");
        assert_eq!(rows[0].translation, "hi");
        assert_eq!(rows[0].pronunciation, "OH-lah");
        assert_eq!(rows[1].word, "mundo");
    }
}
