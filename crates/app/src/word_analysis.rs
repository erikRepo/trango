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
use std::time::Duration;

use niqud::NiqudClient;
use slint::VecModel;
use subtitle::Cue;
use word_analysis::{OllamaClient, OllamaError, WordAnalysis};

use crate::niqud_pronunciation::apply_niqud_pronunciation;
use crate::{AppWindow, WordAnalysisBatchStatus, WordAnalysisRow, WordAnalysisStatus};

/// Number of times `spawn_batch_analyze` calls `analyze_sentence` for a
/// single cue before giving up on it — covers a single transient Ollama
/// hiccup (e.g. a model occasionally dropping a field from its JSON reply)
/// without one flaky sentence needing a rerun of an otherwise long batch.
const MAX_ANALYZE_ATTEMPTS: u32 = 3;

/// Pause between retry attempts within `MAX_ANALYZE_ATTEMPTS` — long enough
/// to let a momentary hiccup pass, short enough not to noticeably slow a
/// batch run that's mostly succeeding.
const RETRY_DELAY: Duration = Duration::from_millis(500);

/// The two clients `spawn_batch_analyze`/`spawn_analyze_sentence` need:
/// `ollama` for translation/pronunciation, `niqud` for a Hebrew sentence's
/// own word boundaries and pronunciation (see [`analyze_sentence`]).
/// Bundled into one struct to keep both functions' parameter counts within
/// clippy's `too_many_arguments` limit.
pub struct AnalysisClients<C, N> {
    /// Talks to a local Ollama instance for word-by-word translation.
    pub ollama: C,
    /// Derives a Hebrew sentence's pronunciation guide from niqud, in
    /// place of Ollama's own (less reliable) guess.
    pub niqud: N,
}

/// Calls `attempt`, retrying up to `max_attempts` times (with
/// `RETRY_DELAY` between attempts) as long as it keeps failing. Returns the
/// last attempt's result, whichever way it went.
fn call_with_retries<T>(
    max_attempts: u32,
    mut attempt: impl FnMut() -> Result<T, OllamaError>,
) -> Result<T, OllamaError> {
    let mut count = 1;
    let mut result = attempt();
    while let Err(err) = &result {
        if count >= max_attempts {
            break;
        }
        tracing::warn!(attempt = count, %err, "retrying failed word analysis");
        thread::sleep(RETRY_DELAY);
        result = attempt();
        count += 1;
    }
    result
}

/// Analyzes `sentence` word-by-word, calling `attempt()` at most
/// `max_attempts` times as long as it keeps failing (see
/// [`call_with_retries`]). For a Hebrew sentence, niqud's own
/// whitespace-split word boundaries are fetched first and given to
/// `client.analyze_words` as a fixed list to fill in translations for —
/// rather than leaving word splitting itself up to Ollama's free-text
/// `analyze_sentence` prompt, which real use has shown drifts from
/// niqud's boundaries (e.g. over-splitting Hebrew's attached prefix
/// particles, despite being asked not to — see
/// `niqud_pronunciation::apply_niqud_pronunciation`'s doc comment). Falls
/// back to `analyze_sentence` when `sentence` isn't Hebrew, or when the
/// niqud call itself fails (logged as a warning; Ollama's own word
/// splitting is still better than no analysis at all).
fn analyze_sentence<C: OllamaClient, N: NiqudClient>(
    client: &C,
    niqud_client: &N,
    model: &str,
    sentence: &str,
    target_language: &str,
    max_attempts: u32,
) -> Result<WordAnalysis, OllamaError> {
    if niqud::contains_hebrew(sentence) {
        match niqud_client.transliterate_sentence(sentence) {
            Ok(niqud_result) => {
                let words: Vec<String> = niqud_result
                    .words
                    .iter()
                    .map(|word| word.word.clone())
                    .collect();
                return call_with_retries(max_attempts, || {
                    client.analyze_words(model, &words, target_language)
                })
                .map(|analysis| apply_niqud_pronunciation(&niqud_result, analysis));
            }
            Err(err) => {
                tracing::warn!(
                    %err,
                    %sentence,
                    "niqud transliteration failed, falling back to Ollama's own word splitting"
                );
            }
        }
    }
    call_with_retries(max_attempts, || {
        client.analyze_sentence(model, sentence, target_language)
    })
}

/// Runs word-by-word analysis for every cue in `cues` not already present
/// in the cache file at `cache_path`, calling `on_progress(done, total)`
/// after each cue (whether newly analyzed, skipped as already cached, or
/// failed) and `on_done` once the whole run finishes. Returns immediately
/// without blocking the calling thread.
///
/// A cue that still fails after `MAX_ANALYZE_ATTEMPTS` retries gets an
/// empty `WordAnalysis` saved in its place rather than aborting the run —
/// one bad sentence (e.g. a persistent Ollama JSON-shape mismatch)
/// shouldn't stop the rest of the subtitle from being analyzed, and
/// leaving the cue's cache entry blank (rather than absent) means it's
/// not retried again on every future run; `on_done` reports the *last*
/// error seen, if any, so the caller can surface that a run finished with
/// some cues left blank.
///
/// Each Hebrew cue is analyzed via [`analyze_sentence`]'s niqud-first path:
/// niqud's own word boundaries are fetched before Ollama is even called,
/// and its pronunciation replaces Ollama's guess afterward (see
/// `niqud_pronunciation::apply_niqud_pronunciation`).
pub fn spawn_batch_analyze<C, N>(
    clients: AnalysisClients<C, N>,
    model: String,
    target_language: String,
    cues: Vec<Cue>,
    cache_path: PathBuf,
    on_progress: impl Fn(usize, usize) + Send + 'static,
    on_done: impl FnOnce(Result<(), OllamaError>) + Send + 'static,
) where
    C: OllamaClient + Send + 'static,
    N: NiqudClient + Send + 'static,
{
    let AnalysisClients {
        ollama: client,
        niqud: niqud_client,
    } = clients;
    thread::spawn(move || {
        let mut cache = word_analysis::load_cache(&cache_path);
        cache.model = model.clone();
        let total = cues.len();
        let mut last_error = None;

        for (done, cue) in cues.iter().enumerate() {
            if let std::collections::hash_map::Entry::Vacant(entry) = cache.entries.entry(cue.index)
            {
                match analyze_sentence(
                    &client,
                    &niqud_client,
                    &model,
                    &cue.text,
                    &target_language,
                    MAX_ANALYZE_ATTEMPTS,
                ) {
                    Ok(analysis) => {
                        entry.insert(analysis);
                    }
                    Err(err) => {
                        tracing::warn!(
                            cue_index = cue.index,
                            %err,
                            "word analysis failed for cue after {MAX_ANALYZE_ATTEMPTS} attempts, leaving it blank"
                        );
                        entry.insert(WordAnalysis { words: Vec::new() });
                        last_error = Some(err);
                    }
                }
                word_analysis::save_cache(&cache_path, &cache);
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

/// Runs [`analyze_sentence`] (with no retries) on a background thread for
/// a single `sentence`, calling `on_done` with its result once finished —
/// the Ctrl+A popup's uncached-sentence path. Returns immediately without
/// blocking the calling thread.
pub fn spawn_analyze_sentence<C, N>(
    client: C,
    niqud_client: N,
    model: String,
    sentence: String,
    target_language: String,
    on_done: impl FnOnce(Result<WordAnalysis, OllamaError>) + Send + 'static,
) where
    C: OllamaClient + Send + 'static,
    N: NiqudClient + Send + 'static,
{
    thread::spawn(move || {
        let result = analyze_sentence(
            &client,
            &niqud_client,
            &model,
            &sentence,
            &target_language,
            1,
        );
        on_done(result);
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
            parts_label: parts_label(&entry.parts).into(),
        })
        .collect()
}

/// Formats a `WordEntry::parts` breakdown (e.g. Hebrew's ל "to" + סרטים
/// "movies", written attached with no space as לסרטים) into one display
/// line, e.g. `"ל = to · סרטים = movies"` — empty when there's nothing
/// to break down, which the popup uses to hide the line entirely.
fn parts_label(parts: &[word_analysis::WordPart]) -> String {
    parts
        .iter()
        .map(|part| format!("{} = {}", part.word, part.translation))
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use niqud::{NiqudError, NiqudResult, NiqudWord};
    use word_analysis::{AnalysisCache, WordAnalysis, WordEntry};

    use super::*;

    /// A `NiqudClient` test double that panics if ever called — every test
    /// in this module analyzes non-Hebrew sentences, so
    /// `apply_niqud_pronunciation`'s `contains_hebrew` short-circuit
    /// should always skip it. Its presence proves that end-to-end, not
    /// just at the unit level (see `niqud_pronunciation`'s own tests).
    struct NeverCalledNiqudClient;

    impl NiqudClient for NeverCalledNiqudClient {
        fn transliterate_sentence(&self, _sentence: &str) -> Result<NiqudResult, NiqudError> {
            unreachable!("niqud client should never be called for a non-Hebrew sentence")
        }
    }

    /// A `NiqudClient` test double returning a fixed successful result
    /// with one word matching whatever single-word Hebrew sentence the
    /// batch-analysis integration test below uses.
    struct FixedNiqudClient;

    impl NiqudClient for FixedNiqudClient {
        fn transliterate_sentence(&self, _sentence: &str) -> Result<NiqudResult, NiqudError> {
            Ok(NiqudResult {
                words: vec![NiqudWord {
                    word: "שכב".to_string(),
                    niqud: "שָׁכַב".to_string(),
                    pronunciation: "sha-khav".to_string(),
                }],
            })
        }
    }

    /// An `OllamaClient` test double whose `analyze_sentence` returns a
    /// fixed result per call count, and records every sentence it was
    /// asked to analyze — enough to check "already-cached cues are
    /// skipped", "failures don't abort the run", and "a sentence that
    /// fails a few times before succeeding is retried rather than given
    /// up on immediately" without any real network I/O.
    #[derive(Default)]
    struct RecordingClient {
        /// `Arc`-wrapped so a test can hold on to a clone and inspect the
        /// call count after the client itself has been moved into
        /// `spawn_batch_analyze`.
        calls: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        /// Sentences that fail every single call, however many times
        /// they're retried.
        fail_sentences: Vec<String>,
        /// Sentences that fail their first N calls and then succeed —
        /// keyed by sentence text, value is the remaining failure count
        /// (decremented on each call).
        flaky_sentences: std::sync::Mutex<std::collections::HashMap<String, u32>>,
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
            if let Some(remaining) = self.flaky_sentences.lock().unwrap().get_mut(sentence) {
                if *remaining > 0 {
                    *remaining -= 1;
                    return Err(OllamaError::ConnectionFailed(
                        "transient test failure".to_string(),
                    ));
                }
            }
            Ok(WordAnalysis {
                words: vec![WordEntry {
                    word: sentence.to_string(),
                    translation: "translated".to_string(),
                    pronunciation: "pronounced".to_string(),
                    parts: Vec::new(),
                }],
            })
        }

        fn analyze_words(
            &self,
            _model: &str,
            words: &[String],
            _target_language: &str,
        ) -> Result<WordAnalysis, OllamaError> {
            let joined = words.join(" ");
            self.calls.lock().unwrap().push(joined.clone());
            if self.fail_sentences.contains(&joined) {
                return Err(OllamaError::ConnectionFailed(
                    "fixed test failure".to_string(),
                ));
            }
            if let Some(remaining) = self.flaky_sentences.lock().unwrap().get_mut(&joined) {
                if *remaining > 0 {
                    *remaining -= 1;
                    return Err(OllamaError::ConnectionFailed(
                        "transient test failure".to_string(),
                    ));
                }
            }
            Ok(WordAnalysis {
                words: words
                    .iter()
                    .map(|word| WordEntry {
                        word: word.clone(),
                        translation: "translated".to_string(),
                        pronunciation: "pronounced".to_string(),
                        parts: Vec::new(),
                    })
                    .collect(),
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
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fail_sentences: Vec::new(),
            ..Default::default()
        };
        let cues = vec![cue(0, "hola"), cue(1, "mundo")];
        let (progress_tx, progress_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            AnalysisClients {
                ollama: client,
                niqud: NeverCalledNiqudClient,
            },
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
                    parts: Vec::new(),
                }],
            },
        );
        word_analysis::save_cache(&cache_path, &existing);
        let client = RecordingClient {
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fail_sentences: Vec::new(),
            ..Default::default()
        };
        let cues = vec![cue(0, "hola"), cue(1, "mundo")];
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            AnalysisClients {
                ollama: client,
                niqud: NeverCalledNiqudClient,
            },
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
        // Given: one cue that always fails and one that succeeds
        // When:  running the batch analysis
        // Then:  the successful cue is saved normally, the failing cue is
        //        retried (not just given up on after one failure) and
        //        ends up saved with an empty analysis rather than being
        //        left out of the cache entirely, and on_done reports the
        //        failure rather than silently succeeding
        let cache_path = temp_cache_path("continues-past-failure");
        let _ = std::fs::remove_file(&cache_path);
        let client = RecordingClient {
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fail_sentences: vec!["hola".to_string()],
            ..Default::default()
        };
        let cues = vec![cue(0, "hola"), cue(1, "mundo")];
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            AnalysisClients {
                ollama: client,
                niqud: NeverCalledNiqudClient,
            },
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
        assert_eq!(cache.entries.len(), 2);
        assert!(cache.entries[&0].words.is_empty());
        assert_eq!(cache.entries[&1].words[0].word, "mundo");

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn test_spawn_batch_analyze_retries_a_cue_that_fails_a_couple_of_times_then_succeeds() {
        // Given: a cue whose first two calls fail but the third succeeds
        //        (e.g. a transient Ollama hiccup), well within
        //        MAX_ANALYZE_ATTEMPTS
        // When:  running the batch analysis
        // Then:  the cue ends up saved with the successful analysis, not
        //        left blank, and on_done reports success
        let cache_path = temp_cache_path("retries-then-succeeds");
        let _ = std::fs::remove_file(&cache_path);
        let mut flaky = std::collections::HashMap::new();
        flaky.insert("hola".to_string(), 2);
        let client = RecordingClient {
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            flaky_sentences: std::sync::Mutex::new(flaky),
            ..Default::default()
        };
        let cues = vec![cue(0, "hola")];
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            AnalysisClients {
                ollama: client,
                niqud: NeverCalledNiqudClient,
            },
            "llama3.1:8b".to_string(),
            "English".to_string(),
            cues,
            cache_path.clone(),
            |_, _| {},
            move |result| {
                let _ = done_tx.send(result);
            },
        );

        recv_with_timeout(&done_rx).expect("batch run should succeed after retrying");
        let cache = word_analysis::load_cache(&cache_path);
        assert_eq!(cache.entries[&0].words[0].word, "hola");

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn test_spawn_batch_analyze_gives_up_after_max_attempts() {
        // Given: a cue that always fails
        // When:  running the batch analysis
        // Then:  the client is called exactly MAX_ANALYZE_ATTEMPTS times
        //        for that cue, not once and not indefinitely
        let cache_path = temp_cache_path("gives-up-after-max-attempts");
        let _ = std::fs::remove_file(&cache_path);
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let client = RecordingClient {
            calls: calls.clone(),
            fail_sentences: vec!["hola".to_string()],
            ..Default::default()
        };
        let cues = vec![cue(0, "hola")];
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            AnalysisClients {
                ollama: client,
                niqud: NeverCalledNiqudClient,
            },
            "llama3.1:8b".to_string(),
            "English".to_string(),
            cues,
            cache_path.clone(),
            |_, _| {},
            move |result| {
                let _ = done_tx.send(result);
            },
        );

        recv_with_timeout(&done_rx).expect_err("batch run should report the failure");
        assert_eq!(calls.lock().unwrap().len(), MAX_ANALYZE_ATTEMPTS as usize);

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn test_spawn_batch_analyze_applies_niqud_pronunciation_for_hebrew_cues() {
        // Given: a Hebrew cue and a niqud client returning a matching word
        // When:  running the batch analysis
        // Then:  the cached analysis carries the niqud-derived
        //        pronunciation, not Ollama's own guess
        let cache_path = temp_cache_path("applies-niqud-pronunciation");
        let _ = std::fs::remove_file(&cache_path);
        let client = RecordingClient {
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fail_sentences: Vec::new(),
            ..Default::default()
        };
        let cues = vec![cue(0, "שכב")];
        let (done_tx, done_rx) = mpsc::channel();

        spawn_batch_analyze(
            AnalysisClients {
                ollama: client,
                niqud: FixedNiqudClient,
            },
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
        assert_eq!(cache.entries[&0].words[0].pronunciation, "sha-khav");

        let _ = std::fs::remove_file(&cache_path);
    }

    #[test]
    fn test_spawn_analyze_sentence_runs_off_the_calling_thread_and_reports_success() {
        // Given: a client that succeeds
        // When:  spawning it and immediately reading the calling thread's id
        // Then:  on_done fires on a different thread than the caller's,
        //        carrying the client's analysis
        let client = RecordingClient {
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fail_sentences: Vec::new(),
            ..Default::default()
        };
        let caller_thread_id = thread::current().id();
        let (tx, rx) = mpsc::channel();

        spawn_analyze_sentence(
            client,
            NeverCalledNiqudClient,
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
            calls: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
            fail_sentences: vec!["hola".to_string()],
            ..Default::default()
        };
        let (tx, rx) = mpsc::channel();

        spawn_analyze_sentence(
            client,
            NeverCalledNiqudClient,
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
                    parts: Vec::new(),
                },
                WordEntry {
                    word: "mundo".to_string(),
                    translation: "world".to_string(),
                    pronunciation: "MOON-doh".to_string(),
                    parts: Vec::new(),
                },
            ],
        };

        let rows = analysis_rows(&analysis);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].word, "hola");
        assert_eq!(rows[0].translation, "hi");
        assert_eq!(rows[0].pronunciation, "OH-lah");
        assert_eq!(rows[0].parts_label, "");
        assert_eq!(rows[1].word, "mundo");
    }

    #[test]
    fn test_analysis_rows_formats_parts_label_for_a_prefixed_hebrew_word() {
        // Given: a WordEntry for a prefixed Hebrew word, broken into parts
        // When:  mapping it to popup rows
        // Then:  word/pronunciation stay as the whole combined form, and
        //        parts_label carries a readable breakdown of the parts
        let analysis = WordAnalysis {
            words: vec![WordEntry {
                word: "לסרטים".to_string(),
                translation: "to the movies".to_string(),
                pronunciation: "le-sratim".to_string(),
                parts: vec![
                    word_analysis::WordPart {
                        word: "ל".to_string(),
                        translation: "to".to_string(),
                    },
                    word_analysis::WordPart {
                        word: "סרטים".to_string(),
                        translation: "movies".to_string(),
                    },
                ],
            }],
        };

        let rows = analysis_rows(&analysis);

        assert_eq!(rows[0].word, "לסרטים");
        assert_eq!(rows[0].pronunciation, "le-sratim");
        assert_eq!(rows[0].parts_label, "ל = to · סרטים = movies");
    }
}
