# ureq

## What it is

[`ureq`](https://docs.rs/ureq) is a small, synchronous (blocking) HTTP
client for Rust.

## Why it's needed

`TODO.md` Vaihe 24 talks to a local Ollama instance over HTTP (listing
installed models, and asking a model to analyze a sentence word-by-word)
for the Ctrl+A popup and "Analyze all sentences" batch loop.

## Why this one

trango has no async runtime anywhere in the codebase — background work
(e.g. `subtitle::WhisperCliGenerator` running `whisper-cli`) is done with
plain `std::thread::spawn` plus `slint::invoke_from_event_loop` to hand
results back to the UI thread. Pulling in `reqwest` would mean adding
`tokio` as a dependency solely for this one feature. `ureq` is
synchronous, so an Ollama call is just a blocking function call made from
inside a `thread::spawn` closure — no new execution model for the rest of
the app to learn.

## Usage in this project

Used in `crates/word-analysis/src/ollama.rs`'s `HttpOllamaClient`, with
the `json` feature enabled (for `send_json`/`read_json`):

```rust
let body: TagsResponse = ureq::get(&url)
    .call()?
    .body_mut()
    .read_json()?;

let response: GenerateResponse = ureq::post(&url)
    .send_json(&request_body)?
    .body_mut()
    .read_json()?;
```

## Pitfalls

- By default, `ureq` turns any non-2xx HTTP status into an
  `Err(ureq::Error::StatusCode(status))` rather than returning the
  response — `HttpOllamaClient`'s `map_ureq_error` relies on this to
  distinguish "Ollama responded with an error" (`OllamaError::Http`) from
  "couldn't reach Ollama at all" (`OllamaError::ConnectionFailed`).
- `.body_mut()`/`.read_json()` are `http::Response<ureq::Body>` methods,
  not `ureq`-specific ones — `ureq::get(...).call()` returns a plain
  `http::Response<Body>`.
