# ort

Rust bindings for [ONNX Runtime](https://onnxruntime.ai/), used in
`crates/niqud/src/onnx_client.rs` to run the Hebrew niqud diacritization
model directly — no Python, no subprocess (see
[specs.md](../specs.md)'s "Hebrew pronunciation" entry for why a Python
CLI wrapper was tried first and replaced). Chosen over reimplementing ONNX
inference from scratch, obviously; `tokenizers` was *not* added alongside
it, since the specific model's tokenizer turned out to be simple enough
(character-level) to reimplement directly in `tokenizer.rs`.

## Pitfalls

**Build-time vs. runtime linking.** `ort`'s default `download-binaries`
feature fetches a prebuilt ONNX Runtime binary over the network at
*compile* time — breaks offline/CI builds. `crates/niqud/Cargo.toml` uses
`load-dynamic` instead: `libonnxruntime.so` is loaded at *runtime*, via
the `ORT_DYLIB_PATH` env var or the system dynamic linker's normal search
path. This makes the shared library a runtime dependency the user
installs separately (e.g. Ubuntu's `libonnxruntime1.23` package),
consistent with whisper-cli/ffmpeg/Ollama already being external runtime
dependencies.

**API version must be pinned explicitly and conservatively.** With
`default-features = false`, no `api-XX` feature is enabled by default,
which fails to compile against parts of `ort`'s own code. The crate's
own `default` feature set requests `api-24` — against Ubuntu's
apt-packaged `libonnxruntime1.23`, that **hangs indefinitely** rather
than erroring. `api-23` works correctly against the same library. Always
verify a chosen `api-XX` against the actual runtime version being
targeted; a mismatch's failure mode isn't guaranteed to be a clean error.
