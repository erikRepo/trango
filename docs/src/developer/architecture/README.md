# Architecture

TrangoPlayer is a Cargo workspace split into five crates so that most of
the business logic — subtitle parsing, cue navigation, Ollama's
HTTP/JSON handling, system audio capture — is testable without pulling
in the heavier Slint/libmpv dependencies.

- **[Crate structure](crates.md)** — the five crates, what each one
  owns, and how they depend on each other.
- **[Video playback](video-playback.md)** — how libmpv's render API is
  embedded inside the Slint window without mpv opening a window of its
  own.
- **[System audio capture](system-audio-capture.md)** — how "No video"
  mode records the system's own audio output, and why it's Linux/
  PulseAudio-PipeWire only for now.
- **[Testing](testing.md)** — what's covered by fast unit tests, what
  the end-to-end suite exercises, and what's deliberately left to
  manual testing.
