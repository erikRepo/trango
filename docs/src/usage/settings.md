# Settings

TrangoPlayer has no separate settings screen — the few things it
remembers are saved automatically as you use the app, and picked up
again the next time it starts.

## What's remembered

Stored in a small config file (`$XDG_CONFIG_HOME/trango/config.toml`,
falling back to `$HOME/.config/trango/config.toml`), written whenever
you change one of these:

- The folder the last video you opened was in — so "Open video…" starts
  there next time.
- The whisper model you last picked (see
  [Generating subtitles automatically](generating-subtitles.md)).
- The Ollama model and target language you last picked (see
  [Word-by-word analysis](word-analysis.md)).

An `audio_monitor_source` setting isn't picked from the UI — add it to
`config.toml` by hand if needed. "No video" mode's Ctrl+Space recording
(see [Playback modes](playback-modes.md)) autodetects which PulseAudio/
PipeWire "monitor" source captures your system's audio output by asking
`pactl` for the default sink. If that picks the wrong device — for
example, you have multiple audio outputs and want to record from a
non-default one — set `audio_monitor_source` to the exact source name
(check `pactl list sources short`) to skip autodetection entirely.

If this file is missing or unreadable, TrangoPlayer just starts with
nothing remembered rather than failing to open — losing a remembered
setting is far less disruptive than the app refusing to start.

## Locating external tools

`whisper-cli` and `ffmpeg` (see
[Generating subtitles automatically](generating-subtitles.md)) are
found on your `PATH` by default. If you've installed either somewhere
that isn't on `PATH`, point TrangoPlayer at it directly with an
environment variable:

- `TRANGO_WHISPER_CLI_PATH` — path to the `whisper-cli` binary.
- `TRANGO_FFMPEG_PATH` — path to the `ffmpeg` binary, used both for
  subtitle generation and for "No video" mode's system audio capture.

These are environment variables rather than settings inside the app
because they're one-time system install paths that rarely change, unlike
the model choices above, which you might switch often.

## Debug logging

The `--debug` command-line flag turns on detailed logging, mainly useful
for diagnosing [word analysis](word-analysis.md) issues. See
[Keyboard shortcuts](keyboard-shortcuts.md#debugging) for details.
