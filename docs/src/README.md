# TrangoPlayer

TrangoPlayer is a desktop video player built for learning a language by
watching videos with subtitles.

You open a video together with a subtitle file, and TrangoPlayer can play it
in two ways:

- **Normal mode** — a regular video player: continuous playback with a
  scrub bar, like any other video app.
- **Sentence-by-sentence mode** — built specifically for language learners.
  The video is driven by the subtitle timing instead of the clock: one key
  jumps to the next line, another replays the line you're on as many times
  as you want, and a toggle reveals a translated line underneath the
  original so you can check your understanding without leaving the video.

If a video doesn't have a subtitle file yet, TrangoPlayer can generate one
automatically (using speech recognition that runs entirely on your own
computer), and it can look up a word-by-word translation and pronunciation
guide for whichever sentence is currently on screen.

## Where to go next

- **[Getting Started](getting-started/installation.md)** — install
  TrangoPlayer and open your first video.
- **Using TrangoPlayer** — the features above, one page each: playback
  modes, keyboard shortcuts, subtitles, automatic subtitle generation,
  word-by-word analysis, and settings.
- **Developer Guide** — for anyone working on TrangoPlayer's own code:
  architecture, design decisions, and the third-party libraries it's built
  on.

## Everything runs on your computer

TrangoPlayer doesn't send your videos, subtitles, or viewing activity
anywhere. Subtitle generation and word analysis both use software that
runs locally — nothing is uploaded to a cloud service. See
[Generating subtitles automatically](usage/generating-subtitles.md) and
[Word-by-word analysis](usage/word-analysis.md) for what that software is
and how to install it.
