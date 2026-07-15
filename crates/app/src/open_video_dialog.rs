//! Open Video dialog (Vaihe 18): an in-app modal listing video files from a
//! folder — no OS-native file picker, per README's `#2a` mock — plus
//! same-stem `.srt` auto-matching once a file is opened. Folder switching
//! (browsing to a different folder from inside the dialog) is out of scope
//! here; see `docs/src/specs/README.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::VecModel;

use crate::{AppWindow, OpenVideoFileRow};

/// Video file extensions recognized when listing a folder. README doesn't
/// scope this further, so this covers the common containers a language
/// learner is likely to have on disk.
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "webm", "mov", "avi"];

/// One row in the Open Video dialog's file list. README defers
/// duration/size metadata to a later iteration if probing turns out heavy
/// (`TODO.md` Vaihe 18) — `size_label` (a cheap `std::fs::metadata` read) is
/// currently the only metadata shown alongside the name; duration would
/// need decoding the file with libmpv/ffprobe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoFileEntry {
    /// Absolute path to the video file — used to open it and to look up a
    /// matching subtitle.
    pub path: PathBuf,
    /// Displayed filename (the path's file name component).
    pub name: String,
    /// Formatted file size, e.g. "340 MB" (see [`format_file_size`]).
    pub size_label: String,
}

/// Lists `folder`'s video files (by extension, see [`VIDEO_EXTENSIONS`]),
/// sorted by filename. Returns an empty list (logging a warning) if
/// `folder` can't be read, so a missing/inaccessible folder doesn't panic
/// the dialog.
pub fn list_video_files(folder: &Path) -> Vec<VideoFileEntry> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(err) => {
            tracing::warn!(?folder, %err, "failed to read video folder");
            return Vec::new();
        }
    };

    let mut files: Vec<VideoFileEntry> = entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_file()))
        .filter(|entry| is_video_file(&entry.path()))
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_string_lossy().into_owned();
            let size_label = entry
                .metadata()
                .map(|metadata| format_file_size(metadata.len()))
                .unwrap_or_default();
            Some(VideoFileEntry {
                path,
                name,
                size_label,
            })
        })
        .collect();
    files.sort_by(|a, b| a.name.cmp(&b.name));
    files
}

/// Whether `path`'s extension matches one of [`VIDEO_EXTENSIONS`]
/// (case-insensitive).
fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            VIDEO_EXTENSIONS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        })
}

/// Formats a byte count as a human-readable size label ("340 MB", "2 KB",
/// "2.1 GB"), matching the mock's size column
/// (`sketch/design_reference.dc.html#2a`).
pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GB {
        format!("{:.1} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.0} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes / KB)
    } else {
        format!("{bytes:.0} B")
    }
}

/// Looks for a same-stem `.srt` file next to `video_path` (README: "attempts
/// to auto-match a same-name subtitle file"), e.g. `der_anruf.mp4` →
/// `der_anruf.srt`. Returns `None` if `video_path` has no parent/stem or no
/// matching file exists on disk.
pub fn matching_subtitle_path(video_path: &Path) -> Option<PathBuf> {
    let parent = video_path.parent()?;
    let stem = video_path.file_stem()?;
    let candidate = parent.join(stem).with_extension("srt");
    candidate.is_file().then_some(candidate)
}

/// Rebuilds the dialog's row model from `entries`, pre-selecting the first
/// one (if any — mirrors the mock's pre-highlighted first row) and opens the
/// modal: sets `open-video-folder-label`/`open-video-rows`/
/// `open-video-selected-index`/`is-open-video-dialog-open`. Called by
/// `main.rs`'s `wire_open_video_dialog` when the top bar's "Open video…"
/// button is clicked.
pub fn open_dialog(window: &AppWindow, folder: &Path, entries: &[VideoFileEntry]) {
    let selected_index = if entries.is_empty() { -1 } else { 0 };
    window.set_open_video_folder_label(folder.display().to_string().into());
    window
        .set_open_video_rows(Rc::new(VecModel::from(dialog_rows(entries, selected_index))).into());
    window.set_open_video_selected_index(selected_index);
    window.set_is_open_video_dialog_open(true);
}

/// Rebuilds the row model with `selected_index` marked current, without
/// touching the folder label or dialog-open state — used when a row is
/// clicked (`main.rs`'s `on_select_open_video_row`).
pub fn mark_selected(window: &AppWindow, entries: &[VideoFileEntry], selected_index: i32) {
    window
        .set_open_video_rows(Rc::new(VecModel::from(dialog_rows(entries, selected_index))).into());
}

/// Maps `entries` into the Slint row model, marking the entry at
/// `selected_index` (if in range) as selected.
fn dialog_rows(entries: &[VideoFileEntry], selected_index: i32) -> Vec<OpenVideoFileRow> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| OpenVideoFileRow {
            name: entry.name.clone().into(),
            size_label: entry.size_label.clone().into(),
            is_selected: usize::try_from(selected_index).ok() == Some(index),
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
    fn test_list_video_files_finds_real_sample_video() {
        // Given: the repo's real test-media/sample fixture folder, which has
        //        one .mp4 alongside two .srt files
        // When:  listing its video files
        // Then:  only the .mp4 is returned, with a non-empty size label
        let files = list_video_files(&fixture_dir());

        assert_eq!(files.len(), 1);
        assert_eq!(files[0].name, "sample.mp4");
        assert!(!files[0].size_label.is_empty());
    }

    #[test]
    fn test_list_video_files_missing_folder_returns_empty() {
        // Given: a folder that doesn't exist
        // When:  listing its video files
        // Then:  an empty list is returned rather than panicking
        let files = list_video_files(Path::new("/no/such/folder/trango-test"));

        assert!(files.is_empty());
    }

    #[test]
    fn test_list_video_files_filters_and_sorts() {
        // Given: a temp folder with mixed video/non-video files, one with an
        //        uppercase extension, created out of alphabetical order
        // When:  listing its video files
        // Then:  only recognized video extensions come back, sorted by name
        let dir = std::env::temp_dir().join("trango-test-list-video-files-filters-and-sorts");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create temp test dir");
        fs::write(dir.join("b_video.mp4"), b"").expect("failed to write fixture file");
        fs::write(dir.join("a_video.MKV"), b"").expect("failed to write fixture file");
        fs::write(dir.join("notes.txt"), b"").expect("failed to write fixture file");

        let files = list_video_files(&dir);

        let names: Vec<&str> = files.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["a_video.MKV", "b_video.mp4"]);

        fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_format_file_size() {
        // Given/When/Then: byte counts across the KB/MB/GB thresholds format
        //                   as the mock's "N MB"-style labels
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(2_048), "2 KB");
        assert_eq!(format_file_size(356_515_840), "340 MB");
        assert_eq!(format_file_size(2_254_857_830), "2.1 GB");
    }

    #[test]
    fn test_matching_subtitle_path_finds_real_sample_srt() {
        // Given: the repo's real sample.mp4/sample.srt fixture pair
        // When:  looking for a matching subtitle
        // Then:  sample.srt is found
        let video_path = fixture_dir().join("sample.mp4");

        assert_eq!(
            matching_subtitle_path(&video_path),
            Some(fixture_dir().join("sample.srt"))
        );
    }

    #[test]
    fn test_matching_subtitle_path_no_match_returns_none() {
        // Given: a video path with no same-stem .srt next to it
        // When:  looking for a matching subtitle
        // Then:  None is returned
        let video_path = fixture_dir().join("does_not_exist.mp4");

        assert_eq!(matching_subtitle_path(&video_path), None);
    }
}
