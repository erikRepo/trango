# Opening your first video

## From the command line

```
cargo run --release -p trango -- <path/to/video> [path/to/subs.srt] [path/to/subs.translation.srt]
```

All three arguments are optional:

- With no arguments, TrangoPlayer opens an empty window — use the top
  bar's **"Open video…"** button to pick a file.
- With just a video path, TrangoPlayer looks for a subtitle file with the
  same name next to it (`video.mp4` → `video.srt`) and links it
  automatically if found.
- The second argument links a specific subtitle file explicitly.
- The third argument links a second subtitle file as a **translation**
  track, shown alongside the original (see
  [Subtitles](../usage/subtitles.md)).

## From inside the app

Click **"Open video…"** in the top bar. This opens an in-app file browser
(not your operating system's native file picker) starting in the folder
of the last video you opened. Navigate into subfolders with the listed
folder rows, or go up with the **"‥ Up"** row, then select a video and
click **"Open"**.

Opening a video this way also tries to auto-match a same-name subtitle
file, exactly like the command-line path above.

## What happens next

TrangoPlayer always opens **paused** — nothing plays automatically, no
matter how you opened the video or whether a subtitle was found. Press
**Space** to start playback. See [Playback modes](../usage/playback-modes.md)
for what Space, and the rest of the keyboard shortcuts, do in each mode.

If no subtitle file was found, the video still plays fine in Normal mode;
Sentence-by-sentence mode needs a subtitle to know where the sentence
boundaries are. You can link one — or generate one automatically — from
the **"Subtitles…"** button, covered in [Subtitles](../usage/subtitles.md).
