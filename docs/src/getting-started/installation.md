# Installation

## Debian/Ubuntu: download the .deb

Every release publishes a `.deb` package — grab the latest one from the
[GitHub Releases page](https://github.com/erikRepo/trango/releases) and
install it:

```
sudo apt install ./trango_<version>-1_amd64.deb
```

This pulls in `libmpv2` and the other runtime libraries automatically.
For other platforms, or to build from source, continue below.

## Building from source

### 1. Install the Rust toolchain

If you don't already have Rust, install it via [rustup](https://rustup.rs/).
TrangoPlayer needs Rust 1.97 or newer.

### 2. Install libmpv

TrangoPlayer uses [libmpv](https://github.com/mpv-player/mpv/tree/master/libmpv)
(the mpv media player's playback engine) for video decoding, and needs its
development headers installed to build:

- **Debian/Ubuntu:** `sudo apt install libmpv-dev`
- **Fedora/RHEL:** `sudo dnf install mpv-libs-devel`
- **Arch:** `sudo pacman -S mpv`
- **macOS:** `brew install mpv`

### 3. Get the source and build

```
git clone <repository-url>
cd trango
cargo build --release
```

The first build compiles the whole workspace and takes a few minutes;
later builds are much faster.

### 4. Run it

```
cargo run --release -p trango
```

This opens TrangoPlayer with an empty window — see
[Opening your first video](first-video.md) for what to do next.

## Optional tools

Two features need extra software installed separately, only if you want to
use them — TrangoPlayer works fully without either:

- **Automatic subtitle generation** needs `whisper-cli` and `ffmpeg`, see
  [Generating subtitles automatically](../usage/generating-subtitles.md).
- **Word-by-word analysis** needs a locally running [Ollama](https://ollama.com)
  instance, see [Word-by-word analysis](../usage/word-analysis.md).
