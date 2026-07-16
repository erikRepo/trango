# Crate structure

trango is a Cargo workspace with four members, all inheriting `version`,
`edition`, and `rust-version` from `[workspace.package]` in the root
`Cargo.toml`.

## `crates/subtitle` (library)

`Cue { index, start, end, text, translation }` and `SubtitleError` (see
[thiserror](../technology/thiserror.md)). `parse_srt(&str) ->
Result<Vec<Cue>, SubtitleError>` parses `.srt` content (strips a BOM,
normalizes line endings), tested against fixtures in
`crates/subtitle/tests/fixtures/`. `merge_translation(original,
translation)` attaches a translation track by **timing overlap**, not
index — each original cue takes the translation cue with the most
overlap — since the two tracks may not have matching cue counts (e.g. a
hand-timed original paired with an STT-generated translation). No
Slint/libmpv dependency.

## `crates/playback-state` (library)

Depends on `subtitle`. `PlaybackMode` (`Normal` | `SentenceBySentence`,
default `SentenceBySentence`) and `PlayerState { mode, cues,
current_cue_index, show_translation }`.

Cue navigation is pure logic returning a `SeekCommand`/`PlaySpanCommand`
— "what the player should do" — rather than driving mpv directly:
`next_cue`/`previous_cue` move the cursor and return a command (`None` at
either end); `repeat_current_cue` never moves the cursor, always
returning the same command for the same cue; `jump_to_cue(index)` backs
the sentence list's row clicks, sharing the same command logic so clicks
behave exactly like arrow navigation.

`format_time(seconds) -> String` formats `MM:SS`/`H:MM:SS`, clamping
non-finite/negative input to `00:00`. `sync_cue_to_time(time)` finds the
latest cue starting at-or-before `time`, driving `Normal` mode's live
sentence tracking (see [Video playback](video-playback.md)).

No I/O, no UI — TDD'd standalone.

## `crates/word-analysis` (library)

Word-by-word sentence analysis, split out the same way for testability
without Slint/libmpv. `WordEntry`/`WordAnalysis` (`entry.rs`) is the data
model. `cache.rs` persists analyses to a JSON sidecar
(`subs.srt` → `subs.wordanalysis.json`), `AnalysisCache { model, entries:
HashMap<u32, WordAnalysis> }` keyed by `Cue::index`; a missing/corrupt
cache loads as empty rather than erroring. `ollama.rs`'s `OllamaClient`
trait (`list_models`, `analyze_sentence`) lets tests swap in a fake
instead of a real server; `HttpOllamaClient` talks to
`http://localhost:11434` via [ureq](../technology/ureq.md) (`GET
/api/tags`, `POST /api/generate` with `stream: false`/`format: "json"`),
defensively stripping a ` ```json ` fence some models add. Prompt-building
and response-parsing are plain functions tested with canned strings;
`HttpOllamaClient` itself is tested against a local mock HTTP server
(`TcpListener` on a random port).

## `crates/app` (binary, package `trango`)

Ties Slint, libmpv, and the two library crates together. Package name
`trango` (binary name), directory `crates/app`; UI-facing product name is
**TrangoPlayer**.

`main.rs` initializes `tracing`, opens the Slint window
(`app-window.slint`), and always calls
`video_player::VideoPlayer::attach` once at startup — even with no CLI
video path — because Slint's `RenderingSetup` notification only ever
fires once per window (see [Video playback](video-playback.md) for why
this can't be deferred). A CLI video argument starts loading immediately;
otherwise the video area stays a placeholder until one is picked via the
top bar's "Open video…" (`open_video_dialog.rs`: lists a folder's
subfolders/videos as rows, auto-matches a same-stem `.srt`, only `Video`
rows are selectable) or a second/third CLI argument (`subs.srt`, and a
translation `subs.en.srt` merged via `subtitle::merge_translation`). A
subtitle or translation file that can't be read/parsed is logged and
skipped rather than blocking video playback.

`wire_player_state` creates the shared `Rc<RefCell<PlayerState>>`
(UI-thread-only, so no `Send`/`Sync` needed) and wires
`toggle-mode`/`toggle-translation` to `PlayerState`'s methods, mirroring
the result back into `AppWindow` properties the top bar/translation
toggle read directly.

## Why four crates instead of one

Splitting `subtitle`, `playback-state`, and `word-analysis` out of the
binary keeps most business logic testable without Slint/libmpv, and keeps
files small (CLAUDE.md: aim for ~200 lines/file).
