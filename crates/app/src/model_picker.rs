//! Whisper.cpp model selection (`TODO.md` Vaihe 21.6): an in-app folder
//! browser — reusing `app-window.slint`'s `FileListDialog` chrome, same as
//! the Open Video dialog and the Open Subtitles dialog's translation-link
//! picker — for picking the ggml/gguf model `subtitle::WhisperCliGenerator`
//! runs against, plus best-effort autodiscovery of a sensible starting
//! folder so the user isn't always dropped at their home directory.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::VecModel;

use crate::config::TrangoConfig;
use crate::{AppWindow, FileListRow};

/// Model file extensions recognized when listing a folder — whisper.cpp
/// models are distributed as either format depending on how they were
/// converted/quantized.
const MODEL_EXTENSIONS: &[&str] = &["bin", "gguf"];

/// A selectable model file — one possible [`FolderEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFileEntry {
    /// Absolute path to the model file — passed to `WhisperCliGenerator`.
    pub path: PathBuf,
    /// Displayed name, e.g. `"ggml-medium.bin (multilingual)"` — see
    /// [`display_name`].
    pub name: String,
}

/// One row in the model picker's listing: either something that navigates
/// the dialog to a different folder (`Up`/`Folder`), or a model file that
/// can be selected. Mirrors `open_media_dialog::FolderEntry`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FolderEntry {
    /// The listed folder's parent — shown first, only when a parent exists.
    Up(PathBuf),
    /// A subfolder of the listed folder.
    Folder {
        /// Absolute path to navigate to on click.
        path: PathBuf,
        /// Displayed folder name.
        name: String,
    },
    /// A model file.
    Model(ModelFileEntry),
}

/// Lists `folder`'s contents as dialog rows: an `Up` entry first (if
/// `folder` has a parent), then subfolders sorted by name, then model files
/// (by extension, see [`MODEL_EXTENSIONS`]) sorted by name. Returns just the
/// `Up` entry (or nothing) if `folder`'s contents can't be read (logging a
/// warning), so a missing/inaccessible folder doesn't panic the dialog.
pub fn list_folder_entries(folder: &Path) -> Vec<FolderEntry> {
    let mut entries = Vec::new();
    if let Some(parent) = folder.parent() {
        entries.push(FolderEntry::Up(parent.to_path_buf()));
    }

    let dir_entries = match fs::read_dir(folder) {
        Ok(dir_entries) => dir_entries,
        Err(err) => {
            tracing::warn!(?folder, %err, "failed to read model folder");
            return entries;
        }
    };

    let mut subfolders: Vec<(String, PathBuf)> = Vec::new();
    let mut models: Vec<ModelFileEntry> = Vec::new();
    for dir_entry in dir_entries.filter_map(|entry| entry.ok()) {
        let Ok(file_type) = dir_entry.file_type() else {
            continue;
        };
        let path = dir_entry.path();
        if file_type.is_dir() {
            let Some(name) = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            subfolders.push((name, path));
        } else if file_type.is_file() && is_model_file(&path) {
            models.push(ModelFileEntry {
                name: display_name(&path),
                path,
            });
        }
    }
    subfolders.sort_by(|a, b| a.0.cmp(&b.0));
    models.sort_by(|a, b| a.name.cmp(&b.name));

    entries.extend(
        subfolders
            .into_iter()
            .map(|(name, path)| FolderEntry::Folder { path, name }),
    );
    entries.extend(models.into_iter().map(FolderEntry::Model));
    entries
}

/// Whether `path`'s extension matches one of [`MODEL_EXTENSIONS`]
/// (case-insensitive).
fn is_model_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            MODEL_EXTENSIONS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        })
}

/// Whether `model_path`'s name marks it English-only by whisper.cpp's own
/// naming convention (e.g. `ggml-base.en.bin`) — the `.en` segment sits
/// just before the final extension.
fn is_english_only(model_path: &Path) -> bool {
    model_path
        .file_stem()
        .and_then(|stem| Path::new(stem).extension())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("en"))
}

/// The `-l`/`--language` value to pass `whisper-cli` for `model_path`:
/// `"en"` for an English-only model (`is_english_only`), `"auto"`
/// otherwise — multilingual models need this passed explicitly, since
/// whisper-cli's own default language is `"en"` regardless of which model
/// is loaded.
pub fn language_flag(model_path: &Path) -> &'static str {
    if is_english_only(model_path) {
        "en"
    } else {
        "auto"
    }
}

/// A model file's display name: its filename plus an `"(English)"` /
/// `"(multilingual)"` hint (see [`is_english_only`]), so the picker and the
/// Open Subtitles dialog's model row don't require the user to already
/// know whisper.cpp's naming convention.
pub fn display_name(model_path: &Path) -> String {
    let name = model_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| model_path.display().to_string());
    if is_english_only(model_path) {
        format!("{name} (English)")
    } else {
        format!("{name} (multilingual)")
    }
}

/// Folders whisper.cpp models commonly end up in, given `home` (as
/// `std::env::var_os("HOME")` would give [`default_start_folder`]) — most
/// likely first (a cloned+built whisper.cpp repo's own `models/` folder),
/// down to `./models`, which matches whisper-cli's own default model
/// lookup location relative to the current working directory. Doesn't
/// filter to folders that actually exist — callers decide what to do with
/// missing ones.
fn candidate_model_folders(home: Option<&Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(home) = home {
        candidates.push(home.join("whisper.cpp").join("models"));
        candidates.push(home.join(".cache").join("whisper.cpp").join("models"));
        candidates.push(
            home.join(".local")
                .join("share")
                .join("whisper.cpp")
                .join("models"),
        );
    }
    candidates.push(PathBuf::from("models"));
    candidates
}

/// Picks the model picker's starting folder: `config.whisper_model_folder`
/// if it still exists, else the first [`candidate_model_folders`] entry
/// that both exists and actually contains model files, else the first one
/// that just exists, else the current working directory.
pub fn default_start_folder(config: &TrangoConfig) -> PathBuf {
    if let Some(folder) = config.whisper_model_folder.as_deref() {
        if folder.is_dir() {
            return folder.to_path_buf();
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let candidates = candidate_model_folders(home.as_deref());
    let has_models = |folder: &&PathBuf| {
        folder.is_dir()
            && list_folder_entries(folder)
                .iter()
                .any(|entry| matches!(entry, FolderEntry::Model(_)))
    };
    candidates
        .iter()
        .find(has_models)
        .or_else(|| candidates.iter().find(|folder| folder.is_dir()))
        .cloned()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Rebuilds the picker's row model from `entries`, without pre-selecting
/// any row (unlike the Open Video dialog — there's no "obvious first pick"
/// convention among a folder's models), and opens the modal.
pub fn open_dialog(window: &AppWindow, folder: &Path, entries: &[FolderEntry]) {
    window.set_model_picker_folder_label(folder.display().to_string().into());
    window.set_model_picker_rows(Rc::new(VecModel::from(dialog_rows(entries, -1))).into());
    window.set_model_picker_selected_index(-1);
    window.set_is_model_picker_dialog_open(true);
}

/// Rebuilds the row model with `selected_index` marked current — used when
/// a model row is clicked (`main.rs`'s `on_select_model_picker_row`).
pub fn mark_selected(window: &AppWindow, entries: &[FolderEntry], selected_index: i32) {
    window.set_model_picker_rows(
        Rc::new(VecModel::from(dialog_rows(entries, selected_index))).into(),
    );
}

/// Maps `entries` into the shared `FileListDialog` row model: `Up`/`Folder`
/// entries render as navigable rows (no size label, never selected —
/// clicking them navigates instead, handled in `main.rs`); the `Model`
/// entry at `selected_index` (if any) is marked selected.
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
            FolderEntry::Model(model) => FileListRow {
                name: model.name.clone().into(),
                size_label: "".into(),
                is_selected: usize::try_from(selected_index).ok() == Some(index),
                is_navigable: false,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expect_model(entry: &FolderEntry) -> &ModelFileEntry {
        match entry {
            FolderEntry::Model(model) => model,
            other => panic!("expected a Model entry, got {other:?}"),
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-model-picker-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    #[test]
    fn test_is_english_only_detects_dot_en_suffix() {
        // Given/When/Then: whisper.cpp's ".en" naming convention is
        //                   detected, and a plain (multilingual) name isn't
        assert!(is_english_only(Path::new("/models/ggml-base.en.bin")));
        assert!(!is_english_only(Path::new("/models/ggml-base.bin")));
        assert!(!is_english_only(Path::new("/models/ggml-large-v3.bin")));
    }

    #[test]
    fn test_language_flag() {
        // Given/When/Then: English-only models get "en", others get "auto"
        //                   (whisper-cli's own default is "en" regardless
        //                   of the loaded model, so multilingual models
        //                   need this passed explicitly)
        assert_eq!(language_flag(Path::new("/models/ggml-tiny.en.bin")), "en");
        assert_eq!(language_flag(Path::new("/models/ggml-medium.bin")), "auto");
    }

    #[test]
    fn test_display_name_labels_english_and_multilingual_models() {
        // Given/When/Then: the display name calls out which kind of model
        //                   it is, since whisper.cpp's naming convention
        //                   isn't self-explanatory
        assert_eq!(
            display_name(Path::new("/models/ggml-base.en.bin")),
            "ggml-base.en.bin (English)"
        );
        assert_eq!(
            display_name(Path::new("/models/ggml-medium.bin")),
            "ggml-medium.bin (multilingual)"
        );
    }

    #[test]
    fn test_list_folder_entries_filters_sorts_and_lists_subfolders_first() {
        // Given: a temp folder with mixed model/non-model files (one with
        //        an uppercase extension) and a subfolder, created out of
        //        order
        // When:  listing its contents
        // Then:  Up, then the subfolder, then only recognized model
        //        extensions, each group sorted by name
        let dir = temp_dir("filters-sorts-and-lists-subfolders-first");
        fs::create_dir_all(dir.join("archive")).expect("failed to create temp test dir");
        fs::write(dir.join("ggml-medium.bin"), b"").expect("failed to write fixture file");
        fs::write(dir.join("ggml-base.en.GGUF"), b"").expect("failed to write fixture file");
        fs::write(dir.join("readme.txt"), b"").expect("failed to write fixture file");

        let entries = list_folder_entries(&dir);

        assert!(matches!(entries[0], FolderEntry::Up(_)));
        assert_eq!(
            entries[1],
            FolderEntry::Folder {
                path: dir.join("archive"),
                name: "archive".to_string(),
            }
        );
        assert_eq!(
            expect_model(&entries[2]).path,
            dir.join("ggml-base.en.GGUF")
        );
        assert_eq!(expect_model(&entries[3]).path, dir.join("ggml-medium.bin"));
        assert_eq!(entries.len(), 4);

        fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_list_folder_entries_missing_folder_returns_just_up() {
        // Given: a folder that doesn't exist, but has a parent path
        // When:  listing its contents
        // Then:  only the Up entry comes back, rather than panicking
        let entries = list_folder_entries(Path::new("/no/such/folder/trango-test"));

        assert_eq!(
            entries,
            vec![FolderEntry::Up(PathBuf::from("/no/such/folder"))]
        );
    }

    #[test]
    fn test_candidate_model_folders_includes_home_based_and_relative_paths() {
        // Given/When/Then: with a home dir given, both home-based
        //                   candidates and the relative "models" fallback
        //                   (matching whisper-cli's own default lookup)
        //                   are present
        let candidates = candidate_model_folders(Some(Path::new("/home/alice")));

        assert!(candidates.contains(&PathBuf::from("/home/alice/whisper.cpp/models")));
        assert!(candidates.contains(&PathBuf::from("models")));
    }

    #[test]
    fn test_default_start_folder_prefers_existing_config_folder() {
        // Given: a config pointing at a folder that exists
        // When:  picking the picker's starting folder
        // Then:  that folder wins over any autodiscovery
        let dir = temp_dir("prefers-config-folder");
        let config = TrangoConfig {
            whisper_model_path: None,
            whisper_model_folder: Some(dir.clone()),
            ollama_model: None,
            ollama_target_language: None,
            video_folder: None,
            audio_monitor_source: None,
            audio_recording_folder: None,
        };

        assert_eq!(default_start_folder(&config), dir);

        fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_default_start_folder_falls_back_to_cwd_when_nothing_found() {
        // Given: a config pointing at a folder that no longer exists, and
        //        no real whisper.cpp install to autodiscover in this test
        //        environment's $HOME
        // When:  picking the picker's starting folder
        // Then:  it falls back to the current working directory rather
        //        than panicking or returning a nonexistent path
        let config = TrangoConfig {
            whisper_model_path: None,
            whisper_model_folder: Some(PathBuf::from("/no/such/trango-test-model-folder")),
            ollama_model: None,
            ollama_target_language: None,
            video_folder: None,
            audio_monitor_source: None,
            audio_recording_folder: None,
        };

        let folder = default_start_folder(&config);

        assert!(folder.is_dir());
    }
}
