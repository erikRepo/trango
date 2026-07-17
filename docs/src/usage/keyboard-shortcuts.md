# Keyboard shortcuts

| Key | Effect | Mode |
| --- | --- | --- |
| Space | Play/pause. In Sentence by sentence with a line in focus: play that line's span, then auto-pause at its end; press again to replay it from the start. Otherwise: plain play/pause toggle. | All |
| Right Arrow | Jump to the next subtitle line's start and pause there | Sentence by sentence |
| Left Arrow | Jump to the previous subtitle line's start and pause there | Sentence by sentence |
| Ctrl+T | Show/hide the translated line under the current sentence | All |
| Ctrl+A | Look up a word-by-word translation for the current sentence | All |
| Ctrl+Space | Start/stop recording system audio to a WAV file | Audio source |

Clicking a row in the sentence list does the same thing as Right/Left
Arrow — jump to that line's start, paused — for whichever row you click.

## Debugging

`--debug` is a command-line flag, not a keyboard shortcut, but is worth
knowing about if a feature isn't behaving as expected:

```
cargo run --release -p trango -- --debug video.mp4 subs.srt
```

It can go anywhere among the other arguments. It turns on detailed
logging for TrangoPlayer's own code — useful mainly when diagnosing
[word analysis](word-analysis.md) issues, since it logs the exact prompt
sent to Ollama and the raw response received.
