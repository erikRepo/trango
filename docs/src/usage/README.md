# Usage

## Running trango

```
cargo run -p trango -- <path/to/video> [path/to/subs.srt] [path/to/subs.translation.srt]
```

All three arguments are optional. Without a video path, the window opens
empty — pick one with the top bar's "Open video…" button. Without a
subtitle path, trango looks for a same-stem `.srt` next to the video
(`video.mp4` → `video.srt`) once one is opened, or you can link/generate
one from the "Open subtitles…" dialog. The third argument, if given, is
merged in as a translation track alongside the first subtitle file.

## Generating subtitles with whisper-cli

If a video has no subtitle file, the Open Subtitles dialog's "Generate
subtitles" button runs [whisper.cpp](https://github.com/ggml-org/whisper.cpp)'s
`whisper-cli` as an external process to transcribe it — trango doesn't
bundle or link against whisper.cpp itself (see `docs/src/specs/`'s
"Subtitle generation: whisper-cli as an external process" for why), so
`whisper-cli` needs to be installed and reachable separately. This is
**not** the `openai-whisper` Python package (its CLI is `whisper`, with
different flags) — it specifically means whisper.cpp's own C++ binary.

**`ffmpeg` is also required.** `whisper-cli` only reads a handful of raw
audio formats (`flac`/`mp3`/`ogg`/`wav`) — not video containers like
`.mp4`/`.mkv` at all, and it exits *successfully* even when it silently
failed to read an unsupported file, which looked like a mysterious
"no subtitle file was found" error before this was diagnosed. trango
works around this by extracting the video's audio to a temporary WAV file
with `ffmpeg` before handing it to `whisper-cli` — this is automatic, but
`ffmpeg` needs to be installed and on `PATH` (or pointed at via
`TRANGO_FFMPEG_PATH`, see below). It's extremely commonly preinstalled or
a one-line install (`sudo apt install ffmpeg` / `brew install ffmpeg` /
the [official builds](https://ffmpeg.org/download.html) for Windows).

### Installing whisper-cli

**Linux:** Debian/Ubuntu ship a `whisper.cpp` package with `apt`:

```
sudo apt install whisper.cpp
```

This installs `whisper-cli` straight onto `PATH` — no build step needed.
If your distro doesn't package it (or you want a newer version), build
from source instead — no unusual dependencies, just a C++ toolchain and
CMake:

```
git clone https://github.com/ggml-org/whisper.cpp
cd whisper.cpp
cmake -B build
cmake --build build --config Release
```

This produces `build/bin/whisper-cli`. Either copy/symlink it onto your
`PATH` (e.g. `~/.local/bin`), or point trango at it directly with
`TRANGO_WHISPER_CLI_PATH` (see below) — no need to move it.

**Windows:** the project's GitHub Releases page publishes prebuilt
Windows binaries (no build toolchain needed) — download the archive
matching your CPU/GPU setup and extract `whisper-cli.exe` somewhere
convenient. Building from source works the same way as Linux, using
CMake with Visual Studio's toolchain, if you'd rather build it yourself.

Either way, `Command::new` (what `WhisperCliGenerator` uses to run it)
works identically on both platforms — the only difference is the
binary's name/location, covered by `TRANGO_WHISPER_CLI_PATH` below.

### Getting a model

`whisper-cli` also needs a ggml/gguf model file, downloaded separately —
whisper.cpp's repo includes a `models/download-ggml-model.sh` script for
fetching one (e.g. `./models/download-ggml-model.sh medium` for a
mid-sized multilingual model). Larger models transcribe more accurately
but take longer and use more memory.

**Model size matters a lot for anything other than English.** Whisper's
smaller models (`tiny`/`base`/`small`) are trained on mostly English data,
so quality for lower-resource languages — Hebrew is a good example —
drops noticeably compared to English. For non-English language-learning
videos, prefer `medium` or `large-v3` (both multilingual — don't use an
`.en`-suffixed model, those are English-only and won't transcribe
anything else). English-only content can still use the smaller, faster
`base.en`/`small.en` models fine.

Where you put the downloaded file doesn't matter much — trango's model
picker (see below) can browse to wherever it ends up, but dropping it in
`~/whisper.cpp/models/` (if you built from source there) or `./models`
(relative to wherever you run `cargo run -p trango` from, matching
whisper-cli's own default lookup) means the picker finds it automatically
without any manual navigation.

### Configuring trango

**The whisper-cli and ffmpeg binaries** are configured through
environment variables, since they're one-time system install paths that
rarely change:

- `TRANGO_WHISPER_CLI_PATH`: path (or bare name) of the `whisper-cli`
  binary to run. Defaults to `"whisper-cli"`, resolved via `PATH`.
- `TRANGO_FFMPEG_PATH`: path (or bare name) of the `ffmpeg` binary used
  for audio extraction. Defaults to `"ffmpeg"`, resolved via `PATH`.

**The model**, on the other hand, is picked from inside the app — the
Open Subtitles dialog's "select a whisper model…" row opens an in-app
folder browser (no OS-native file picker, same as the rest of trango)
scoped to `.bin`/`.gguf` files. It starts in whichever likely folder it
finds first (a few common whisper.cpp install locations, then `./models`),
but you can navigate anywhere. The pick is remembered across restarts —
see `docs/src/specs/`'s "Model selection" for exactly where and why
(`$XDG_CONFIG_HOME/trango/config.toml`, falling back to
`$HOME/.config/trango/config.toml`). The language passed to whisper-cli is
inferred automatically from the model's filename (whisper.cpp's own
`.en`-suffix convention) — no separate language setting.

"Generate subtitles" stays disabled until a model is selected. If
`whisper-cli` itself can't be found (or the run otherwise fails),
"Generate subtitles" ends in the dialog's `Error` state with a message
explaining what went wrong, rather than a generic failure.
