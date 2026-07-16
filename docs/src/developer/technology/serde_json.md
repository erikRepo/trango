# serde_json

Standard JSON (de)serializer for `serde`-derived types. Used for the
word-analysis cache sidecar (`crates/word-analysis/src/cache.rs`) and to
parse Ollama's JSON response envelope plus the model's own JSON reply
nested inside it (`ollama.rs`). Chosen because trango already uses
`serde`, and it's what `ureq`'s `json` feature uses internally.

## Pitfall

`AnalysisCache::entries` is `HashMap<u32, WordAnalysis>` — `serde_json`
stringifies integer map keys, so the file's `entries` keys read as `"0"`,
`"1"`, ... by hand, though it round-trips fine.
