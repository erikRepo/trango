//! Wires Ctrl+Space (`app-window.slint`'s `toggle-audio-capture` callback)
//! to `audio_capture::AudioCapture`'s start/stop (`TODO.md` Vaihe 26) — a
//! plain recorder toggle, writing the system's audio output straight to a
//! WAV file. No rec/stop UI beyond the error message yet (`TODO.md` Vaihe
//! 27 adds a visible control, filename display, and a persisted recording
//! folder — this step just makes Ctrl+Space produce a file at all).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use ::audio_capture::AudioCapture;
use slint::ComponentHandle;

use crate::{config, AppWindow};

/// Wires `window`'s `toggle-audio-capture` callback to `capture`: stops an
/// in-progress capture, or starts one against the monitor source from
/// `config::TrangoConfig::audio_monitor_source` (falling back to
/// `AudioCapture::default_monitor_source`'s `pactl` autodetection), writing
/// to a fresh path from `temp_recording_path`. `capture` is caller-supplied
/// (rather than constructed here) so tests can inject one pointed at a fake
/// `ffmpeg`/`pactl` instead of the real ones. Returns the shared
/// `AudioCapture` handle so callers/tests can inspect/drive it.
///
/// Every start/stop outcome is mirrored into `window`'s
/// `audio-capture-error-message` property: cleared on success, set to the
/// error's message on failure — a missing `pactl`/`ffmpeg` install would
/// otherwise only show up in the (usually invisible) log, making
/// Ctrl+Space look like it silently did nothing.
pub fn wire_audio_capture(window: &AppWindow, capture: AudioCapture) -> Rc<RefCell<AudioCapture>> {
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

        match capture.start(&monitor_source, &temp_recording_path()) {
            Ok(()) => {
                tracing::info!("system audio capture started");
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

/// A process-unique temporary path for a new recording, e.g.
/// `/tmp/trango-recording-<pid>-<counter>.wav` — unique per call within the
/// process (via a monotonic counter, not wall-clock time), mirroring
/// `subtitle::generate`'s `temp_segment_audio_path`/`temp_audio_path`
/// scheme. A placeholder default location: `TODO.md` Vaihe 27 replaces
/// this with a persisted, user-visible recording folder and filename.
fn temp_recording_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "trango-recording-{}-{counter}.wav",
        std::process::id()
    ))
}

#[cfg(test)]
mod tests {
    // `wire_audio_capture`'s callback wiring (start on first press, stop on
    // second) is exercised in crates/app/src/main.rs's
    // test_app_window_properties — Slint's winit backend only allows one
    // AppWindow::new() call per test binary, so every AppWindow-dependent
    // assertion lives in that single test (see its doc comment).
}
