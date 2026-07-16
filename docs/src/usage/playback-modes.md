# Playback modes

TrangoPlayer has three modes, switched with the segmented control in the
top bar: **Normal**, **Sentence by sentence**, and **No video**.

## Sentence by sentence

This is the mode built for language learning, and the one TrangoPlayer
starts in by default. Playback is driven by the subtitle's cue timing
instead of the clock:

- **Right Arrow** jumps to the start of the next subtitle line and
  pauses there.
- **Left Arrow** jumps to the start of the previous line, same way.
- Clicking a row in the **sentence list** (the scrollable list on the
  right) jumps straight to that line, exactly like the arrow keys.
- **Space** plays the line you're currently on, from its start to its
  end, then pauses automatically. Press it again and it replays the
  *same* line from the start — it never advances to the next one on its
  own.

Nothing plays until you press Space. Jumping between lines with the
arrow keys or the sentence list only moves the playhead and leaves the
video paused — this is deliberate, so you can look at a line as long as
you like before deciding to hear it.

This mode needs a subtitle file to know where the sentence boundaries
are. See [Subtitles](subtitles.md) for linking or generating one.

## Normal

Continuous playback with a scrub bar, closer to an ordinary video
player. Space still works here — it's a plain play/pause toggle, with no
per-line seeking or auto-pausing. Click or drag the scrub bar to seek to
any point in the video.

## No video

Selecting this mode replaces the video area with an empty placeholder —
there's no video loaded or playing, and the scrub bar/speed slider are
hidden since there's no playhead. The sentence list and Ctrl+A word
analysis still work on whatever subtitle is linked. This mode is the
starting point for live subtitle recording from your system's audio,
which isn't implemented yet.

## Playback speed

A speed slider sits below the scrub bar, always visible in either mode.
Its right edge is normal speed (1.0x) — dragging it left only slows the
video down, in steps down to 0.5x, marked "0.5x"/"0.75x"/"1.0x" along
the track. Useful for hearing a fast line more clearly without losing
per-sentence navigation in Sentence by sentence mode.

## Common to both modes

- **Ctrl+T** toggles a translated line underneath the current sentence,
  if a translation subtitle is linked. Works in either mode, and is
  purely visual — it doesn't affect playback. See
  [Subtitles](subtitles.md).
- **Ctrl+A** looks up a word-by-word breakdown of the current sentence.
  See [Word-by-word analysis](word-analysis.md).

The bottom hint bar always shows whichever of these shortcuts actually
do something in the mode you're currently in.

For the full shortcut list, see [Keyboard shortcuts](keyboard-shortcuts.md).
