# Architecture

TrangoPlayer is a Cargo workspace split into four crates so that most of
the business logic — subtitle parsing, cue navigation, Ollama's
HTTP/JSON handling — is testable without pulling in the heavier Slint/
libmpv dependencies.

- **[Crate structure](crates.md)** — the four crates, what each one
  owns, and how they depend on each other.
- **[Video playback](video-playback.md)** — how libmpv's render API is
  embedded inside the Slint window without mpv opening a window of its
  own.
- **[Testing](testing.md)** — what's covered by fast unit tests, what
  the end-to-end suite exercises, and what's deliberately left to
  manual testing.
