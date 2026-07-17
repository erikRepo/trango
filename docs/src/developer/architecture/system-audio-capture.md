# System audio capture

`crates/audio-capture`'s `AudioCapture` captures the system's own audio
*output* for the Audio source's Ctrl+Space shortcut (`TODO.md` Vaihe 26) —
the foundation for live subtitle generation without a loaded video (see
`docs/src/developer/specs.md`'s "Audio source: system-audio capture" for
why this captures locally playing audio instead of downloading/scraping
from a source like YouTube).

## How it works

`ffmpeg -f pulse -i <monitor-source> -f s16le pipe:1` runs as a
subprocess, the same external-process pattern
`subtitle::WhisperCliGenerator` uses for `whisper-cli`/audio extraction —
no new Cargo dependency. Rather than writing a WAV file, `ffmpeg` streams
raw 16kHz mono PCM to its stdout; a background thread reads it, decodes
it into samples, and feeds a fresh `VadSegmenter` (see "Speech
segmentation" below) — no audio ever touches disk in this crate. `pactl
get-default-sink` finds the system's default output device; PulseAudio/
PipeWire's `<sink>.monitor` naming convention gives the matching input
source that captures whatever that sink is currently playing, rather
than a microphone. `AudioCapture::stop` asks `ffmpeg` to quit gracefully
by writing `q` to its stdin before falling back to killing it after a
timeout; either way, `ffmpeg` exiting closes the pipe, letting the
reader thread finish (flushing any still-in-progress segment) before
`stop` returns.

## Linux/PulseAudio-PipeWire only

`pactl` and `ffmpeg -f pulse` have no equivalent wired up on Windows or
macOS — this is an explicit exception to trango's usual
`std::process::Command`-based approach working identically on both
platforms (CLAUDE.md), since audio *capture* is far more
platform-specific than running an external CLI tool. Windows (WASAPI
loopback) and macOS (Core Audio, which has no built-in loopback device)
would each need their own capture mechanism entirely. Not implemented;
revisit if trango needs to support those platforms.

Autodetection can also be wrong for setups with multiple audio outputs —
`crates/app/src/config.rs`'s `audio_monitor_source` overrides it with an
exact source name (see `docs/src/usage/settings.md`).

## Speech segmentation and live transcription

`audio_capture::VadSegmenter` (`TODO.md` Vaihe 27) chops the captured PCM
stream into speech segments at pauses, so each one can be transcribed as
a sentence-sized chunk instead of a fixed sliding window. It lives
entirely inside `AudioCapture::start`'s own reader thread — `webrtc_vad`'s
`Vad` wraps a raw FFI pointer and isn't `Send`, so it's created fresh on
that thread rather than passed in, and only the resulting `SpeechSegment`s
(themselves plain owned data) ever cross a thread boundary, via the
`on_segment` callback. See `docs/src/developer/technology/webrtc-vad.md`
for why `webrtc-vad` was chosen over whisper.cpp's own `--vad` support or
a hand-rolled detector.

`crates/app/src/system_audio_capture.rs` (`TODO.md` Vaihe 28) spawns one
background thread per completed segment, each running
`subtitle::WhisperCliGenerator::transcribe_segment` — writing the
segment's samples to a temporary WAV, running `whisper-cli` against it
directly (no `ffmpeg` extraction needed, the samples are already in the
right format), and deleting both the WAV and the resulting `.srt` once
parsed, so nothing but cues in memory survives a segment's transcription.
Results are sent through a channel that `crates/app/src/live_transcription.rs`'s
`LiveTranscription` drains on a polling `slint::Timer`, appending them to
`PlayerState` and refreshing the sentence list — the channel indirection
exists because `PlayerState` lives behind a non-`Send` `Rc<RefCell<_>>`,
so appending must happen back on the UI thread rather than from the
transcription threads themselves.
