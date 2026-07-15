//! Wires the Open Subtitles dialog's "Generate subtitles" button
//! (`TODO.md` Vaihe 20/21.5) to `subtitle::SubtitleGenerator`, mirroring the
//! resulting `Idle -> Generating -> Done`/`Error` transition into the
//! window's `subtitle-generation-status` property.
//!
//! `spawn_generate` runs a generator on a background thread — needed for
//! real speech-to-text (`subtitle::WhisperCliGenerator`), which can take
//! seconds to minutes and would freeze the UI thread if run directly on
//! it — handing its result to a caller-supplied callback (`main.rs`'s
//! `wire_open_subtitles_dialog` posts that callback back onto the UI
//! thread via `slint::invoke_from_event_loop`, mirroring
//! `video_player.rs`'s `load_file` pattern) so the app-window-touching part
//! of the flow (`apply_result`) still only ever runs on the UI thread.

use std::path::PathBuf;
use std::thread;

use subtitle::{SubtitleError, SubtitleGenerator};

use crate::{open_subtitles_dialog, AppWindow, SubtitleGenerationStatus};

/// Runs `generator.generate(&video_path)` on a background thread, calling
/// `on_done` with its result once finished. Returns immediately without
/// blocking the calling thread — used for `subtitle::WhisperCliGenerator`,
/// whose real transcription can take seconds to minutes (`TODO.md` Vaihe
/// 21.5), so `subtitle-generation-status` needs to stay `Generating` while
/// the rest of the UI stays responsive. Doesn't touch `AppWindow` itself:
/// callers that need to update one (as `main.rs` does) should do so inside
/// `on_done`, dispatched via `slint::invoke_from_event_loop` since `on_done`
/// runs on the background thread, not the UI thread.
pub fn spawn_generate<G>(
    generator: G,
    video_path: PathBuf,
    on_done: impl FnOnce(Result<PathBuf, SubtitleError>) + Send + 'static,
) where
    G: SubtitleGenerator + Send + 'static,
{
    thread::spawn(move || {
        on_done(generator.generate(&video_path));
    });
}

/// Mirrors a finished `SubtitleGenerator::generate` call's `result` into
/// `window`'s `subtitle-generation-status`/`-error-message` properties and,
/// on success, the Open Subtitles dialog's original row
/// (`open_subtitles_dialog::mark_original_linked`). Returns the generated
/// subtitle's path on success so the caller (`main.rs`'s background-thread
/// callback) can hand it off to `AppWindow::subtitle-generated` for loading
/// into the player and recording in `CurrentMedia`; returns `None` and
/// leaves the status at `Error` (with `error-message` set to `result`'s
/// message) otherwise. Must be called on the UI thread — `AppWindow`
/// property setters aren't safe to call from a background thread.
pub fn apply_result(window: &AppWindow, result: Result<PathBuf, SubtitleError>) -> Option<PathBuf> {
    match result {
        Ok(subtitle_path) => {
            tracing::info!(?subtitle_path, "subtitle generation succeeded");
            window.set_subtitle_generation_status(SubtitleGenerationStatus::Done);
            window.set_subtitle_generation_error_message("".into());
            open_subtitles_dialog::mark_original_linked(window, &subtitle_path);
            Some(subtitle_path)
        }
        Err(err) => {
            tracing::warn!(%err, "subtitle generation failed");
            window.set_subtitle_generation_status(SubtitleGenerationStatus::Error);
            window.set_subtitle_generation_error_message(err.to_string().into());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    /// A `SubtitleGenerator` test double that returns a fixed `result`
    /// without touching disk — `spawn_generate`'s job is just to move work
    /// off the calling thread and report back, which doesn't need a real
    /// generator to exercise.
    struct FixedResultGenerator {
        result: Result<PathBuf, SubtitleErrorKind>,
    }

    /// A cheaply cloneable/constructible stand-in for `SubtitleError`
    /// (which isn't `Clone`), just enough to build the two outcomes these
    /// tests check for.
    #[derive(Debug, PartialEq, Eq)]
    enum SubtitleErrorKind {
        GenerationFailed,
    }

    impl SubtitleGenerator for FixedResultGenerator {
        fn generate(&self, _video_path: &Path) -> Result<PathBuf, SubtitleError> {
            match &self.result {
                Ok(path) => Ok(path.clone()),
                Err(SubtitleErrorKind::GenerationFailed) => Err(SubtitleError::GenerationFailed(
                    "fixed test failure".to_string(),
                )),
            }
        }
    }

    /// Blocks up to a few seconds for `rx` to receive a value — long
    /// enough that a genuinely broken background thread fails the test
    /// instead of hanging it forever, short enough not to slow down a
    /// passing run noticeably.
    fn recv_with_timeout<T>(rx: &mpsc::Receiver<T>) -> T {
        rx.recv_timeout(Duration::from_secs(5))
            .expect("background thread did not report back in time")
    }

    #[test]
    fn test_spawn_generate_runs_off_the_calling_thread_and_reports_success() {
        // Given: a generator that succeeds
        // When:  spawning it and immediately reading the calling thread's id
        // Then:  on_done fires on a different thread than the caller's,
        //        carrying the generator's Ok result
        let generator = FixedResultGenerator {
            result: Ok(PathBuf::from("/videos/some_video.srt")),
        };
        let caller_thread_id = thread::current().id();
        let (tx, rx) = mpsc::channel();

        spawn_generate(
            generator,
            PathBuf::from("/videos/some_video.mp4"),
            move |result| {
                let _ = tx.send((thread::current().id(), result));
            },
        );

        let (callback_thread_id, result) = recv_with_timeout(&rx);
        assert_ne!(callback_thread_id, caller_thread_id);
        assert_eq!(result.unwrap(), PathBuf::from("/videos/some_video.srt"));
    }

    #[test]
    fn test_spawn_generate_reports_generator_errors() {
        // Given: a generator that fails
        // When:  spawning it
        // Then:  on_done receives the same error
        let generator = FixedResultGenerator {
            result: Err(SubtitleErrorKind::GenerationFailed),
        };
        let (tx, rx) = mpsc::channel();

        spawn_generate(
            generator,
            PathBuf::from("/videos/some_video.mp4"),
            move |result| {
                let _ = tx.send(result);
            },
        );

        let result = recv_with_timeout(&rx);
        assert!(
            matches!(result, Err(SubtitleError::GenerationFailed(message)) if message == "fixed test failure")
        );
    }
}
