//! System audio capture (`TODO.md` Vaihe 26): records the PC's outgoing
//! audio (e.g. a video playing in a browser) to a WAV file via an external
//! `ffmpeg -f pulse` subprocess, mirroring `subtitle::WhisperCliGenerator`'s
//! external-process pattern — no new Cargo dependency. Linux/PulseAudio-
//! PipeWire only for now; see
//! `docs/src/developer/architecture/system-audio-capture.md`. No Slint/
//! libmpv dependency.

mod capture;
mod error;

pub use capture::AudioCapture;
pub use error::AudioCaptureError;
