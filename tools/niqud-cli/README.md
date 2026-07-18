# niqud-cli

A thin CLI wrapper around [Phonikud](https://github.com/thewh1teagle/phonikud)
(specifically its lightweight [`phonikud-onnx`](https://pypi.org/project/phonikud-onnx/)
runtime package), used by trango's `crates/niqud` to add niqud diacritics
to Hebrew subtitle sentences. Phonikud itself has no CLI — only a Python
library — hence this script. See `docs/src/developer/specs.md`'s "Hebrew
pronunciation" entry for why trango needs this at all.

## Install

```sh
python3 -m venv venv
source venv/bin/activate
pip install -r requirements.txt
```

Download a model file (the smaller int8-quantized one is plenty accurate
and noticeably faster):

```python
python3 -c "from huggingface_hub import hf_hub_download; \
  print(hf_hub_download('thewh1teagle/phonikud-onnx', 'phonikud-1.0.int8.onnx'))"
```

This prints the downloaded file's path — point `NIQUD_MODEL_PATH` at it
(e.g. in your shell profile, or trango's environment):

```sh
export NIQUD_MODEL_PATH=/path/to/phonikud-1.0.int8.onnx
```

**First run needs network access once**, to cache the tokenizer
(`dicta-il/dictabert-large-char-menaked`) from Hugging Face Hub — after
that, every run is fully offline (the script sets `HF_HUB_OFFLINE=1`
itself). Run the script once manually to prime the cache:

```sh
python3 niqud_cli.py "שלום עולם"
```

Finally, make trango able to find it — either put `niqud_cli.py` on
`PATH` as `trango-niqud-cli` (a symlink works fine), or set
`TRANGO_NIQUD_CLI_PATH` to its full path.

## Contract

```
$ python3 niqud_cli.py "הוא שכב במיטה"
{"words": [{"word": "הוא", "niqud": "הוּא"}, {"word": "שכב", "niqud": "שָׁכַב"}, {"word": "במיטה", "niqud": "בַּ|מִּיטָּה"}]}
```

Exit code 0 with the JSON above on stdout; any other exit code means
failure, with an explanation on stderr. No `pronunciation` field — the
Latin pronunciation guide is derived deterministically from `niqud` in
Rust (`crates/niqud/src/transliterate.rs`), not by this script.

## CPU, not GPU

`niqud_cli.py` pins `CPUExecutionProvider` explicitly. This was a
deliberate, measured choice, not an oversight: the int8 model's inference
is already ~16ms on CPU (the ~0.7-1s a fresh invocation takes is Python
interpreter + tokenizer/session startup, not compute), so GPU wouldn't
meaningfully help — and `onnxruntime`'s CUDA provider silently falls back
to CPU if system cuDNN isn't installed, which would otherwise go
unnoticed. See `docs/src/developer/specs.md`.

## License

Phonikud's model and code are licensed [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/),
not vendored into this repository — `niqud_cli.py` only calls it as an
installed package, the same way trango calls `whisper-cli`/`ffmpeg`/Ollama.
