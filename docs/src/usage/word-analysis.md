# Word-by-word analysis

**Ctrl+A** breaks down the sentence currently shown in the current-sentence
card word by word, showing a translation and a pronunciation guide for
each word. It uses a locally running [Ollama](https://ollama.com)
instance — like whisper-cli, Ollama runs entirely on your own computer
and isn't bundled with TrangoPlayer, so it needs to be installed
separately.

## Setting up Ollama

1. Install Ollama from [ollama.com](https://ollama.com).
2. Make sure it's running (`ollama serve`, or however your install
   starts it).
3. Pull at least one model: `ollama pull llama3.1` (or any model you
   prefer).

TrangoPlayer talks to Ollama's default local address,
`http://localhost:11434` — no configuration needed if Ollama is running
with its own defaults.

## Picking a model and target language

The Subtitles dialog's **"Ollama model"** row opens a picker listing
whatever models `ollama list` would show. The pick is remembered across
restarts, the same way the whisper model is (see [Settings](settings.md)).

The **"Target language"** field next to it (defaults to "English") is
what translations and pronunciations are produced in — type any language
name. It saves as you type and is remembered across restarts. Changing
it only affects sentences analyzed *after* the change; sentences already
analyzed keep whatever language they were analyzed in until re-analyzed
(delete the cache file described below to force re-analysis in a new
language).

## Using it

**Ctrl+A** works in both Normal and Sentence-by-sentence mode, on
whichever sentence the current-sentence card is showing — in Normal
mode, the card automatically follows along as the video plays, so Ctrl+A
always analyzes the line currently on screen, not whatever line happened
to be current when you switched into Normal mode. The first time
a given sentence is analyzed, it calls Ollama (a few seconds, depending
on the model and machine); every time after that — including across
restarts — it's instant, since the result is cached to a
`<subtitle-name>.wordanalysis.json` file right next to the subtitle file
(e.g. `movie.srt` → `movie.wordanalysis.json`).

**"Analyze all sentences"** (also in the Subtitles dialog, next to the
Ollama model row) runs the same analysis for every sentence in the
currently linked subtitle in one background pass — useful for
pre-analyzing a whole video before watching it, rather than one sentence
at a time via Ctrl+A. It writes to the same cache file, skipping
sentences already analyzed, so it's safe to stop partway through (close
the app, or just decide you have enough) and pick up later — including
after adding individual Ctrl+A analyses in between. A sentence that fails
to analyze is retried a few times before the run moves on to the next
one; if it still fails, it's saved with a blank analysis rather than
stopping the whole run — re-run Ctrl+A on that sentence later to fill it
in.

Both features need a subtitle to be linked and an Ollama model selected
first; TrangoPlayer shows a clear inline message rather than a generic
error if either is missing.

## Hebrew pronunciation

For Hebrew sentences specifically, the pronunciation guide isn't guessed
by Ollama — small local models transliterate Hebrew unreliably even when
they translate it correctly. Instead TrangoPlayer runs a separate niqud
diacritization tool ([Phonikud](https://github.com/thewh1teagle/phonikud)
via a small CLI wrapper, see `tools/niqud-cli/README.md` for installing
it) and derives the pronunciation deterministically from its output. This
happens automatically — Hebrew sentences are detected from their script,
no setting to configure. If the niqud CLI isn't installed (or fails),
Ctrl+A/"Analyze all sentences" still work exactly as before, just with
Ollama's own (less accurate) pronunciation guess for Hebrew lines.

## If a model returns bad or empty analyses

Run with the `--debug` flag to see exactly what prompt was sent to
Ollama and the raw text it returned:

```
cargo run --release -p trango -- --debug video.mp4 subs.srt
```

This is the most common way to diagnose a model returning nothing:
some reasoning-capable models (e.g. the `qwen3` family) can spend their
whole generation budget "thinking" instead of answering unless told not
to. TrangoPlayer already asks models not to do this, but if a similar
issue turns up with a different model, the debug log shows the raw
response that failed to parse. See
[Keyboard shortcuts](keyboard-shortcuts.md) for more on `--debug`.
