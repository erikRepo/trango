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

`crates/app/src/subtitle_generation.rs`'s `generate` runs the generator
synchronously on the UI thread and mirrors the result into
`AppWindow::subtitle-generation-status` plus (on success) the Open
Subtitles dialog's original row. That's fine for a stub that returns
instantly; a real STT backend will likely need to move this off the UI
thread (e.g. a background thread reporting back via
`slint::invoke_from_event_loop`) in whatever later step wires it in.
