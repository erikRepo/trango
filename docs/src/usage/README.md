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

### Installing whisper-cli

**Linux:** whisper.cpp doesn't currently ship prebuilt Linux binaries, so
build it from source — no unusual dependencies, just a C++ toolchain and
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
binary's name/location, which the two environment variables below cover.

### Getting a model

`whisper-cli` also needs a ggml/gguf model file, downloaded separately —
whisper.cpp's repo includes a `models/download-ggml-model.sh` script for
fetching one (e.g. `./models/download-ggml-model.sh base.en` for a small
English-only model). Larger models transcribe more accurately but take
longer and use more memory; `base.en` or `small.en` are reasonable
starting points for English-language learning videos.

### Configuring trango

Two environment variables configure how trango finds `whisper-cli` and
its model — both optional:

- `TRANGO_WHISPER_CLI_PATH`: path (or bare name) of the `whisper-cli`
  binary to run. Defaults to `"whisper-cli"`, resolved via `PATH`.
- `TRANGO_WHISPER_MODEL_PATH`: path to the ggml/gguf model file to use.
  If unset, `whisper-cli` falls back to its own default model lookup
  (which requires a model to already be present at the location it
  expects — see whisper.cpp's own docs).

```
TRANGO_WHISPER_CLI_PATH=/opt/whisper.cpp/build/bin/whisper-cli \
TRANGO_WHISPER_MODEL_PATH=/opt/whisper.cpp/models/ggml-base.en.bin \
cargo run -p trango -- video.mp4
```

If `whisper-cli` can't be found, "Generate subtitles" ends in the
dialog's `Error` state with a message explaining that, rather than a
generic failure.
