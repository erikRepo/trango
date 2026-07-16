# serde_json

## What it is

[`serde_json`](https://docs.rs/serde_json) is the standard JSON
(de)serializer for `serde`-derived Rust types.

## Why it's needed

`TODO.md` Vaihe 24's word-analysis cache (`crates/word-analysis/src/cache.rs`)
is a JSON sidecar file, and Ollama's HTTP API (`crates/word-analysis/src/ollama.rs`)
speaks JSON for both requests and responses — the model's own analysis
reply is itself JSON text embedded inside Ollama's response envelope,
which needs a second parse.

## Why this one

trango already uses `serde` (see `docs/src/technology/serde.md`) for its
TOML config; `serde_json` is `serde`'s equally standard JSON counterpart
and is also what `ureq`'s `json` feature (see `docs/src/technology/ureq.md`)
uses under the hood for `send_json`/`read_json`.

## Usage in this project

Used in `crates/word-analysis/src/cache.rs` for the sidecar cache file:

```rust
let cache: AnalysisCache = serde_json::from_str(&contents)?;
let contents = serde_json::to_string_pretty(&cache)?;
```

And in `crates/word-analysis/src/ollama.rs`'s `parse_analysis_response`,
to parse the `WordAnalysis` JSON a model's `response` text is expected to
contain (after stripping a possible ` ```json ` code fence):

```rust
let analysis: WordAnalysis = serde_json::from_str(without_fence.trim())?;
```

## Pitfalls

- `AnalysisCache::entries` is a `HashMap<u32, WordAnalysis>` — `serde_json`
  accepts integer map keys by stringifying them in the JSON object, so
  this round-trips without a custom (de)serializer, but it does mean the
  cache file's `entries` keys read as strings (`"0"`, `"1"`, ...) when
  inspected by hand.
