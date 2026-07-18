//! Ollama model selection (`TODO.md` Vaihe 24, part 3/6): an in-app picker
//! — reusing `app-window.slint`'s `FileListDialog` chrome, same as the
//! whisper.cpp model picker (`model_picker.rs`) — but listing models
//! reported by a running Ollama instance instead of browsing the
//! filesystem, since there's no folder structure to navigate.
//!
//! Unlike the whisper.cpp picker, listing models is a network call
//! (`word_analysis::OllamaClient::list_models`), so it can't just run
//! synchronously on the UI thread the way `model_picker::list_folder_entries`
//! does — `spawn_list_models` runs it on a background thread, mirroring
//! `subtitle_generation::spawn_generate`.

use std::rc::Rc;
use std::thread;

use slint::VecModel;
use word_analysis::{OllamaClient, OllamaError};

use crate::{AppWindow, FileListRow};

/// Runs `client.list_models()` on a background thread, calling `on_done`
/// with its result once finished. Returns immediately without blocking
/// the calling thread — a network call to Ollama shouldn't freeze the UI,
/// even though it's usually fast for a local instance.
pub fn spawn_list_models<C>(
    client: C,
    on_done: impl FnOnce(Result<Vec<String>, OllamaError>) + Send + 'static,
) where
    C: OllamaClient + Send + 'static,
{
    thread::spawn(move || {
        on_done(client.list_models());
    });
}

/// Opens the picker in a loading state — an empty row list and
/// `ollama-model-picker-loading: true` — while `spawn_list_models` runs in
/// the background. `main.rs`'s handler calls `apply_models_result` once it
/// reports back.
pub fn open_dialog_loading(window: &AppWindow) {
    window.set_ollama_model_picker_rows(Rc::new(VecModel::from(Vec::<FileListRow>::new())).into());
    window.set_ollama_model_picker_selected_index(-1);
    window.set_ollama_model_picker_loading(true);
    window.set_ollama_model_picker_error("".into());
    window.set_is_ollama_model_picker_open(true);
}

/// Applies a finished `list_models()` call to the picker: on success,
/// rebuilds the row list (pre-selecting `current_model` if it's among the
/// results) and returns the model names for `main.rs` to hold onto for
/// later row clicks; on failure, shows `err` as an inline message and
/// returns an empty list.
pub fn apply_models_result(
    window: &AppWindow,
    result: Result<Vec<String>, OllamaError>,
    current_model: Option<&str>,
) -> Vec<String> {
    window.set_ollama_model_picker_loading(false);
    match result {
        Ok(models) => {
            let selected_index = selected_index_of(&models, current_model);
            window.set_ollama_model_picker_rows(
                Rc::new(VecModel::from(model_rows(&models, selected_index))).into(),
            );
            window.set_ollama_model_picker_selected_index(selected_index);
            models
        }
        Err(err) => {
            tracing::warn!(%err, "failed to list Ollama models");
            window.set_ollama_model_picker_error(err.to_string().into());
            Vec::new()
        }
    }
}

/// Rebuilds the row model with `selected_index` marked current — used
/// when a model row is clicked (`main.rs`'s `on_select_ollama_model_picker_row`).
pub fn mark_selected(window: &AppWindow, models: &[String], selected_index: i32) {
    window.set_ollama_model_picker_rows(
        Rc::new(VecModel::from(model_rows(models, selected_index))).into(),
    );
    window.set_ollama_model_picker_selected_index(selected_index);
}

/// `models`' index matching `current_model`, or `-1` if `current_model` is
/// `None` or isn't among `models` (e.g. it was removed from Ollama since
/// last picked).
fn selected_index_of(models: &[String], current_model: Option<&str>) -> i32 {
    current_model
        .and_then(|current| models.iter().position(|model| model == current))
        .map_or(-1, |index| index as i32)
}

/// Maps `models` into the shared `FileListDialog` row model: no size
/// label (models aren't sized like files), never navigable (this picker
/// has no subfolders), with the entry at `selected_index` (if any) marked
/// current.
fn model_rows(models: &[String], selected_index: i32) -> Vec<FileListRow> {
    models
        .iter()
        .enumerate()
        .map(|(index, name)| FileListRow {
            name: name.clone().into(),
            size_label: "".into(),
            is_selected: usize::try_from(selected_index).ok() == Some(index),
            is_navigable: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// An `OllamaClient` test double that returns a fixed `list_models`
    /// result without any network I/O — `spawn_list_models`'s job is just
    /// to move the call off the calling thread and report back, which
    /// doesn't need a real Ollama instance to exercise.
    struct FixedResultClient {
        result: Result<Vec<String>, OllamaErrorKind>,
    }

    /// A cheaply constructible stand-in for `OllamaError` (which isn't
    /// `Clone`), just enough to build the two outcomes these tests check.
    enum OllamaErrorKind {
        ConnectionFailed,
    }

    impl OllamaClient for FixedResultClient {
        fn list_models(&self) -> Result<Vec<String>, OllamaError> {
            match &self.result {
                Ok(models) => Ok(models.clone()),
                Err(OllamaErrorKind::ConnectionFailed) => Err(OllamaError::ConnectionFailed(
                    "fixed test failure".to_string(),
                )),
            }
        }

        fn analyze_sentence(
            &self,
            _model: &str,
            _sentence: &str,
            _target_language: &str,
        ) -> Result<word_analysis::WordAnalysis, OllamaError> {
            unreachable!("not exercised by these tests")
        }

        fn analyze_words(
            &self,
            _model: &str,
            _words: &[String],
            _target_language: &str,
        ) -> Result<word_analysis::WordAnalysis, OllamaError> {
            unreachable!("not exercised by these tests")
        }
    }

    /// Blocks up to a few seconds for `rx` to receive a value — see
    /// `subtitle_generation.rs`'s identically-named helper for why.
    fn recv_with_timeout<T>(rx: &mpsc::Receiver<T>) -> T {
        rx.recv_timeout(Duration::from_secs(5))
            .expect("background thread did not report back in time")
    }

    #[test]
    fn test_spawn_list_models_runs_off_the_calling_thread_and_reports_success() {
        // Given: a client that succeeds
        // When:  spawning it and immediately reading the calling thread's id
        // Then:  on_done fires on a different thread than the caller's,
        //        carrying the client's model list
        let client = FixedResultClient {
            result: Ok(vec!["llama3.1:8b".to_string(), "gemma2:9b".to_string()]),
        };
        let caller_thread_id = thread::current().id();
        let (tx, rx) = mpsc::channel();

        spawn_list_models(client, move |result| {
            let _ = tx.send((thread::current().id(), result));
        });

        let (callback_thread_id, result) = recv_with_timeout(&rx);
        assert_ne!(callback_thread_id, caller_thread_id);
        assert_eq!(
            result.unwrap(),
            vec!["llama3.1:8b".to_string(), "gemma2:9b".to_string()]
        );
    }

    #[test]
    fn test_spawn_list_models_reports_client_errors() {
        // Given: a client that fails
        // When:  spawning it
        // Then:  on_done receives the same error
        let client = FixedResultClient {
            result: Err(OllamaErrorKind::ConnectionFailed),
        };
        let (tx, rx) = mpsc::channel();

        spawn_list_models(client, move |result| {
            let _ = tx.send(result);
        });

        let result = recv_with_timeout(&rx);
        assert!(
            matches!(result, Err(OllamaError::ConnectionFailed(message)) if message == "fixed test failure")
        );
    }

    #[test]
    fn test_selected_index_of_finds_current_model() {
        // Given: a model list containing the currently configured model
        // When:  resolving its index
        // Then:  the matching index comes back
        let models = vec!["llama3.1:8b".to_string(), "gemma2:9b".to_string()];

        assert_eq!(selected_index_of(&models, Some("gemma2:9b")), 1);
    }

    #[test]
    fn test_selected_index_of_missing_or_none_returns_negative_one() {
        // Given/When/Then: no current model, or one no longer in the list,
        //        both resolve to -1 (no row marked selected)
        let models = vec!["llama3.1:8b".to_string()];

        assert_eq!(selected_index_of(&models, None), -1);
        assert_eq!(selected_index_of(&models, Some("not-installed:1b")), -1);
    }

    #[test]
    fn test_model_rows_marks_selected_index() {
        // Given: three candidate models
        // When:  building picker rows with the second one selected
        // Then:  only that row is marked selected, none are navigable, and
        //        none have a size label
        let models = vec!["a".to_string(), "b".to_string(), "c".to_string()];

        let rows = model_rows(&models, 1);

        assert_eq!(rows.len(), 3);
        assert!(!rows[0].is_selected);
        assert!(rows[1].is_selected);
        assert!(!rows[2].is_selected);
        assert!(rows.iter().all(|row| !row.is_navigable));
        assert!(rows.iter().all(|row| row.size_label.is_empty()));
    }
}
