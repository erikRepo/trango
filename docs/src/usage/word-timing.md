# Word timing

**Ctrl+W** shows exactly where each word of the sentence currently on
screen starts and ends within its audio, and lets you play any single
word back on its own — useful for checking pronunciation word by word,
or just to hear a tricky word again in isolation.

## Using it

Press **Ctrl+W** while a sentence is shown in the current-sentence card
(works in both Normal and Sentence-by-sentence mode, same as Ctrl+A).
TrangoPlayer re-analyzes just that sentence's audio and lists its words
with their start/end times — this takes a moment, since it runs
whisper-cli again for a short, precise result rather than reusing the
subtitle's own (much coarser) sentence-level timing. Click any word in
the list to play just that word.

This reuses whichever whisper model is already selected for
[generating subtitles](generating-subtitles.md) — no separate setup.
TrangoPlayer shows a clear inline message instead of a generic error if
no model is selected, no video/audio is open, or no sentence is
currently in focus.
