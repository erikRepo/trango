//! Wires the Open Subtitles dialog's "Generate subtitles" button (`TODO.md`
//! Vaihe 20) to `subtitle::SubtitleGenerator`, mirroring the resulting
//! `Idle -> Generating -> Done`/`Error` transition into the window's
//! `subtitle-generation-status` property.
//!
//! `generate` runs synchronously — fine while the only implementation is
//! `subtitle::StubSubtitleGenerator`, which is instant. A real
//! speech-to-text backend (a later, separate step) will likely take long
//! enough to need moving off the UI thread.

use std::path::{Path, PathBuf};

use subtitle::SubtitleGenerator;

use crate::{open_subtitles_dialog, AppWindow, SubtitleGenerationStatus};

/// Runs `generator` against `video_path`, mirroring the resulting status
/// into the window's `subtitle-generation-status` property and, on
/// success, the dialog's original-language row
/// (`open_subtitles_dialog::mark_original_linked`). Returns the generated
/// subtitle's path on success so the caller (`main.rs`'s
/// `on_generate_subtitles_requested`) can load it into the player and
/// record it in `CurrentMedia`; returns `None` and leaves the status at
/// `Error` if generation failed.
pub fn generate(
    window: &AppWindow,
    generator: &dyn SubtitleGenerator,
    video_path: &Path,
) -> Option<PathBuf> {
    window.set_subtitle_generation_status(SubtitleGenerationStatus::Generating);
    match generator.generate(video_path) {
        Ok(subtitle_path) => {
            tracing::info!(?subtitle_path, "subtitle generation succeeded");
            window.set_subtitle_generation_status(SubtitleGenerationStatus::Done);
            open_subtitles_dialog::mark_original_linked(window, &subtitle_path);
            Some(subtitle_path)
        }
        Err(err) => {
            tracing::warn!(%err, ?video_path, "subtitle generation failed");
            window.set_subtitle_generation_status(SubtitleGenerationStatus::Error);
            None
        }
    }
}
