# Subtitles

TrangoPlayer works with two subtitle tracks per video: an **original**
subtitle (the language you're learning) and an optional **translation**
subtitle (shown alongside it).

## Original subtitle

Opening a video automatically looks for a subtitle file with the same
name next to it (`video.mp4` → `video.srt`) and links it if found — see
[Opening your first video](../getting-started/first-video.md).

If none is found, open the **"Subtitles…"** dialog from the top bar. Its
first section shows either the linked file, or — if none was found — an
empty state with a **"Generate subtitles"** button that transcribes the
video's audio automatically. See
[Generating subtitles automatically](generating-subtitles.md).

## Translation subtitle

The dialog's second section lets you link a second `.srt` file as a
translation track, using the same in-app file browser as elsewhere in
TrangoPlayer, scoped to `.srt` files next to the video. Picking one
merges it in immediately.

The two tracks don't need to have the same number of lines — TrangoPlayer
matches them up by comparing each line's timing, not its position in the
file, so a hand-timed original and a machine-generated translation still
line up correctly.

## Showing the translation

Once a translation is linked, the current-sentence card's toggle switch
(or the **Ctrl+T** keyboard shortcut) shows or hides the translated line
underneath the original. It's off by default, works in both playback
modes, and is purely visual — it never affects what's playing.
