# System audio capture

`crates/audio-capture`'s `AudioCapture` records the system's own audio
*output* to a WAV file for "No video" mode's Ctrl+Space shortcut
(`TODO.md` Vaihe 26) — the first building block toward live subtitle
generation without a loaded video (see `docs/src/developer/specs.md`'s
"No video mode: system-audio capture" for why this captures locally
playing audio instead of downloading/scraping from a source like
YouTube).

## How it works

`ffmpeg -f pulse -i <monitor-source>` runs as a subprocess, the same
external-process pattern `subtitle::WhisperCliGenerator` uses for
`whisper-cli`/audio extraction — no new Cargo dependency. `pactl
get-default-sink` finds the system's default output device; PulseAudio/
PipeWire's `<sink>.monitor` naming convention gives the matching input
source that captures whatever that sink is currently playing, rather
than a microphone. `AudioCapture::stop` asks `ffmpeg` to quit gracefully
by writing `q` to its stdin (finalizing the WAV header correctly) before
falling back to killing it after a timeout.

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
