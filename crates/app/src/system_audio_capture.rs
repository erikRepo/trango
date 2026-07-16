//! Wires Ctrl+Space (`app-window.slint`'s `toggle-audio-capture` callback,
//! `TODO.md` Vaihe 26) to `audio_capture::AudioCapture`'s start/stop. Only a
//! start/stop signal for now — no capture-state UI yet (`TODO.md` Vaihe 29
//! adds a visible rec/stop control and a proper output filename/folder;
//! this step's output path is just a timestamped file in `output_dir`,
//! good enough to manually verify a WAV file appears).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use ::audio_capture::AudioCapture;
use slint::ComponentHandle;

use crate::{config, AppWindow};

/// Wires `window`'s `toggle-audio-capture` callback to `capture`: stops an
/// in-progress capture, or starts one against the monitor source from
/// `config::TrangoConfig::audio_monitor_source` (falling back to
/// `AudioCapture::default_monitor_source`'s `pactl` autodetection), writing
/// into `output_dir`. `capture` is caller-supplied (rather than constructed
/// here) so tests can inject one pointed at fake `ffmpeg`/`pactl` binaries
/// instead of the real ones. Returns the shared `AudioCapture` so callers/
/// tests can inspect it.
///
/// Every outcome is mirrored into `window`'s `audio-capture-error-message`
/// property: cleared on success, set to the error's message on failure — a
/// missing `pactl`/`ffmpeg` install would otherwise only show up in the
/// (usually invisible) log, making Ctrl+Space look like it silently did
/// nothing.
pub fn wire_audio_capture(
    window: &AppWindow,
    capture: AudioCapture,
    output_dir: PathBuf,
) -> Rc<RefCell<AudioCapture>> {
    let capture = Rc::new(RefCell::new(capture));

    let capture_for_callback = Rc::clone(&capture);
    let window_weak = window.as_weak();
    window.on_toggle_audio_capture(move || {
        let Some(window) = window_weak.upgrade() else {
            return;
        };
        let mut capture = capture_for_callback.borrow_mut();
        if capture.is_recording() {
            match capture.stop() {
                Ok(()) => {
                    tracing::info!("system audio capture stopped");
                    window.set_audio_capture_error_message("".into());
                }
                Err(err) => {
                    tracing::warn!(%err, "failed to stop system audio capture");
                    window.set_audio_capture_error_message(err.to_string().into());
                }
            }
            return;
        }

        let monitor_source = match config::load().audio_monitor_source {
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

        let output_path = capture_output_path(&output_dir);
        match capture.start(&monitor_source, &output_path) {
            Ok(()) => {
                tracing::info!(?output_path, "system audio capture started");
                window.set_audio_capture_error_message("".into());
            }
            Err(err) => {
                tracing::warn!(%err, "failed to start system audio capture");
                window.set_audio_capture_error_message(err.to_string().into());
            }
        }
    });

    capture
}

/// A WAV output path for a new capture inside `dir`:
/// `trango-capture-<unix epoch seconds>.wav` — a placeholder name good
/// enough for this step's manual "does a WAV file appear" test. `TODO.md`
/// Vaihe 29 replaces this with a user-visible filename and a persisted
/// recording folder.
fn capture_output_path(dir: &Path) -> PathBuf {
    let epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    dir.join(format!("trango-capture-{epoch_secs}.wav"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // `wire_audio_capture`'s callback wiring itself (start on first press,
    // stop on second) is exercised in crates/app/src/main.rs's
    // test_app_window_properties — Slint's winit backend only allows one
    // AppWindow::new() call per test binary, so every AppWindow-dependent
    // assertion lives in that single test (see its doc comment).

    #[test]
    fn test_capture_output_path_is_a_wav_file_inside_the_given_directory() {
        // Given: an arbitrary directory
        // When:  computing an output path inside it
        // Then:  it's a .wav file directly inside that directory
        let dir = Path::new("/some/recordings/dir");

        let path = capture_output_path(dir);

        assert_eq!(path.parent(), Some(dir));
        assert_eq!(path.extension().and_then(|ext| ext.to_str()), Some("wav"));
    }
}
