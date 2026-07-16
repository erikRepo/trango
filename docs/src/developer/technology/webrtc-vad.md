# webrtc-vad

Rust bindings to WebRTC's voice-activity-detection (VAD) C library
(`libfvad`, vendored and built via the `cc` crate — no system dependency).
Used by `audio_capture::VadSegmenter` (`crates/audio-capture/src/vad.rs`,
`TODO.md` Vaihe 27) to chop a continuous captured audio stream into
speech segments at pauses.

## Why this over the alternatives

Discussed with the user before choosing (`TODO.md` Vaihe 27):

- **whisper.cpp's own `--vad`/`--vad-model`** support exists in the
  `whisper-cli` build this project already depends on, but it only runs
  *inside* a single transcription call — there's no way to pull segment
  boundaries out ahead of time to slice separate per-segment audio files.
  That's a hard mismatch with Vaihe 28's architecture (one `whisper-cli`
  call per detected segment).
- **A hand-rolled energy-threshold detector** needs no new dependency,
  but reinventing noise-floor calibration is exactly the kind of thing
  `webrtc-vad` already solves, and it's more likely to misfire on the
  background music/ambient audio a language-learning video often has.

`webrtc-vad` operates directly on the 16kHz mono 16-bit PCM samples
`audio_capture::AudioCapture` already captures — no model file to manage.

## How it's used

`Vad::is_voice_segment` classifies one 30ms frame (480 samples at 16kHz)
at a time; `VadMode::Aggressive` is used to lean toward reporting
non-speech on borderline frames, reducing false positives from background
audio. `VadSegmenter` wraps this in a small state machine: a segment
starts on the first voiced frame and closes once trailing silence exceeds
a configurable threshold, with a separate minimum-duration filter to
discard short noise blips.

## Pitfall

The underlying classifier's noise-floor estimate needs a moment to
settle back down after loud audio — in practice it keeps reporting a few
extra frames (~90-120ms) of "voice" right after real speech actually
stops, even though speech onset itself has no such delay. `VadSegmenter`
trims a completed segment's audio back to its last voiced frame, but
callers (and this crate's own tests) should treat a segment's `end_ms` as
accurate to within a couple hundred milliseconds, not to the frame.
