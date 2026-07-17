//! Open dialog (Vaihe 18, generalized to audio in Vaihe 28): an in-app modal
//! listing a folder's video or audio files and subfolders — no OS-native
//! file/folder picker, per SPEC.md's `#2a` mock — with in-dialog navigation
//! (an "‥ Up" row plus clicking a subfolder) and same-stem `.srt`
//! auto-matching once a file is opened. Shared by the top bar's single
//! "Open…" button, which lists videos or audio recordings depending on the
//! active `MediaSource` (`main.rs`'s `wire_open_media_dialog`).

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::VecModel;

use crate::{AppWindow, FileListRow};

/// Video file extensions recognized when listing a folder in
/// [`MediaKind::Video`]. SPEC.md doesn't scope this further, so this covers
/// the common containers a language learner is likely to have on disk.
const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "webm", "mov", "avi"];

/// Audio file extensions recognized when listing a folder in
/// [`MediaKind::Audio`] — just `.wav`, the format `audio_capture::AudioCapture`
/// writes recordings in (`TODO.md` Vaihe 26/27).
const AUDIO_EXTENSIONS: &[&str] = &["wav"];

/// Which kind of media the dialog is listing — chooses the extension filter
/// ([`MediaKind::extensions`]) and, in `main.rs`'s `wire_open_media_dialog`,
/// which config folder is used as the default listing location and which
/// one gets updated once a file is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    /// Video files, opened via the Video source's "Open…" button.
    Video,
    /// Audio recordings (`.wav`), opened via the Audio source's "Open…"
    /// button.
    Audio,
}

impl MediaKind {
    /// The recognized file extensions for this kind (case-insensitive
    /// comparison happens in [`is_media_file`]).
    fn extensions(self) -> &'static [&'static str] {
        match self {
            MediaKind::Video => VIDEO_EXTENSIONS,
            MediaKind::Audio => AUDIO_EXTENSIONS,
        }
    }
}

/// A selectable, openable media file — one possible [`FolderEntry`]. SPEC.md
/// defers duration/size metadata to a later iteration if probing turns out
/// heavy (`TODO.md` Vaihe 18) — `size_label` (a cheap `std::fs::metadata`
/// read) is currently the only metadata shown alongside the name; duration
/// would need decoding the file with libmpv/ffprobe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFileEntry {
    /// Absolute path to the file — used to open it and to look up a
    /// matching subtitle.
    pub path: PathBuf,
    /// Displayed filename (the path's file name component).
    pub name: String,
    /// Formatted file size, e.g. "340 MB" (see [`format_file_size`]).
    pub size_label: String,
}

/// One row in the Open dialog's listing: either something that navigates
/// the dialog to a different folder (`Up`/`Folder`), or a media file that
/// can be selected and opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderEntry {
    /// The listed folder's parent — shown first, only when a parent exists
    /// (i.e. not at the filesystem root).
    Up(PathBuf),
    /// A subfolder of the listed folder.
    Folder {
        /// Absolute path to navigate to on click.
        path: PathBuf,
        /// Displayed folder name.
        name: String,
    },
    /// A media file matching the dialog's current [`MediaKind`].
    File(MediaFileEntry),
}

/// Lists `folder`'s contents as dialog rows: an `Up` entry first (if
/// `folder` has a parent), then subfolders sorted by name, then media files
/// matching `kind` (by extension, see [`MediaKind::extensions`]) sorted by
/// name. Returns just the `Up` entry (or nothing) if `folder`'s contents
/// can't be read (logging a warning), so a missing/inaccessible folder
/// doesn't panic the dialog.
pub fn list_folder_entries(folder: &Path, kind: MediaKind) -> Vec<FolderEntry> {
    let mut entries = Vec::new();
    if let Some(parent) = folder.parent() {
        entries.push(FolderEntry::Up(parent.to_path_buf()));
    }

    let dir_entries = match fs::read_dir(folder) {
        Ok(dir_entries) => dir_entries,
        Err(err) => {
            tracing::warn!(?folder, %err, "failed to read media folder");
            return entries;
        }
    };

    let mut subfolders: Vec<(String, PathBuf)> = Vec::new();
    let mut files: Vec<MediaFileEntry> = Vec::new();
    for dir_entry in dir_entries.filter_map(|entry| entry.ok()) {
        let Ok(file_type) = dir_entry.file_type() else {
            continue;
        };
        let path = dir_entry.path();
        let Some(name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if file_type.is_dir() {
            subfolders.push((name, path));
        } else if file_type.is_file() && is_media_file(&path, kind) {
            let size_label = dir_entry
                .metadata()
                .map(|metadata| format_file_size(metadata.len()))
                .unwrap_or_default();
            files.push(MediaFileEntry {
                path,
                name,
                size_label,
            });
        }
    }
    subfolders.sort_by(|a, b| a.0.cmp(&b.0));
    files.sort_by(|a, b| a.name.cmp(&b.name));

    entries.extend(
        subfolders
            .into_iter()
            .map(|(name, path)| FolderEntry::Folder { path, name }),
    );
    entries.extend(files.into_iter().map(FolderEntry::File));
    entries
}

/// Whether `path`'s extension matches one of `kind`'s recognized extensions
/// (case-insensitive).
fn is_media_file(path: &Path, kind: MediaKind) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            kind.extensions()
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

/// Looks for a same-stem `.srt` file next to `media_path` (SPEC.md: "attempts
/// to auto-match a same-name subtitle file"), e.g. `der_anruf.mp4` →
/// `der_anruf.srt`. Returns `None` if `media_path` has no parent/stem or no
/// matching file exists on disk.
pub fn matching_subtitle_path(media_path: &Path) -> Option<PathBuf> {
    let parent = media_path.parent()?;
    let stem = media_path.file_stem()?;
    let candidate = parent.join(stem).with_extension("srt");
    candidate.is_file().then_some(candidate)
}

/// Rebuilds the dialog's row model from `entries`, pre-selecting the first
/// file entry (if any — mirrors the mock's pre-highlighted first row) and
/// opens the modal: sets `open-media-folder-label`/`open-media-rows`/
/// `open-media-selected-index`/`is-open-media-dialog-open`. Called by
/// `main.rs`'s `wire_open_media_dialog` both when the top bar's "Open…"
/// button is clicked and when a row navigates to a different folder
/// (setting `is-open-media-dialog-open` again in the latter case is
/// harmless — it's already `true`). Doesn't touch the dialog's title —
/// callers set `open-media-dialog-title` themselves, since it depends on
/// `MediaKind`, not on `entries`.
pub fn open_dialog(window: &AppWindow, folder: &Path, entries: &[FolderEntry]) {
    let selected_index = first_file_index(entries);
    window.set_open_media_folder_label(folder.display().to_string().into());
    window
        .set_open_media_rows(Rc::new(VecModel::from(dialog_rows(entries, selected_index))).into());
    window.set_open_media_selected_index(selected_index);
    window.set_is_open_media_dialog_open(true);
}

/// The index of `entries`' first `File` entry, or `-1` if none.
fn first_file_index(entries: &[FolderEntry]) -> i32 {
    entries
        .iter()
        .position(|entry| matches!(entry, FolderEntry::File(_)))
        .and_then(|index| i32::try_from(index).ok())
        .unwrap_or(-1)
}

/// Rebuilds the row model with `selected_index` marked current, without
/// touching the folder label or dialog-open state — used when a file row
/// is clicked (`main.rs`'s `on_select_open_media_row`).
pub fn mark_selected(window: &AppWindow, entries: &[FolderEntry], selected_index: i32) {
    window
        .set_open_media_rows(Rc::new(VecModel::from(dialog_rows(entries, selected_index))).into());
}

/// Maps `entries` into the Slint row model. `Up`/`Folder` entries render as
/// navigable rows (no size label, never selected — clicking them navigates
/// instead, handled in `main.rs`); the `File` entry at `selected_index` (if
/// any) is marked selected.
fn dialog_rows(entries: &[FolderEntry], selected_index: i32) -> Vec<FileListRow> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| match entry {
            FolderEntry::Up(_) => FileListRow {
                name: "⬆ .. (up)".into(),
                size_label: "".into(),
                is_selected: false,
                is_navigable: true,
            },
            FolderEntry::Folder { name, .. } => FileListRow {
                name: format!("{name}/").into(),
                size_label: "".into(),
                is_selected: false,
                is_navigable: true,
            },
            FolderEntry::File(file) => FileListRow {
                name: file.name.clone().into(),
                size_label: file.size_label.clone().into(),
                is_selected: usize::try_from(selected_index).ok() == Some(index),
                is_navigable: false,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-media/sample")
    }

    /// Extracts a `File` entry's `MediaFileEntry`, panicking otherwise —
    /// keeps the folder-navigation tests focused on the entries that matter
    /// for that assertion.
    fn expect_file(entry: &FolderEntry) -> &MediaFileEntry {
        match entry {
            FolderEntry::File(file) => file,
            other => panic!("expected a File entry, got {other:?}"),
        }
    }

    #[test]
    fn test_list_folder_entries_finds_real_sample_video() {
        // Given: the repo's real test-media/sample fixture folder, which has
        //        one .mp4 alongside two .srt files and no subfolders
        // When:  listing its contents in Video mode
        // Then:  an Up entry (the folder has a parent) is followed by the
        //        single video entry, with a non-empty size label
        let entries = list_folder_entries(&fixture_dir(), MediaKind::Video);

        assert!(matches!(entries[0], FolderEntry::Up(_)));
        assert_eq!(entries.len(), 2);
        let file = expect_file(&entries[1]);
        assert_eq!(file.name, "sample.mp4");
        assert!(!file.size_label.is_empty());
    }

    #[test]
    fn test_list_folder_entries_audio_mode_ignores_video_files() {
        // Given: the same fixture folder, which has no .wav files
        // When:  listing its contents in Audio mode
        // Then:  only the Up entry comes back — the .mp4/.srt files don't
        //        match the audio extension filter
        let entries = list_folder_entries(&fixture_dir(), MediaKind::Audio);

        assert_eq!(
            entries,
            vec![FolderEntry::Up(
                fixture_dir().parent().unwrap().to_path_buf()
            )]
        );
    }

    #[test]
    fn test_list_folder_entries_audio_mode_finds_wav_files() {
        // Given: a temp folder with a .wav recording and an unrelated .mp4
        // When:  listing its contents in Audio mode
        // Then:  only the .wav file is listed
        let dir = std::env::temp_dir().join("trango-test-list-folder-entries-audio-mode");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create temp test dir");
        fs::write(dir.join("2026-07-17_18-42-05.wav"), b"").expect("failed to write fixture file");
        fs::write(dir.join("unrelated.mp4"), b"").expect("failed to write fixture file");

        let entries = list_folder_entries(&dir, MediaKind::Audio);

        assert_eq!(entries.len(), 2);
        assert_eq!(expect_file(&entries[1]).name, "2026-07-17_18-42-05.wav");

        fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_list_folder_entries_missing_folder_returns_just_up() {
        // Given: a folder that doesn't exist, but has a parent path
        // When:  listing its contents
        // Then:  only the Up entry comes back, rather than panicking
        let entries =
            list_folder_entries(Path::new("/no/such/folder/trango-test"), MediaKind::Video);

        assert_eq!(
            entries,
            vec![FolderEntry::Up(PathBuf::from("/no/such/folder"))]
        );
    }

    #[test]
    fn test_list_folder_entries_filters_sorts_and_lists_subfolders_first() {
        // Given: a temp folder with mixed video/non-video files (one with an
        //        uppercase extension) and a subfolder, created out of order
        // When:  listing its contents in Video mode
        // Then:  Up, then the subfolder, then only recognized video
        //        extensions, each group sorted by name
        let dir = std::env::temp_dir()
            .join("trango-test-list-folder-entries-filters-sorts-and-lists-subfolders-first");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("clips")).expect("failed to create temp test dir");
        fs::write(dir.join("b_video.mp4"), b"").expect("failed to write fixture file");
        fs::write(dir.join("a_video.MKV"), b"").expect("failed to write fixture file");
        fs::write(dir.join("notes.txt"), b"").expect("failed to write fixture file");

        let entries = list_folder_entries(&dir, MediaKind::Video);

        assert!(matches!(entries[0], FolderEntry::Up(_)));
        assert_eq!(
            entries[1],
            FolderEntry::Folder {
                path: dir.join("clips"),
                name: "clips".to_string(),
            }
        );
        assert_eq!(expect_file(&entries[2]).name, "a_video.MKV");
        assert_eq!(expect_file(&entries[3]).name, "b_video.mp4");
        assert_eq!(entries.len(), 4);

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

    #[test]
    fn test_first_file_index() {
        // Given: an entry list with navigable rows before the first file
        // When:  finding the first file's index
        // Then:  it's the file's position, not 0
        let entries = vec![
            FolderEntry::Up(PathBuf::from("/videos")),
            FolderEntry::Folder {
                path: PathBuf::from("/videos/clips"),
                name: "clips".to_string(),
            },
            FolderEntry::File(MediaFileEntry {
                path: PathBuf::from("/videos/a.mp4"),
                name: "a.mp4".to_string(),
                size_label: "1 MB".to_string(),
            }),
        ];

        assert_eq!(first_file_index(&entries), 2);
    }

    #[test]
    fn test_first_file_index_with_no_files_is_negative_one() {
        // Given: an entry list with no File entries
        // When:  finding the first file's index
        // Then:  -1, meaning "nothing selectable"
        let entries = vec![FolderEntry::Up(PathBuf::from("/videos"))];

        assert_eq!(first_file_index(&entries), -1);
    }
}
