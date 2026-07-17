//! Wires Ctrl+Space (`app-window.slint`'s `toggle-audio-capture` callback)
//! to `audio_capture::AudioCapture`'s start/stop (`TODO.md` Vaihe 26) and,
//! per completed `audio_capture::SpeechSegment`, a background
//! `subtitle::WhisperCliGenerator::transcribe_segment` call whose resulting
//! cues feed `live_transcription::LiveTranscription` (`TODO.md` Vaihe 28) —
//! the Audio source's sentence list grows live as speech is captured. Still
//! no capture-state UI beyond the error message (`TODO.md` Vaihe 29 adds a
//! visible rec/stop control).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ::audio_capture::{AudioCapture, SpeechSegment};
use playback_state::PlayerState;
use slint::ComponentHandle;
use subtitle::{Cue, WhisperCliGenerator};

use crate::live_transcription::LiveTranscription;
use crate::{config, whisper_cli_generator, AppWindow};

/// Wires `window`'s `toggle-audio-capture` callback to `capture`: stops an
/// in-progress capture, or starts one against the monitor source from
/// `config::TrangoConfig::audio_monitor_source` (falling back to
/// `AudioCapture::default_monitor_source`'s `pactl` autodetection).
/// Starting requires a whisper model to already be selected
/// (`selected_model`, see `wire_model_picker`) — each completed speech
/// segment is transcribed with it on its own background thread
/// (`spawn_segment_transcription`), and the resulting cues are appended to
/// `state` via `live_transcription`. `capture` is caller-supplied (rather
/// than constructed here) so tests can inject one pointed at fake
/// `ffmpeg`/`pactl` binaries instead of the real ones. Returns the shared
/// `AudioCapture` and `LiveTranscription` handles so callers/tests can
/// inspect/drive them.
///
/// Every start/stop outcome is mirrored into `window`'s
/// `audio-capture-error-message` property: cleared on success, set to the
/// error's message on failure — a missing `pactl`/`ffmpeg` install, or no
/// whisper model selected yet, would otherwise only show up in the
/// (usually invisible) log, making Ctrl+Space look like it silently did
/// nothing.
pub fn wire_audio_capture(
    window: &AppWindow,
    capture: AudioCapture,
    state: Rc<RefCell<PlayerState>>,
    selected_model: Rc<RefCell<Option<std::path::PathBuf>>>,
) -> (Rc<RefCell<AudioCapture>>, Rc<LiveTranscription>) {
    let capture = Rc::new(RefCell::new(capture));
    let live_transcription = LiveTranscription::start(window, state);

    let capture_for_callback = Rc::clone(&capture);
    let live_transcription_for_callback = Rc::clone(&live_transcription);
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

        let Some(model_path) = selected_model.borrow().clone() else {
            tracing::warn!("audio capture requested with no whisper model selected");
            window.set_audio_capture_error_message("Select a whisper model first.".into());
            return;
        };

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

        let generator = Arc::new(whisper_cli_generator(model_path));
        let cues_tx = live_transcription_for_callback.sender();
        let on_segment = move |segment: SpeechSegment| {
            spawn_segment_transcription(Arc::clone(&generator), segment, cues_tx.clone());
        };

        match capture.start(&monitor_source, on_segment) {
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

    (capture, live_transcription)
}

/// Spawns a background thread transcribing one completed `segment` via
/// `generator` (`TODO.md` Vaihe 28) — this runs off `AudioCapture`'s own
/// capture-reading thread (which calls the `on_segment` callback this
/// builds), so a slow `whisper-cli` call never stalls audio capture, and
/// concurrent segments transcribe in parallel rather than queueing behind
/// each other. Non-empty results are sent through `cues_tx` for
/// `live_transcription::LiveTranscription` to drain onto the UI thread;
/// failures are logged rather than surfaced anywhere user-visible yet
/// (`TODO.md` Vaihe 30 covers making transcription progress/lag visible).
fn spawn_segment_transcription(
    generator: Arc<WhisperCliGenerator>,
    segment: SpeechSegment,
    cues_tx: std::sync::mpsc::Sender<Vec<Cue>>,
) {
    thread::spawn(move || {
        let segment_start = Duration::from_millis(segment.start_ms);
        match generator.transcribe_segment(&segment.samples, segment_start) {
            Ok(cues) if !cues.is_empty() => {
                let _ = cues_tx.send(cues);
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(%err, start_ms = segment.start_ms, "segment transcription failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    // `wire_audio_capture`'s callback wiring (start on first press, stop on
    // second, plus live transcription end to end) is exercised in
    // crates/app/src/main.rs's test_app_window_properties — Slint's winit
    // backend only allows one AppWindow::new() call per test binary, so
    // every AppWindow-dependent assertion lives in that single test (see
    // its doc comment).
}
