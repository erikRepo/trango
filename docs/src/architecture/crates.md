# Crate structure

trango is a Cargo workspace (`[workspace]` in the root `Cargo.toml`) with
three members:

## `crates/subtitle` (library, package `subtitle`)

Holds the `Cue` data model (`index`, `start`, `end`, `text`) and
`SubtitleError` (see `docs/src/technology/thiserror.md`). `Cue::new`
validates that `start < end`. `parse_srt(&str) -> Result<Vec<Cue>,
SubtitleError>` parses `.srt` file contents into cues: it strips a
leading UTF-8 BOM, normalizes `\n`/`\r\n` line endings, and returns
`SubtitleError::InvalidFormat` for malformed blocks (bad index, missing
timing line, unparseable timestamp). Tested against fixture files in
`crates/subtitle/tests/fixtures/`.

No dependency on Slint or libmpv, so it can be tested with fast, isolated
unit tests, later against real `.srt` fixtures.

## `crates/playback-state` (library, package `playback-state`)

Currently an empty library — populated starting at Vaihe 6 in `TODO.md`.

Intended to hold the playback state machine (mode, cursor position, state
transitions such as next/previous/repeat sentence and Normal ↔
Sentence-by-sentence mode switching), with no I/O and no UI, so the
sentence-by-sentence navigation logic can be TDD'd without a Slint window
or a video file.

## `crates/app` (binary, package `trango`)

The binary that ties the Slint UI, libmpv, and the two library crates
together. The package name is `trango` (`[package] name = "trango"`), so
the compiled binary is `trango`; the directory is named `crates/app` to
describe its role. The product name shown in the UI is **TrangoPlayer**.

At this point in development, `crates/app/src/main.rs` only initializes
`tracing` logging and prints the crate version — no Slint UI or libmpv
integration yet (see `TODO.md` for the current step).

## Why three crates instead of one

Splitting `subtitle` and `playback-state` out of the binary means most of
the business logic (subtitle parsing, cue navigation) is testable without
pulling in the heavier Slint/libmpv dependencies, and keeps individual
files small (see `CLAUDE.md`: aim for ~200 lines per file).

## Shared workspace metadata

All three crates inherit `version`, `edition`, and `rust-version` from
`[workspace.package]` in the root `Cargo.toml` (`version.workspace = true`,
etc.), so the version only needs to be bumped in one place.
