# TrangoPlayer

📖 **[Full documentation](https://erikrepo.github.io/trango/)** — installation, usage, and developer guide.

A desktop video player for language learners, built with Rust, Slint, and
libmpv. Open a video with a subtitle file and TrangoPlayer plays it
normally, or in a **sentence-by-sentence** mode driven entirely by subtitle
timing: jump between lines, replay the one you're on, and reveal a
translated line alongside the original. If a video has no subtitles yet,
TrangoPlayer can generate them automatically — and look up a word-by-word
translation for whatever sentence is on screen — all running locally on
your own machine.

## Quick start

```
cargo build --release
cargo run --release -p trango -- <path/to/video>
```

See the [installation guide](https://erikrepo.github.io/trango/getting-started/installation.html)
for prerequisites (Rust, libmpv) and
[opening your first video](https://erikrepo.github.io/trango/getting-started/first-video.html)
for the rest.

## Repository guide

- [docs/](docs/) — the mdBook source for the documentation linked above.
- [CLAUDE.md](CLAUDE.md) — development workflow (TDD, scripts, git
  workflow, Rust conventions).
- [TODO.md](TODO.md) — the step-by-step development roadmap.
- [releasenotes.md](releasenotes.md) — per-version changes (Keep a
  Changelog format).
- [SPEC.md](SPEC.md) — the original functional handoff spec (screens,
  interactions, state).
- [STYLE.md](STYLE.md) — the original visual design reference and design
  tokens.

## License and Contributions

Copyright (C) 2026 Erik Repo. All rights reserved.

- This code is public for viewing only. No permission is granted to copy,
  modify, or distribute this software in other projects.
- Contributions (Pull Requests) are welcome! By submitting a Pull Request,
  you agree that your contributions will be licensed under the same terms
  as the rest of the project.

See [LICENSE](LICENSE) for the full text.
