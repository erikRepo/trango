# Settings

The gear icon in the top bar opens the Settings screen, showing and
editing everything TrangoPlayer remembers between runs in one place.

## What's remembered

Stored in a small config file (`$XDG_CONFIG_HOME/trango/config.toml`,
falling back to `$HOME/.config/trango/config.toml`), written whenever
you change one of these:

- The folder the last video you opened was in — so "Open" starts there
  next time you're in the Video source.
- The whisper model you last picked (see
  [Generating subtitles automatically](generating-subtitles.md)).
- The Ollama model and target language you last picked (see
  [Word-by-word analysis](word-analysis.md)).
- The folder your last Audio-source recording was opened from or saved
  to — new recordings, and "Open" in the Audio source, both default
  there too (see [Playback modes](playback-modes.md)). The Audio
  source's placeholder panel always shows this folder ("Saving to:
  …"), and starting a recording into a folder that no longer exists
  shows an error instead of silently failing.

Every field in the Settings screen is editable, and saves immediately —
no separate "Save" button:

- **Video folder**, **audio recording folder** — plain text fields;
  type a path and it's used from then on.
- **Whisper model**, **Ollama model**, **Hebrew niqud model** —
  clicking the current value (or "select a model…") opens a picker
  dialog rather than typing a path, guaranteeing a valid, absolute path.
- **Word analysis target language** — the same field as the Subtitles
  dialog's language box; editing it in either place updates the other.
- **Audio monitor source** — overrides the Audio source's Ctrl+Space
  recording's autodetection of which PulseAudio/PipeWire "monitor"
  source captures your system's audio output (normally asks `pactl`
  for the default sink). Set this if autodetection picks the wrong
  device — for example, you have multiple audio outputs and want to
  record from a non-default one — to the exact source name (check
  `pactl list sources short`) to skip autodetection entirely. Empty
  means "keep autodetecting".
- **Hebrew niqud model (.onnx)** — points at a downloaded niqud
  diacritization model file, used to correct Hebrew pronunciation
  guides in [word analysis](word-analysis.md#hebrew-pronunciation).
  Not set means Hebrew falls back to Ollama's own (less accurate)
  guess. A new pick only takes effect after restarting TrangoPlayer —
  the dialog says so once you've picked one.

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
- `TRANGO_FFMPEG_PATH` — path to the `ffmpeg` binary, used for subtitle
  generation, the Audio source's system audio capture, and
  [practice audio](practice-audio.md).
- `TRANGO_ESPEAK_PATH` — path to the `espeak-ng` binary, used for
  [practice audio](practice-audio.md)'s per-word translation TTS.

These are environment variables rather than settings inside the app
because they're one-time system install paths that rarely change, unlike
the model choices above, which you might switch often.

## Debug logging

The `--debug` command-line flag turns on detailed logging, mainly useful
for diagnosing [word analysis](word-analysis.md) issues. See
[Keyboard shortcuts](keyboard-shortcuts.md#debugging) for details.
