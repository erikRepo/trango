#!/usr/bin/env python3
"""CLI wrapper around phonikud-onnx, since Phonikud itself has no CLI.

Adds niqud diacritics to a Hebrew sentence and prints
{"words": [{"word", "niqud"}, ...]} as JSON on stdout. Exits non-zero
with an explanatory message on stderr on failure. See README.md.

Latin pronunciation is intentionally NOT computed here -- that's done
deterministically in Rust (crates/niqud/src/transliterate.rs) from the
niqud text this script produces.
"""

import json
import os
import sys

# Avoids a network round-trip to Hugging Face Hub on every invocation once
# the tokenizer is cached locally -- see README.md's "first run" note.
os.environ.setdefault("HF_HUB_OFFLINE", "1")


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: niqud_cli.py <sentence>", file=sys.stderr)
        return 1
    sentence = sys.argv[1]

    model_path = os.environ.get("NIQUD_MODEL_PATH")
    if not model_path:
        print(
            "NIQUD_MODEL_PATH is not set -- point it at a downloaded "
            "phonikud-onnx model file (e.g. phonikud-1.0.int8.onnx). "
            "See README.md.",
            file=sys.stderr,
        )
        return 1
    if not os.path.isfile(model_path):
        print(f"NIQUD_MODEL_PATH does not exist: {model_path}", file=sys.stderr)
        return 1

    try:
        import onnxruntime as ort
        from phonikud_onnx import Phonikud
    except ImportError as exc:
        print(
            f"missing dependency: {exc}. Run: pip install -r requirements.txt",
            file=sys.stderr,
        )
        return 1

    try:
        # Pinned explicitly rather than left to onnxruntime's default
        # provider search -- see README.md's "CPU, not GPU" note.
        session = ort.InferenceSession(model_path, providers=["CPUExecutionProvider"])
        model = Phonikud.from_session(session)
        diacritized = model.add_diacritics(sentence)
    except Exception as exc:  # noqa: BLE001 -- surfaced as the process's own error, not re-raised
        print(f"phonikud failed: {exc}", file=sys.stderr)
        return 1

    words = sentence.split()
    niquds = diacritized.split()
    result = {
        "words": [
            {"word": word, "niqud": niqud} for word, niqud in zip(words, niquds)
        ]
    }
    print(json.dumps(result, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
