//! Wires Ctrl+Space and the Audio source's rec/stop button
//! (`app-window.slint`'s `toggle-audio-capture` callback) to
//! `audio_capture::AudioCapture`'s start/stop (`TODO.md` Vaihe 26/27) — a
//! plain recorder toggle, writing the system's audio output straight to a
//! WAV file whose name and folder are shown and managed in the Audio
//! panel.

use std::cell::RefCell;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use ::audio_capture::AudioCapture;
use chrono::{DateTime, Local};
use slint::ComponentHandle;

use crate::{config, AppWindow};

/// Wires `window`'s `toggle-audio-capture` and `rename-audio-recording-file`
/// callbacks to `capture`: stops an in-progress capture, or starts one
/// against the monitor source from `config::TrangoConfig::audio_monitor_source`
/// (falling back to `AudioCapture::default_monitor_source`'s `pactl`
/// autodetection), writing to a fresh, timestamped filename inside
/// `config::TrangoConfig::audio_recording_folder` (or the current working
/// directory on first use). `capture` is caller-supplied (rather than
/// constructed here) so tests can inject one pointed at a fake
/// `ffmpeg`/`pactl` instead of the real ones. Returns the shared
/// `AudioCapture` handle so callers/tests can inspect/drive it.
///
/// Every start/stop outcome is mirrored into `window`'s
/// `audio-capture-error-message` property: cleared on success, set to the
/// error's message on failure — a missing `pactl`/`ffmpeg` install would
/// otherwise only show up in the (usually invisible) log, making
/// Ctrl+Space look like it silently did nothing. `is-audio-recording` and
/// `audio-recording-filename` mirror the current state for the Audio
/// panel's rec/stop button and filename field; the filename is locked
/// (`enabled: !is-audio-recording` in Slint) for the duration of a
/// recording and renamable afterwards via `rename-audio-recording-file`.
pub fn wire_audio_capture(window: &AppWindow, capture: AudioCapture) -> Rc<RefCell<AudioCapture>> {
    let capture = Rc::new(RefCell::new(capture));
    let recording_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));

    let capture_for_toggle = Rc::clone(&capture);
    let recording_path_for_toggle = Rc::clone(&recording_path);
    let toggle_window_weak = window.as_weak();
    window.on_toggle_audio_capture(move || {
        let Some(window) = toggle_window_weak.upgrade() else {
            return;
        };
        let mut capture = capture_for_toggle.borrow_mut();
        if capture.is_recording() {
            match capture.stop() {
                Ok(()) => {
                    tracing::info!("system audio capture stopped");
                    window.set_audio_capture_error_message("".into());
                    window.set_is_audio_recording(false);
                }
                Err(err) => {
                    tracing::warn!(%err, "failed to stop system audio capture");
                    window.set_audio_capture_error_message(err.to_string().into());
                }
            }
            return;
        }

        let mut config = config::load();
        let monitor_source = match config.audio_monitor_source.clone() {
            Some(source) => source,
            None => match capture.default_monitor_source() {
                Ok(source) => source,
                Err(err) => {
                    tracing::warn!(%err, "failed to detect default monitor source");
                    window.set_audio_capture_error_message(err.to_string().into());
                    return;
                }
            },
        };

        let folder = default_recording_folder(&config);
        let filename = default_recording_filename(Local::now());
        let output_path = folder.join(&filename);

        match capture.start(&monitor_source, &output_path) {
            Ok(()) => {
                tracing::info!(?output_path, "system audio capture started");
                window.set_audio_capture_error_message("".into());
                window.set_is_audio_recording(true);
                window.set_audio_recording_filename(filename.into());
                *recording_path_for_toggle.borrow_mut() = Some(output_path);

                config.audio_recording_folder = Some(folder);
                config::save(&config);
            }
            Err(err) => {
                tracing::warn!(%err, "failed to start system audio capture");
                window.set_audio_capture_error_message(err.to_string().into());
            }
        }
    });

    let capture_for_rename = Rc::clone(&capture);
    let recording_path_for_rename = Rc::clone(&recording_path);
    let rename_window_weak = window.as_weak();
    window.on_rename_audio_recording_file(move |new_name| {
        let Some(window) = rename_window_weak.upgrade() else {
            return;
        };
        // Renaming while a recording is in progress isn't allowed (`TODO.md`
        // Vaihe 27) — Slint's `enabled: !is-audio-recording` on the LineEdit
        // is advisory only, so this is checked again here.
        if capture_for_rename.borrow().is_recording() {
            return;
        }
        let Some(old_path) = recording_path_for_rename.borrow().clone() else {
            return;
        };

        match rename_recording(&old_path, &new_name) {
            Ok(new_path) => {
                window.set_audio_capture_error_message("".into());
                window.set_audio_recording_filename(file_name_string(&new_path).into());
                *recording_path_for_rename.borrow_mut() = Some(new_path);
            }
            Err(err) => {
                tracing::warn!(%err, "failed to rename recording file");
                window.set_audio_capture_error_message(err.to_string().into());
                // Displayed name reverts to the file's still-current name,
                // since the rename didn't actually happen.
                window.set_audio_recording_filename(file_name_string(&old_path).into());
            }
        }
    });

    capture
}

/// The folder a new recording is written to: the last-used recording folder
/// from `config` (`TrangoConfig::audio_recording_folder`), or the current
/// working directory on first use — mirrors `main.rs`'s
/// `default_video_folder` fallback chain, minus the CLI-arg case (there's
/// no CLI-provided path for a fresh recording).
fn default_recording_folder(config: &config::TrangoConfig) -> PathBuf {
    config.audio_recording_folder.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|err| {
            tracing::warn!(%err, "failed to read current directory; falling back to \".\"");
            PathBuf::from(".")
        })
    })
}

/// The default filename for a new recording: `now` formatted as
/// `<date>_<time>.wav`, e.g. `2026-07-17_18-42-05.wav` (`TODO.md` Vaihe 27).
/// Takes `now` explicitly rather than calling `Local::now()` internally so
/// tests can assert on a fixed timestamp.
fn default_recording_filename(now: DateTime<Local>) -> String {
    now.format("%Y-%m-%d_%H-%M-%S.wav").to_string()
}

/// Renames the file at `old_path` to `new_name`, kept in the same folder.
/// Rejects an empty `new_name` or one that isn't a single plain path
/// component (e.g. containing `/` or `..`), so a pasted or malformed value
/// can't move the file outside its recording folder. Returns the resulting
/// path on success.
fn rename_recording(old_path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let new_name = new_name.trim();
    let mut components = Path::new(new_name).components();
    let is_bare_filename = !new_name.is_empty()
        && matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none();
    if !is_bare_filename {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Recording filename must be a plain filename, not a path.",
        ));
    }

    let folder = old_path.parent().unwrap_or_else(|| Path::new("."));
    let new_path = folder.join(new_name);
    std::fs::rename(old_path, &new_path)?;
    Ok(new_path)
}

/// `path`'s file name as a `String`, or an empty string if it has none
/// (shouldn't happen for the recording paths this module builds, but keeps
/// the property-setting call sites above infallible).
fn file_name_string(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_default_recording_filename_formats_local_timestamp() {
        // Given: a fixed local date/time
        // When:  building the default recording filename
        // Then:  it's "<date>_<time>.wav"
        let now = Local.with_ymd_and_hms(2026, 7, 17, 18, 42, 5).unwrap();

        assert_eq!(default_recording_filename(now), "2026-07-17_18-42-05.wav");
    }

    #[test]
    fn test_default_recording_folder_prefers_config_value() {
        // Given: a config with a saved recording folder
        // When:  resolving the default recording folder
        // Then:  the saved folder wins
        let config = config::TrangoConfig {
            audio_recording_folder: Some(PathBuf::from("/saved/recordings")),
            ..Default::default()
        };

        assert_eq!(
            default_recording_folder(&config),
            PathBuf::from("/saved/recordings")
        );
    }

    #[test]
    fn test_default_recording_folder_falls_back_to_cwd() {
        // Given: a config with no saved recording folder
        // When:  resolving the default recording folder
        // Then:  it's the current working directory, not empty/panicking
        let folder = default_recording_folder(&config::TrangoConfig::default());

        assert!(folder.is_dir());
    }

    #[test]
    fn test_rename_recording_renames_file_in_place() {
        // Given: an existing recording file
        // When:  renaming it to a new plain filename
        // Then:  the file moves to the new name in the same folder, and the
        //        returned path reflects that
        let dir = std::env::temp_dir().join("trango-test-rename-recording-happy-path");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        let old_path = dir.join("2026-07-17_18-42-05.wav");
        std::fs::write(&old_path, b"fake wav content").expect("failed to write fixture file");

        let new_path = rename_recording(&old_path, "der_anruf.wav").unwrap();

        assert_eq!(new_path, dir.join("der_anruf.wav"));
        assert!(!old_path.exists());
        assert_eq!(std::fs::read(&new_path).unwrap(), b"fake wav content");

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_rename_recording_rejects_path_traversal() {
        // Given: an existing recording file
        // When:  renaming it to a value containing a path separator
        // Then:  an error comes back and the original file is untouched
        let dir = std::env::temp_dir().join("trango-test-rename-recording-traversal");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        let old_path = dir.join("2026-07-17_18-42-05.wav");
        std::fs::write(&old_path, b"fake wav content").expect("failed to write fixture file");

        let result = rename_recording(&old_path, "../escaped.wav");

        assert!(result.is_err());
        assert!(old_path.exists());

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_rename_recording_rejects_empty_name() {
        // Given: an existing recording file
        // When:  renaming it to an empty/whitespace-only value
        // Then:  an error comes back and the original file is untouched
        let dir = std::env::temp_dir().join("trango-test-rename-recording-empty");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        let old_path = dir.join("2026-07-17_18-42-05.wav");
        std::fs::write(&old_path, b"fake wav content").expect("failed to write fixture file");

        let result = rename_recording(&old_path, "   ");

        assert!(result.is_err());
        assert!(old_path.exists());

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    // `wire_audio_capture`'s callback wiring (start/stop/rename via the
    // AppWindow) is exercised in crates/app/src/main.rs's
    // test_app_window_properties — Slint's winit backend only allows one
    // AppWindow::new() call per test binary, so every AppWindow-dependent
    // assertion lives in that single test (see its doc comment).
}
