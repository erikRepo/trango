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
