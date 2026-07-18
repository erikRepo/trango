//! Settings screen (top bar's gear icon): shows and edits every
//! `config::TrangoConfig` field (`docs/src/usage/settings.md`) in one
//! place. This module only populates the dialog's display properties on
//! open — the edits themselves are handled by `crates/app/src/main.rs`'s
//! `wire_settings_dialog` (video folder, audio monitor source, audio
//! recording folder) and by the pre-existing `wire_model_picker`/
//! `wire_ollama_model_picker`/`wire_ollama_target_language` handlers,
//! reused as-is since the Settings dialog's model/language rows forward
//! straight to the same top-level callbacks those already wire
//! (`app-window.slint`'s SettingsDialog instantiation).

use crate::system_audio_capture::default_recording_folder;
use crate::{config, AppWindow};

/// Populates `window`'s settings-* display properties from `config` and
/// opens the dialog. `audio_recording_folder` falls back through
/// [`default_recording_folder`] (the same resolution the Audio panel's own
/// "Saving to:" label and a new recording use), so the field shows the
/// folder a recording would actually be written to even before any
/// recording has been made or a value has ever been saved to config.toml.
pub fn open_dialog(window: &AppWindow, config: &config::TrangoConfig) {
    window.set_settings_config_path(config_path_label().into());
    window.set_settings_video_folder(path_label(config.video_folder.as_deref()).into());

    window.set_settings_audio_monitor_source(
        config
            .audio_monitor_source
            .clone()
            .unwrap_or_default()
            .into(),
    );

    let recording_folder = default_recording_folder(config);
    window.set_settings_audio_recording_folder(recording_folder.display().to_string().into());
    window.set_settings_audio_recording_folder_exists(recording_folder.is_dir());

    window.set_settings_niqud_model_path(path_label(config.niqud_model_path.as_deref()).into());

    window.set_is_settings_dialog_open(true);
}

/// [`config::config_path`] formatted for display, or an explanatory
/// placeholder if it's unavailable (no `XDG_CONFIG_HOME`/`HOME` — see
/// `config::config_path`'s doc comment).
fn config_path_label() -> String {
    config::config_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "unavailable (neither XDG_CONFIG_HOME nor HOME is set)".to_string())
}

/// `path`'s display string, or `""` if unset — kept as a free function
/// (rather than inlined) so the "unset" case has one obvious spelling
/// wherever this dialog shows a path.
fn path_label(path: Option<&std::path::Path>) -> String {
    path.map(|path| path.display().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn test_path_label_formats_set_and_unset_paths() {
        // Given/When/Then: a set path displays as-is; None displays empty
        assert_eq!(path_label(Some(&PathBuf::from("/videos"))), "/videos");
        assert_eq!(path_label(None), "");
    }
}
