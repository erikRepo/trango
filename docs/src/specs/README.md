# Specs

Not written yet. This section will hold functional specifications for app
behavior that go beyond what's already covered by the repository root
`README.md` handoff spec — for example, implementation decisions the
handoff spec leaves open (see e.g. `TODO.md` Vaihe 21, Normal mode's
sentence-panel behavior).

## Open Video dialog: folder navigation

The Open Video dialog (`TODO.md` Vaihe 18) opens on a default folder
(`main.rs`'s `default_video_folder`: the CLI video path's parent directory
if one was given, otherwise the current working directory), but isn't
limited to it: an "‥ Up" row and clicking a listed subfolder navigate the
dialog in place, re-listing that folder's contents
(`open_video_dialog::list_folder_entries`). This was chosen over a
native OS folder picker to stay consistent with README's "no OS-native
file picker — mockin oma UI" direction for the dialog as a whole, and
needs no new dependency. `TODO.md`'s "Ei tässä listassa" section originally
deferred folder switching with a *native* picker specifically; this in-app
navigation isn't that, so it's covered here instead.

## Open Subtitles dialog: no OS drag-and-drop for the translation link

README specs the translation section's `.srt` linking as an OS-level
drag-and-drop target ("drop a translated .srt file here"). That isn't
implemented as literal drag-and-drop: Slint 1.17.1's winit backend doesn't
relay external file drops to `DropArea` at all (only in-app `DragArea`
sources, of which this dialog has none) — see
`docs/src/technology/slint.md`'s "Pitfalls" section for how that was
confirmed. `TODO.md` Vaihe 19 instead links a translation subtitle through
a small in-app file picker (`open_subtitles_dialog::list_srt_files` +
`crates/app/main.rs`'s `wire_open_subtitles_dialog`'s
`link-translation-requested` handler), reusing the Open Video dialog's
file-list chrome — generalized into `app-window.slint`'s `FileListDialog`
component for that purpose — scoped to the video's own folder's `.srt`
files (no subfolder navigation, unlike the Open Video dialog: a
translation file is expected right next to the video). Picking one there
re-merges cues immediately (not deferred to the Open Subtitles dialog's
"Done" button, which just closes the modal). If Slint gains real OS file
drop support later, this picker can stay as a fallback/alternate entry
point rather than being removed outright.

README's mock also labels the two subtitle sections "(DE)"/"(EN)" as
language-code examples for that specific demo video; since trango doesn't
track subtitle language, the dialog instead uses the generic labels
"Original subtitle" / "Translation".

## Subtitle generation: stub interface, no STT dependency yet

`TODO.md` Vaihe 20 asks for the `subtitleGenerationStatus`
(`Idle | Generating | Done | Error`) flow to be wired end-to-end before any
speech-to-text library is added — adding one (e.g. a local Whisper binding)
is a significant new dependency and needs a separate go-ahead. The
`subtitle` crate's `SubtitleGenerator` trait (`crates/subtitle/src/generate.rs`)
captures the shape a real backend will fill in later:
`fn generate(&self, video_path: &Path) -> Result<PathBuf, SubtitleError>`.
For now, `StubSubtitleGenerator` is the only implementation — it writes a
single fixed placeholder cue to a same-stem `.srt` next to the video (the
same naming convention `open_video_dialog::matching_subtitle_path` looks
for), so a "generated" file is picked up as the video's linked original
subtitle immediately, without a separate refresh step.

At this stage, `crates/app/src/subtitle_generation.rs`'s `generate` ran the
generator synchronously on the UI thread and mirrored the result into
`AppWindow::subtitle-generation-status` plus (on success) the Open
Subtitles dialog's original row. That was fine for a stub that returns
instantly; see "Subtitle generation: whisper-cli as an external process"
below for the real backend that replaced it and moved this off the UI
thread.

## Subtitle generation: whisper-cli as an external process

`TODO.md` Vaihe 21.5 adds the real speech-to-text backend the previous
section deferred. Two implementation options were discussed: a Rust
binding to whisper.cpp (e.g. the `whisper-rs` crate) versus driving
whisper.cpp's own `whisper-cli` binary as an external process via
`std::process::Command`. The external-process route won, for two reasons:
it needs **no new Cargo dependency** (`whisper-cli` isn't linked into
trango at all — the user installs it separately, see
`docs/src/usage/`), and whisper.cpp already ships an `-osrt` flag that
writes a ready-made `.srt` file directly, so trango doesn't need to parse
raw transcript text or timestamps itself. The tradeoff is that generation
now depends on an external tool being installed and discoverable, which
`WhisperCliGenerator`'s `Error` status message (see below) is written to
make legible rather than a generic failure.

`subtitle::WhisperCliGenerator` (`crates/subtitle/src/generate.rs`) runs
`whisper-cli -f <video> [-m <model>] -of <video-stem> -osrt`. The `-of`
flag matters: it must be given the output path *without* an extension,
because `whisper-cli` appends `.srt` itself when `-osrt` is set. Passing
`video_path` with its extension stripped there makes the final output land
at `video_path.with_extension("srt")` — the exact same same-stem
convention `StubSubtitleGenerator` and
`open_video_dialog::matching_subtitle_path` already use — without trango
needing to rename or move whisper-cli's output afterward.

Since `whisper-cli` isn't installed on the machine this was implemented
on (noted in `TODO.md` Vaihe 21.5 itself), `WhisperCliGenerator`'s
automated tests (`crates/subtitle/src/generate.rs`) don't exercise the
real binary. Instead they write small POSIX shell scripts that mimic its
`-of`/`-osrt` contract (writing `<-of value>.srt`, or exiting non-zero
with a stderr message) and point `binary_path` at those — real `Command`
plumbing (argument passing, exit status, stdout/stderr capture) is still
exercised end-to-end, just against a stand-in binary rather than a real
transcription. These tests are `#[cfg(unix)]`-only (the fake scripts need
a POSIX shell and `chmod +x`); the "binary not found" and "video file
missing" cases are platform-independent and run everywhere.

### Background thread, not the UI thread

Real transcription can take seconds to minutes, unlike the stub, so
running it synchronously (the earlier stub-era `generate` function) would
freeze the whole app. `crates/app/src/subtitle_generation.rs::spawn_generate`
runs the generator on a `std::thread::spawn`-ed background thread instead,
reporting its result to a caller-supplied `on_done` callback.

The tricky part: `on_done` runs on the background thread, but updating
`AppWindow` properties and the app's `Rc<RefCell<PlayerState>>`/
`Rc<RefCell<CurrentMedia>>` state must happen on the UI thread, and
`Rc`/`RefCell` aren't `Send` — they can't cross the thread boundary at
all, even transiently through a closure capture. `main.rs`'s
`wire_open_subtitles_dialog` handles this in two hops:

1. `on_done` (background thread) calls `slint::invoke_from_event_loop`
   with a closure that captures only `Send` data — a `Weak<AppWindow>`
   and the owned `Result<PathBuf, SubtitleError>` — mirroring
   `video_player.rs`'s `load_file` pattern for the same reason. That
   closure runs `subtitle_generation::apply_result`, which needs only
   `&AppWindow` to update `subtitle-generation-status`/`-error-message`
   and (on success) the dialog's original row.
2. On success, that same closure invokes a second, UI-thread-only signal:
   `AppWindow::subtitle-generated(string)`. This callback isn't tied to
   any UI element — it exists purely so a *separate* handler, set up once
   in `wire_open_subtitles_dialog` and holding the `Rc`-based state
   directly (no thread crossing involved, since both the `invoke_*` call
   and the handler run on the UI thread), can load the generated subtitle
   into the player and record it in `CurrentMedia`.

This keeps the background thread's payload to genuinely `Send`-safe data
without reaching for `unsafe impl Send` wrappers or switching
`PlayerState`/`CurrentMedia` to `Arc<Mutex<...>>` throughout the app just
for this one feature.

Because `slint::invoke_from_event_loop` only *queues* its closure — it
runs the next time the event loop actually polls — this whole path can't
be driven end-to-end in an automated test the way most of the app's
`AppWindow`-touching code is: `crates/app/src/main.rs`'s tests construct
a real `AppWindow` but never call `AppWindow::run()` (see that test
module's own comment on why only one such window can exist per test
process), so a queued event loop closure never actually executes there.
The test suite instead covers each layer separately: `spawn_generate`'s
thread-spawn-and-callback plumbing is tested directly with a plain
`mpsc` channel (no `AppWindow` involved,
`crates/app/src/subtitle_generation.rs`), `apply_result`'s window-mirroring
is tested against a real `AppWindow` by calling it directly with an
already-resolved `Result`, and the real button wiring
(`window.invoke_generate_subtitles_requested()`) is asserted to return
immediately with status `Generating` — proving it doesn't block the UI
thread — without asserting the later `Done`/`Error` transition, which is
instead covered by manual testing (`TODO.md` Vaihe 21.5's "Voit
ajaa/testata").

## Model selection: UI + autodiscovery instead of an environment variable, persisted to a small TOML config

`TODO.md` Vaihe 21.5 initially configured `WhisperCliGenerator`'s model
through an environment variable (`TRANGO_WHISPER_MODEL_PATH`), mirroring
the binary path's own `TRANGO_WHISPER_CLI_PATH`. Vaihe 21.6 replaces that
for the model specifically (the binary path env var is unchanged) — a
learner is expected to switch models somewhat often (e.g. one per target
language being studied), and re-exporting an environment variable and
restarting the app for that is more friction than the UI can avoid.

The Open Subtitles dialog gained a model row next to "Generate
subtitles" (disabled until a model is picked). Clicking it opens a
`FileListDialog` — the same in-app folder-browsing chrome already used
for the Open Video dialog and the translation-link picker, scoped to
`.bin`/`.gguf` files this time (`crates/app/src/model_picker.rs`,
mirroring `open_video_dialog.rs`'s `FolderEntry`/`list_folder_entries`
shape closely). Three things this module handles that the Open Video
dialog's equivalent doesn't need to:

- **Autodiscovery of a starting folder.** Rather than a folder derived
  from a currently-open file (as the Open Video dialog does) or a plain
  "always start empty", `default_start_folder` tries, in order: the
  config's last-browsed folder (see below) if it still exists; the first
  of a short list of folders whisper.cpp models commonly end up in
  (`candidate_model_folders` — a cloned+built whisper.cpp repo's own
  `models/`, a couple of XDG-ish cache/data locations, and `./models`,
  matching whisper-cli's own default model lookup path) that both exists
  *and* actually contains model files; the first of those that merely
  exists; finally the current working directory. This is deliberately not
  exhaustive OS-specific magic (no registry lookups, no `dirs`/`directories`
  crate) — just a handful of well-known conventions plus always-available
  manual navigation as the fallback, in keeping with README's "no
  OS-native file picker" direction for every other in-app dialog.
- **Persisting the pick.** `crates/app/src/config.rs` adds trango's first
  persistent settings file — `$XDG_CONFIG_HOME/trango/config.toml`
  (falling back to `$HOME/.config/trango/config.toml`), read at startup
  and written whenever a model is confirmed in the picker. This needed a
  new Cargo dependency (`serde` + `toml`) — asked and approved by the user
  before adding, per `CLAUDE.md`. A missing or corrupt config file loads
  as `TrangoConfig::default()` rather than failing startup — losing a
  remembered path is much less disruptive than trango refusing to open.
- **Language inference.** whisper-cli's own `--language` default is
  always `"en"`, regardless of which model is loaded — passing nothing
  would silently mistranscribe non-English audio even with a multilingual
  model loaded. `model_picker::language_flag` inspects the model's
  filename for whisper.cpp's own `.en` naming convention (e.g.
  `ggml-base.en.bin`) and passes `-l en` for those, `-l auto` (explicit
  language auto-detection) for everything else, so the right thing happens
  without asking the user to also pick a language separately. This is
  filename-convention-based, not a guarantee — a model renamed against
  convention would be inferred incorrectly, but whisper.cpp's own
  distribution and download tooling follows this convention consistently.

One more consequence worth documenting: whisper.cpp's smaller models
(`tiny`/`base`/`small`) are trained on far less non-English data than
English, so transcription quality for lower-resource languages (Hebrew
was the concrete case that prompted this) degrades much more on small
models than it does for English. `docs/src/usage/` recommends `medium` or
`large-v3` for anything other than English as a result — this is
documentation/guidance only, trango doesn't enforce or check it.
