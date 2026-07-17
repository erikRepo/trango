# System audio capture

`crates/audio-capture`'s `AudioCapture` captures the system's own audio
*output* for the Audio source's Ctrl+Space shortcut (`TODO.md` Vaihe 26) —
the foundation for live subtitle generation without a loaded video (see
`docs/src/developer/specs.md`'s "Audio source: system-audio capture" for
why this captures locally playing audio instead of downloading/scraping
from a source like YouTube).

## How it works

`ffmpeg -f pulse -i <monitor-source> -ar 16000 -ac 1 <output-path>` runs
as a subprocess, the same external-process pattern
`subtitle::WhisperCliGenerator` uses for `whisper-cli`/audio extraction —
no new Cargo dependency. `ffmpeg` writes a single 16kHz mono WAV file
straight to `output-path` — the format `whisper-cli` reads directly,
matching `extract_audio`'s own settings, so a later "Generate subtitles"
pass (`TODO.md` Vaihe 29) needs no separate extraction step. `pactl
get-default-sink` finds the system's default output device; PulseAudio/
PipeWire's `<sink>.monitor` naming convention gives the matching input
source that captures whatever that sink is currently playing, rather
than a microphone. `AudioCapture::stop` asks `ffmpeg` to quit gracefully
by writing `q` to its stdin before falling back to killing it after a
timeout — killing it outright would leave the WAV header's size field
wrong, since `ffmpeg` only finalizes it on a clean exit.

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

## Recording, not live transcription

`crates/app/src/system_audio_capture.rs` toggles `AudioCapture` start/stop
— no per-segment processing happens while a recording is in progress.
Each recording gets a default `<date>_<time>.wav` filename (local time),
written into `config.rs`'s `audio_recording_folder` (the last folder used,
falling back to the current working directory the first time). The
filename is locked while recording; once stopped, editing the Audio
panel's filename field and pressing Enter renames the file on disk
(rejecting anything that isn't a plain filename, so it can't be moved
outside the recording folder). `TODO.md` Vaihe 28 onward add opening/
playing a finished recording and (Vaihe 29) a "Generate subtitles" pass
over it, reusing the same `WhisperCliGenerator` path video files already
use.
