# Testing

## Unit tests

Every crate carries its own `#[cfg(test)] mod tests` alongside the code they
cover (`crates/subtitle`, `crates/playback-state`, `crates/app`), plus
`crates/subtitle/tests/srt_parsing.rs` reading real `.srt` fixtures from
`crates/subtitle/tests/fixtures/`. These stay fast and isolated: no Slint
window, no libmpv core, no real video file.

## E2E: `crates/app/tests/e2e_sentence_navigation.rs`

The first end-to-end test, added in Vaihe 13. Unlike the unit tests above, it
exercises real subtitle parsing and real cue navigation *together*, against
the checked-in fixtures in `test-media/sample/` (see `test-media/README.md`)
instead of hand-built `Cue` literals:

- `parse_srt` reads and parses the real `sample.srt` file from disk.
- The resulting `Vec<Cue>` is loaded into a real `PlayerState`, then walked
  forward with `next_cue()` to the last cue, back with `previous_cue()` to
  the first, and finally `repeat_current_cue()` is called twice — at each
  step the cursor position and the returned `SeekCommand`'s `start`/`end`
  are checked against the fixture's actual timings, not fabricated values.
- A separate test confirms the paired `sample.mp4` video fixture exists on
  disk and is non-empty, tying the video file to the subtitle track this
  suite exercises.

## What this suite deliberately does not cover

- **libmpv rendering/decoding.** The E2E test never opens `sample.mp4`
  through mpv or drives `video_player::VideoPlayer`. `docs/src/architecture/
  video-playback.md` explains why: the render path only exists once Slint
  has a real OpenGL context backed by a real windowing/display connection,
  which isn't guaranteed available where `cargo test` runs, and correctness
  there is about pixels actually appearing on screen — something `cargo
  test` has no way to observe. That verification stays manual (`cargo run
  -p trango -- test-media/sample/sample.mp4`).
- **Pixel-level UI/screenshot testing.** No screenshot comparison against
  `sketch/design_reference.dc.html` is automated at this stage (see `TODO.md`
  Vaihe 22, done manually).
- **The Slint window itself.** `crates/app/src/main.rs`'s own tests already
  cover `AppWindow` property wiring (e.g. `sentence-mode-active`); the E2E
  suite here stays below that layer, at the `subtitle`/`playback-state`
  level.

`scripts/test.sh` runs this suite as part of the normal workspace test run —
no separate invocation is needed.
