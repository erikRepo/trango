# Specs

Not written yet. This section will hold functional specifications for app
behavior that go beyond what's already covered by the repository root
`README.md` handoff spec — for example, implementation decisions the
handoff spec leaves open (see e.g. `TODO.md` Vaihe 21, Normal mode's
sentence-panel behavior).

## TODO: Open Video dialog folder switching

The Open Video dialog (`TODO.md` Vaihe 18) lists video files from a single
default folder (`main.rs`'s `default_video_folder`: the CLI video path's
parent directory if one was given, otherwise the current working
directory) — there is no in-dialog control to browse to a different folder.
Listed explicitly out of scope for Vaihe 18 in `TODO.md`'s "Ei tässä
listassa" section; a native folder picker is the planned approach when this
is picked up.
