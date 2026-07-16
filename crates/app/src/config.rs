//! Persists trango's user-chosen settings (currently just the selected
//! whisper.cpp model — `TODO.md` Vaihe 21.6) to a small TOML file, so a
//! choice made in the UI survives a restart instead of resetting every run.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// trango's persisted settings. All fields are optional so a missing or
/// partially-filled config file (or none at all, on first run) still loads
/// as a valid, mostly-empty `TrangoConfig` rather than an error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrangoConfig {
    /// The whisper.cpp model file last picked in the model-selection dialog
    /// (`model_picker::open_dialog`), reused as the default for "Generate
    /// subtitles" on the next run.
    pub whisper_model_path: Option<PathBuf>,
    /// The folder the model-selection dialog was last browsing, so
    /// reopening it doesn't always restart from a freshly autodiscovered
    /// default.
    pub whisper_model_folder: Option<PathBuf>,
    /// The Ollama model last picked for word-by-word sentence analysis
    /// (`TODO.md` Vaihe 24), reused as the default for the Ctrl+A popup
    /// and "Analyze all sentences" on the next run.
    pub ollama_model: Option<String>,
    /// The target language last typed into the Open Subtitles dialog's
    /// language field (`TODO.md` Vaihe 24.1) — what word analyses are
    /// translated/pronounced into. `None` (rather than an empty string)
    /// means "never edited yet", so `word_analysis::DEFAULT_TARGET_LANGUAGE`
    /// is used as the starting value instead of an empty field.
    pub ollama_target_language: Option<String>,
}

/// Resolves the config directory from `xdg_config_home`/`home` (as
/// `std::env::var("XDG_CONFIG_HOME")`/`std::env::var("HOME")` would give
/// `config_path` below) rather than reading the environment directly, so
/// this can be tested without touching real process-wide environment
/// variables. Follows the XDG base directory spec: `$XDG_CONFIG_HOME` if
/// set, else `$HOME/.config`. Returns `None` if neither is available
/// (defensive — every real environment trango runs in has `HOME` set).
fn config_dir_from_env(xdg_config_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    if let Some(xdg_config_home) = xdg_config_home.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(xdg_config_home));
    }
    home.filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(".config"))
}

/// The config file's path: `<config dir>/trango/config.toml`, where
/// `<config dir>` is `$XDG_CONFIG_HOME` or `$HOME/.config`. Returns `None`
/// if neither environment variable is set.
pub fn config_path() -> Option<PathBuf> {
    let xdg_config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let home = std::env::var("HOME").ok();
    config_dir_from_env(xdg_config_home.as_deref(), home.as_deref())
        .map(|dir| dir.join("trango").join("config.toml"))
}

/// Reads and parses `path` into a `TrangoConfig`. Returns `TrangoConfig::default()`
/// — not an error — if the file doesn't exist, can't be read, or doesn't
/// parse as valid TOML, logging a warning in the latter two cases: a
/// missing or corrupt config file shouldn't stop trango from starting, it
/// should just start with nothing pre-selected.
fn load_from(path: &Path) -> TrangoConfig {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return TrangoConfig::default(),
        Err(err) => {
            tracing::warn!(?path, %err, "failed to read trango config file");
            return TrangoConfig::default();
        }
    };
    match toml::from_str(&contents) {
        Ok(config) => config,
        Err(err) => {
            tracing::warn!(?path, %err, "failed to parse trango config file");
            TrangoConfig::default()
        }
    }
}

/// Loads the config from [`config_path`], or `TrangoConfig::default()` if
/// there's no config directory to look in (see `config_path`'s doc
/// comment).
pub fn load() -> TrangoConfig {
    match config_path() {
        Some(path) => load_from(&path),
        None => TrangoConfig::default(),
    }
}

/// Serializes `config` to `path` as TOML, creating its parent directory if
/// needed. Errors are logged, not propagated — losing a persisted setting
/// shouldn't interrupt whatever the user was doing when it happened (e.g.
/// picking a model).
fn save_to(path: &Path, config: &TrangoConfig) {
    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            tracing::warn!(?parent, %err, "failed to create trango config directory");
            return;
        }
    }
    let contents = match toml::to_string_pretty(config) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(%err, "failed to serialize trango config");
            return;
        }
    };
    if let Err(err) = std::fs::write(path, contents) {
        tracing::warn!(?path, %err, "failed to write trango config file");
    }
}

/// Saves `config` to [`config_path`], a no-op if there's no config
/// directory to save it in (see `config_path`'s doc comment).
pub fn save(config: &TrangoConfig) {
    if let Some(path) = config_path() {
        save_to(&path, config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_dir_from_env_prefers_xdg_config_home() {
        // Given/When/Then: XDG_CONFIG_HOME set alongside HOME
        // Then:  XDG_CONFIG_HOME wins, used as-is (no ".config" appended)
        assert_eq!(
            config_dir_from_env(Some("/custom/config"), Some("/home/alice")),
            Some(PathBuf::from("/custom/config"))
        );
    }

    #[test]
    fn test_config_dir_from_env_falls_back_to_home_dot_config() {
        // Given: no XDG_CONFIG_HOME, only HOME
        // When:  resolving the config dir
        // Then:  it's HOME/.config
        assert_eq!(
            config_dir_from_env(None, Some("/home/alice")),
            Some(PathBuf::from("/home/alice/.config"))
        );
    }

    #[test]
    fn test_config_dir_from_env_none_when_neither_set() {
        // Given/When/Then: neither variable available
        assert_eq!(config_dir_from_env(None, None), None);
    }

    #[test]
    fn test_load_from_missing_file_returns_default() {
        // Given: a path that doesn't exist
        // When:  loading it
        // Then:  a default (empty) config comes back, not an error
        let config = load_from(Path::new("/no/such/trango-config-test/config.toml"));

        assert_eq!(config, TrangoConfig::default());
    }

    #[test]
    fn test_save_then_load_round_trips() {
        // Given: a config with both fields set, saved to a temp file
        // When:  loading it back
        // Then:  the loaded config matches what was saved
        let dir = std::env::temp_dir().join("trango-test-config-round-trip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("config.toml");
        let config = TrangoConfig {
            whisper_model_path: Some(PathBuf::from("/models/ggml-medium.bin")),
            whisper_model_folder: Some(PathBuf::from("/models")),
            ollama_model: Some("llama3.1:8b".to_string()),
            ollama_target_language: Some("Finnish".to_string()),
        };

        save_to(&path, &config);
        let loaded = load_from(&path);

        assert_eq!(loaded, config);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_load_from_corrupt_file_returns_default() {
        // Given: a file that exists but isn't valid TOML
        // When:  loading it
        // Then:  a default (empty) config comes back, not a panic
        let dir = std::env::temp_dir().join("trango-test-config-corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        let path = dir.join("config.toml");
        std::fs::write(&path, b"this is not { valid toml").expect("failed to write fixture file");

        let config = load_from(&path);

        assert_eq!(config, TrangoConfig::default());

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }
}
