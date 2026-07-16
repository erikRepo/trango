//! System audio capture (`TODO.md` Vaihe 26): streams the PC's outgoing
//! audio (e.g. a video playing in a browser) as raw PCM via an external
//! `ffmpeg -f pulse` subprocess, mirroring `subtitle::WhisperCliGenerator`'s
//! external-process pattern — no new Cargo dependency. A background thread
//! segments the stream into speech chunks at pauses (`TODO.md` Vaihe 27),
//! ready for per-segment `whisper-cli` transcription (`TODO.md` Vaihe 28) —
//! no audio ever touches disk in this crate. Linux/PulseAudio-PipeWire only
//! for now; see `docs/src/developer/architecture/system-audio-capture.md`.
//! No Slint/libmpv dependency.

mod capture;
mod error;
mod vad;

pub use capture::AudioCapture;
pub use error::AudioCaptureError;
pub use vad::{SpeechSegment, VadSegmenter};
