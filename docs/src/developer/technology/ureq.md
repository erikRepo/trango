# ureq

Small synchronous HTTP client. Used in `crates/word-analysis/src/ollama.rs`'s
`HttpOllamaClient` (`json` feature, for `send_json`/`read_json`) to talk
to a local Ollama instance. Chosen because trango has no async runtime
anywhere — background work runs via plain `std::thread::spawn`, so a
blocking client needs no new execution model; `reqwest` would have meant
adding `tokio` just for this one feature.

## Pitfall

Non-2xx responses become `Err(ureq::Error::StatusCode(_))` rather than a
returned response — `map_ureq_error` uses this to distinguish "Ollama
responded with an error" from "couldn't reach Ollama at all".
