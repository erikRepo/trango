# Generating subtitles automatically

If a video has no subtitle file, the **"Subtitles…"** dialog's
**"Generate subtitles"** button transcribes the video's audio into a
subtitle file automatically, using
[whisper.cpp](https://github.com/ggml-org/whisper.cpp)'s `whisper-cli`
speech-recognition tool. This runs entirely on your own computer — nothing
is uploaded anywhere. The same button works the same way in the Audio
source, for a recorded or opened audio file.

TrangoPlayer doesn't bundle whisper-cli itself, so it needs to be
installed and reachable separately. Note that this is **not** the
`openai-whisper` Python package (whose CLI is `whisper`, with different
flags) — it specifically means whisper.cpp's own `whisper-cli` binary.

**`ffmpeg` is also required for video.** `whisper-cli` only reads a
handful of raw audio formats — not video containers like `.mp4`/`.mkv` —
so TrangoPlayer extracts the video's audio to a temporary file with
`ffmpeg` first. This happens automatically, but `ffmpeg` needs to be
installed and on your `PATH`. It's extremely commonly preinstalled, or a
one-line install: `sudo apt install ffmpeg` / `brew install ffmpeg` / the
[official builds](https://ffmpeg.org/download.html) for Windows.
Generating subtitles for the Audio source's recordings doesn't need
`ffmpeg` — they're already audio, so `whisper-cli` reads them directly.

## Installing whisper-cli

**Linux:** Debian/Ubuntu ship a package:

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

This produces `build/bin/whisper-cli`. Copy or symlink it onto your
`PATH` (e.g. `~/.local/bin`), or point TrangoPlayer at it directly — see
[Settings](settings.md).

**Windows:** whisper.cpp's GitHub Releases page publishes prebuilt
Windows binaries (no build toolchain needed) — download the archive
matching your CPU/GPU setup and extract `whisper-cli.exe` somewhere
convenient. Building from source works the same way as Linux, using
CMake with Visual Studio's toolchain, if you'd rather build it yourself.

## Getting a model

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

Where you put the downloaded file doesn't matter much — TrangoPlayer's
model picker (below) can browse to wherever it ends up, but dropping it
in `~/whisper.cpp/models/` (if you built from source there) or `./models`
(relative to wherever you run TrangoPlayer from) means the picker finds
it automatically without any manual navigation.

## Picking a model in TrangoPlayer

The Subtitles dialog's **"select a whisper model…"** row opens an in-app
folder browser (not your operating system's file picker) scoped to
`.bin`/`.gguf` files. It starts in whichever likely folder it finds
first, but you can navigate anywhere. The pick is remembered across
restarts (see [Settings](settings.md)). The language passed to
`whisper-cli` is inferred automatically from the model's filename
(whisper.cpp's own `.en`-suffix convention) — there's no separate
language setting to configure.

**"Generate subtitles" stays disabled until a model is selected.** If
`whisper-cli` itself can't be found, or a transcription run otherwise
fails, the dialog shows a message explaining what went wrong rather than
a generic failure.
