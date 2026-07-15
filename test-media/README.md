# test-media

Small, license-free media fixtures for manual testing (Vaihe 11+ in
[TODO.md](../TODO.md)) and later E2E tests (Vaihe 13).

## `sample/`

- `sample.mp4` — 960x540, ~17s, dark background (`#1c1d22`) with a text label,
  five spoken English sentences as the audio track.
- `sample.srt` — subtitles for `sample.mp4`, one cue per sentence, timed to
  match the generated speech exactly.
- `sample.fi.srt` — Finnish translation track for `sample.srt`, same five
  cue timings, hand-translated text. Used to exercise
  `subtitle::merge_translation` and the translation toggle (Vaihe 17) without
  a second generated audio track.

Both files are generated locally, not sourced from any third party:
- Speech audio: `ffmpeg`'s built-in `flite` filter (offline TTS, voice `slt`),
  reading a script written for this repo.
- Video: `ffmpeg`'s `color`/`drawtext` filters (solid background + label).
- Muxed with `ffmpeg` (H.264 baseline + AAC).

No copyrighted or third-party material is involved, so there are no
attribution/license concerns committing these into the repo.

Regeneration command (voice list: `awb`, `kal`, `kal16`, `rms`, `slt`):

```sh
ffmpeg -f lavfi -i "flite=text='Your sentence.':voice=slt" -ar 44100 -ac 1 line.wav
```

The script used for `sample.srt`'s five sentences:

1. Welcome to Trango Player.
2. This is a short sample video for testing.
3. It contains several spoken sentences.
4. Each sentence has its own subtitle line.
5. Enjoy exploring the sentence by sentence mode.
