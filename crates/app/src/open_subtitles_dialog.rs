//! Open Subtitles dialog (Vaihe 19): a modal scoped to the currently open
//! video, showing whether an original-language subtitle was found next to
//! it (linked row) or not (empty state + "Generate subtitles" stub, see
//! `TODO.md` Vaihe 20), plus a translation section that can be linked via a
//! small in-app file picker.
//!
//! SPEC.md's mock links the translation section via drag-and-drop from the
//! OS file manager, but that isn't available: Slint 1.17.1's winit backend
//! doesn't handle `winit::event::WindowEvent::DroppedFile` at all, so its
//! `DropArea` only ever receives drags started by an in-app `DragArea` —
//! there's no such source here. `main.rs`'s `wire_open_subtitles_dialog`
//! instead reuses the `FileListDialog` component (`open_media_dialog.rs`'s
//! Open dialog) as a small "pick a `.srt` from this folder" picker.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::VecModel;

use crate::{AppWindow, FileListRow, SubtitleGenerationStatus};

/// The original-language and translation subtitle paths already linked to
/// the video the Open Subtitles dialog is scoped to, if any.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubtitleLinks {
    /// Path to the linked original-language subtitle, if one was found.
    pub original_path: Option<PathBuf>,
    /// Path to the linked translation subtitle, if one has been picked.
    pub translation_path: Option<PathBuf>,
}

/// Opens the Open Subtitles modal for `video_path`: title "Subtitles for
/// {filename}", the original section showing `links.original_path` as a
/// linked row (or the empty "No subtitle file found" state if `None`), and
/// the translation section showing `links.translation_path` the same way.
pub fn open_dialog(window: &AppWindow, video_path: &Path, links: &SubtitleLinks) {
    window.set_open_subtitles_title(format!("Subtitles for {}", file_name(video_path)).into());
    set_original(window, links.original_path.as_deref());
    set_translation(window, links.translation_path.as_deref());
    window.set_subtitle_generation_status(SubtitleGenerationStatus::Idle);
    window.set_subtitle_generation_error_message("".into());
    window.set_is_open_subtitles_dialog_open(true);
}

/// Mirrors `path` into the dialog's original-language row/empty-state
/// properties.
fn set_original(window: &AppWindow, path: Option<&Path>) {
    window.set_open_subtitles_original_linked(path.is_some());
    window.set_open_subtitles_original_name(path.map(file_name).unwrap_or_default().into());
}

/// Mirrors `path` into the dialog's translation row/empty-state
/// properties.
fn set_translation(window: &AppWindow, path: Option<&Path>) {
    window.set_open_subtitles_translation_linked(path.is_some());
    window.set_open_subtitles_translation_name(path.map(file_name).unwrap_or_default().into());
}

/// Mirrors a just-linked translation file into the dialog's translation
/// row, without touching the original section or the dialog's open state —
/// called by `main.rs` after `confirm-link-translation` successfully
/// re-merges cues with the newly picked translation.
pub fn mark_translation_linked(window: &AppWindow, path: &Path) {
    set_translation(window, Some(path));
}

/// Mirrors a just-generated original-language subtitle into the dialog's
/// original row, without touching the translation section or the dialog's
/// open state — called by `subtitle_generation::generate` after a
/// successful `subtitle::SubtitleGenerator::generate` call.
pub fn mark_original_linked(window: &AppWindow, path: &Path) {
    set_original(window, Some(path));
}

/// `path`'s file name, or the full path rendered as a string if it has
/// none (defensive — every path reaching this dialog names a real file).
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Lists `.srt` files directly inside `folder` (case-insensitive
/// extension match), sorted by name — the translation-link file picker's
/// contents. Unlike the Open dialog
/// (`open_media_dialog::list_folder_entries`), there's no subfolder
/// navigation: a translation file is expected right next to the video, and
/// this picker is reached from inside the already-scoped Open Subtitles
/// modal rather than being a general-purpose file browser. Returns an
/// empty list (logging a warning) if `folder` can't be read.
pub fn list_srt_files(folder: &Path) -> Vec<PathBuf> {
    let dir_entries = match fs::read_dir(folder) {
        Ok(dir_entries) => dir_entries,
        Err(err) => {
            tracing::warn!(?folder, %err, "failed to read folder for translation subtitles");
            return Vec::new();
        }
    };

    let mut files: Vec<PathBuf> = dir_entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("srt"))
        })
        .collect();
    files.sort();
    files
}

/// Rebuilds the translation-link picker's row model from `entries` and
/// opens it, pre-selecting nothing (unlike the Open Video dialog, there's
/// no "obvious first pick" convention among same-folder subtitle files —
/// the learner knows which language they want).
pub fn open_translation_picker(window: &AppWindow, folder: &Path, entries: &[PathBuf]) {
    window.set_link_translation_folder_label(folder.display().to_string().into());
    window.set_link_translation_rows(Rc::new(VecModel::from(picker_rows(entries, -1))).into());
    window.set_link_translation_selected_index(-1);
    window.set_is_link_translation_dialog_open(true);
}

/// Rebuilds the picker's row model with `selected_index` marked current —
/// used when a row is clicked (`main.rs`'s `on_select_link_translation_row`).
pub fn mark_translation_selected(window: &AppWindow, entries: &[PathBuf], selected_index: i32) {
    window.set_link_translation_rows(
        Rc::new(VecModel::from(picker_rows(entries, selected_index))).into(),
    );
}

/// Maps `entries` into the shared `FileListDialog` row model: no
/// size label (translation files aren't sized like videos are) and never
/// navigable (this picker has no subfolders), with the entry at
/// `selected_index` (if any) marked current.
fn picker_rows(entries: &[PathBuf], selected_index: i32) -> Vec<FileListRow> {
    entries
        .iter()
        .enumerate()
        .map(|(index, path)| FileListRow {
            name: file_name(path).into(),
            size_label: "".into(),
            is_selected: usize::try_from(selected_index).ok() == Some(index),
            is_navigable: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/sample")
    }

    #[test]
    fn test_list_srt_files_finds_both_real_sample_srt_files() {
        // Given: the repo's real test-media/sample fixture folder, which
        //        has two .srt files (sample.srt, sample.fi.srt) alongside
        //        one .mp4
        // When:  listing its .srt files
        // Then:  both come back, sorted by name
        let files = list_srt_files(&fixture_dir());

        assert_eq!(
            files,
            vec![
                fixture_dir().join("sample.fi.srt"),
                fixture_dir().join("sample.srt"),
            ]
        );
    }

    #[test]
    fn test_list_srt_files_missing_folder_returns_empty() {
        // Given: a folder that doesn't exist
        // When:  listing its .srt files
        // Then:  an empty list comes back, rather than panicking
        let files = list_srt_files(Path::new("/no/such/folder/trango-test"));

        assert!(files.is_empty());
    }

    #[test]
    fn test_list_srt_files_filters_and_sorts() {
        // Given: a temp folder with mixed .srt/non-.srt files (one with an
        //        uppercase extension), created out of order
        // When:  listing its .srt files
        // Then:  only .srt files come back, sorted by name
        let dir = std::env::temp_dir().join("trango-test-list-srt-files-filters-and-sorts");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create temp test dir");
        fs::write(dir.join("b.srt"), b"").expect("failed to write fixture file");
        fs::write(dir.join("a.SRT"), b"").expect("failed to write fixture file");
        fs::write(dir.join("notes.txt"), b"").expect("failed to write fixture file");

        let files = list_srt_files(&dir);

        assert_eq!(files, vec![dir.join("a.SRT"), dir.join("b.srt")]);

        fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_file_name_returns_the_file_name_component() {
        // Given/When/Then: a normal path's file name is extracted
        assert_eq!(
            file_name(Path::new("/videos/der_anruf.srt")),
            "der_anruf.srt"
        );
    }

    #[test]
    fn test_picker_rows_marks_selected_index() {
        // Given: three candidate translation files
        // When:  building picker rows with the second one selected
        // Then:  only that row is marked selected, none are navigable, and
        //        none have a size label
        let entries = vec![
            PathBuf::from("/videos/a.srt"),
            PathBuf::from("/videos/b.srt"),
            PathBuf::from("/videos/c.srt"),
        ];

        let rows = picker_rows(&entries, 1);

        assert_eq!(rows.len(), 3);
        assert!(!rows[0].is_selected);
        assert!(rows[1].is_selected);
        assert!(!rows[2].is_selected);
        assert!(rows.iter().all(|row| !row.is_navigable));
        assert!(rows.iter().all(|row| row.size_label.is_empty()));
    }
}
