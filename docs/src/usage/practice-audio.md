# Practice audio

The Open Subtitles dialog's **"Generate practice audio"** button turns
the whole loaded subtitle into a folder of standalone `.mp3` files — one
per sentence — for offline listen-and-repeat practice, no TrangoPlayer
needed to use them.

## What's in each file

For every word in the sentence, in order:

1. Its translation, spoken aloud (TTS)
2. That word's real audio at 50% speed, twice
3. Then at 75% speed, twice
4. Then at normal speed, twice

Once every word has gone through that, the whole sentence's real audio
plays three times at normal speed. A pause follows every single piece,
long enough to say it back yourself before the next one starts.

## Using it

Needs two things done first:
- A whisper model selected (the same one used for subtitle generation).
- [**"Analyze all sentences"**](word-analysis.md) already run — practice
  audio reads the translations it already computed rather than calling
  Ollama itself, so any sentence without a cached analysis is skipped
  (with a note in the log) rather than triggering a fresh analysis run.

Click **"Generate practice audio"**; a progress count shows while it
works through the subtitle (this re-transcribes each sentence's audio
for precise word timing, so it takes a while for a long video). The
files land in a new folder next to the video,
`practice-audio/<video name>-<date>_<time>/`, named `0001.mp3`,
`0002.mp3`, and so on in subtitle order.

## Setting up espeak-ng

The per-word translation voice needs [espeak-ng](https://github.com/espeak-ng/espeak-ng)
installed and on your `PATH` — install it via your system's package
manager (e.g. `apt install espeak-ng` on Ubuntu/Debian). See
[Settings](settings.md#locating-external-tools) if it's installed
somewhere else.
