//! VAD (Voice Activity Detection) ggml model selection: an in-app folder
//! browser, reusing `app-window.slint`'s `FileListDialog` chrome — same
//! pattern as `niqud_model_picker.rs`, but scoped to `.bin` files (the
//! format whisper.cpp's own `convert-silero-vad-to-ggml.py` produces) and
//! with whisper-style `whisper.cpp/models`-family autodiscovery, since a
//! VAD model is commonly kept alongside a whisper model rather than
//! somewhere with "exactly one well-known" location the way niqud's is.

use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::VecModel;

use crate::config::TrangoConfig;
use crate::{AppWindow, FileListRow};

/// A selectable VAD model file — one possible [`FolderEntry`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelFileEntry {
    /// Absolute path to the model file — saved to
    /// `config::TrangoConfig::vad_model_path`.
    pub path: PathBuf,
    /// Displayed name — just the filename, no naming convention worth
    /// calling out (unlike whisper models' `.en`/multilingual suffix).
    pub name: String,
}

/// One row in the VAD model picker's listing. Mirrors
/// `model_picker::FolderEntry`/`niqud_model_picker::FolderEntry`.
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
    /// A `.bin` VAD model file.
    Model(ModelFileEntry),
}

/// Lists `folder`'s contents as dialog rows: `Up` (if a parent exists),
/// then subfolders sorted by name, then `.bin` files sorted by name.
/// Returns just the `Up` entry (or nothing) if `folder`'s contents can't
/// be read, so a missing/inaccessible folder doesn't panic the dialog.
pub fn list_folder_entries(folder: &Path) -> Vec<FolderEntry> {
    let mut entries = Vec::new();
    if let Some(parent) = folder.parent() {
        entries.push(FolderEntry::Up(parent.to_path_buf()));
    }

    let dir_entries = match fs::read_dir(folder) {
        Ok(dir_entries) => dir_entries,
        Err(err) => {
            tracing::warn!(?folder, %err, "failed to read VAD model folder");
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
        } else if file_type.is_file() && is_vad_model_file(&path) {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            models.push(ModelFileEntry { name, path });
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

/// Whether `path` has a (case-insensitive) `.bin` extension.
fn is_vad_model_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("bin"))
}

/// Folders a VAD ggml model commonly ends up in, given `home` (as
/// `std::env::var_os("HOME")` would give [`default_start_folder`]) —
/// the same `whisper.cpp/models`-family locations `model_picker.rs`'s
/// (private) `candidate_model_folders` checks, since a VAD model is
/// typically downloaded/converted right alongside a whisper model.
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

/// The VAD model picker's starting folder: the configured model's parent
/// folder if it still exists, else the first [`candidate_model_folders`]
/// entry that exists, else the current working directory.
pub fn default_start_folder(config: &TrangoConfig) -> PathBuf {
    if let Some(folder) = config
        .vad_model_path
        .as_deref()
        .and_then(Path::parent)
        .filter(|folder| folder.is_dir())
    {
        return folder.to_path_buf();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    candidate_model_folders(home.as_deref())
        .into_iter()
        .find(|folder| folder.is_dir())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Rebuilds the picker's row model from `entries` and opens the modal.
pub fn open_dialog(window: &AppWindow, folder: &Path, entries: &[FolderEntry]) {
    window.set_vad_model_picker_folder_label(folder.display().to_string().into());
    window.set_vad_model_picker_rows(Rc::new(VecModel::from(dialog_rows(entries, -1))).into());
    window.set_vad_model_picker_selected_index(-1);
    window.set_is_vad_model_picker_dialog_open(true);
}

/// Rebuilds the row model with `selected_index` marked current — used
/// when a model row is clicked.
pub fn mark_selected(window: &AppWindow, entries: &[FolderEntry], selected_index: i32) {
    window.set_vad_model_picker_rows(
        Rc::new(VecModel::from(dialog_rows(entries, selected_index))).into(),
    );
}

/// Maps `entries` into the shared `FileListDialog` row model. Mirrors
/// `model_picker::dialog_rows`/`niqud_model_picker::dialog_rows`.
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
        let dir = std::env::temp_dir().join(format!("trango-test-vad-model-picker-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    #[test]
    fn test_list_folder_entries_filters_sorts_and_lists_subfolders_first() {
        // Given: a temp folder with a .bin file (uppercase extension), an
        //        unrelated file, and a subfolder, created out of order
        // When:  listing its contents
        // Then:  Up, then the subfolder, then only the .bin file
        let dir = temp_dir("filters-sorts-and-lists-subfolders-first");
        fs::create_dir_all(dir.join("archive")).expect("failed to create temp test dir");
        fs::write(dir.join("silero-vad-v6.2.0-ggml.BIN"), b"")
            .expect("failed to write fixture file");
        fs::write(dir.join("README.md"), b"").expect("failed to write fixture file");

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
            dir.join("silero-vad-v6.2.0-ggml.BIN")
        );
        assert_eq!(entries.len(), 3);

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
    fn test_default_start_folder_prefers_configured_models_parent() {
        // Given: a config whose vad_model_path's parent folder exists
        // When:  picking the picker's starting folder
        // Then:  that folder wins
        let dir = temp_dir("prefers-configured-parent");
        let config = TrangoConfig {
            vad_model_path: Some(dir.join("silero-vad-v6.2.0-ggml.bin")),
            ..Default::default()
        };

        assert_eq!(default_start_folder(&config), dir);

        fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_default_start_folder_falls_back_to_cwd_when_nothing_configured() {
        // Given: a config with no vad_model_path set, and no real
        //        whisper.cpp install to autodiscover in this test
        //        environment's $HOME
        // When:  picking the picker's starting folder
        // Then:  it falls back to the current working directory rather
        //        than panicking
        let config = TrangoConfig::default();

        let folder = default_start_folder(&config);

        assert!(folder.is_dir());
    }
}
