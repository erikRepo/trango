# Crate structure

trango is a Cargo workspace (`[workspace]` in the root `Cargo.toml`) with
three members:

## `crates/subtitle` (library, package `subtitle`)

Holds the `Cue` data model (`index`, `start`, `end`, `text`,
`translation`) and `SubtitleError` (see
`docs/src/technology/thiserror.md`). `Cue::new` validates that `start <
end` and leaves `translation` as `None`. `parse_srt(&str) ->
Result<Vec<Cue>, SubtitleError>` parses `.srt` file contents into cues:
it strips a leading UTF-8 BOM, normalizes `\n`/`\r\n` line endings, and
returns `SubtitleError::InvalidFormat` for malformed blocks (bad index,
missing timing line, unparseable timestamp). Tested against fixture
files in `crates/subtitle/tests/fixtures/`.

`merge_translation(original: Vec<Cue>, translation: Vec<Cue>) ->
Vec<Cue>` attaches a second (translation) subtitle track's text onto
`original`'s cues. Matching is done by timing overlap, not by index: for
each original cue, the translation cue whose `[start, end)` range
overlaps it the most supplies the text, and a cue with no overlapping
translation cue keeps `translation: None`. Overlap-based matching was
chosen over index-based matching because the two tracks may not have the
same number of cues — e.g. a hand-timed original paired with an
STT-generated translation — so pairing by position would silently drift
out of sync.

No dependency on Slint or libmpv, so it can be tested with fast, isolated
unit tests, later against real `.srt` fixtures.

## `crates/playback-state` (library, package `playback-state`)

Depends on `subtitle` for the `Cue` type. Holds `PlaybackMode` (`Normal` |
`SentenceBySentence`, defaulting to `Normal`) and `PlayerState { mode,
cues: Vec<Cue>, current_cue_index: Option<usize>, show_translation: bool
}`.

`PlayerState::toggle_mode()` flips between `Normal` and
`SentenceBySentence`. `set_cues(cues)` replaces the loaded cues and resets
`current_cue_index` to `Some(0)`, or `None` if `cues` is empty.
`toggle_translation()` flips `show_translation`.

Cue navigation implements the README's Right/Left/Space rules as pure
logic returning a `SeekCommand { start, end, then_pause }` — "what the
player should do" — instead of driving mpv directly:

- `next_cue()` / `previous_cue()` move `current_cue_index` and return the
  command to play the newly-focused cue's span. At the last/first cue (or
  on an empty cue list) they return `None` and leave the cursor where it
  is — there's nothing further to navigate to.
- `repeat_current_cue()` never moves the cursor; calling it any number of
  times for the same cue returns the identical command, matching the
  README's requirement that Space always replays the same span.

No I/O and no UI yet, so this state machine is TDD'd without a Slint
window or a video file.

## `crates/app` (binary, package `trango`)

The binary that ties the Slint UI, libmpv, and the two library crates
together. The package name is `trango` (`[package] name = "trango"`), so
the compiled binary is `trango`; the directory is named `crates/app` to
describe its role. The product name shown in the UI is **TrangoPlayer**.

`crates/app/src/main.rs` initializes `tracing` logging, prints the crate
version, and opens the Slint main window defined in
`crates/app/ui/app-window.slint` (see `docs/src/technology/slint.md`) —
window background and a top bar showing the version, nothing else yet. No
libmpv integration yet (see `TODO.md` for the current step).

## Why three crates instead of one

Splitting `subtitle` and `playback-state` out of the binary means most of
the business logic (subtitle parsing, cue navigation) is testable without
pulling in the heavier Slint/libmpv dependencies, and keeps individual
files small (see `CLAUDE.md`: aim for ~200 lines per file).

## Shared workspace metadata

All three crates inherit `version`, `edition`, and `rust-version` from
`[workspace.package]` in the root `Cargo.toml` (`version.workspace = true`,
etc.), so the version only needs to be bumped in one place.
