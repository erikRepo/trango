# serde

Rust's standard (de)serialization framework — `#[derive(Serialize,
Deserialize)]` generates the conversion code a format crate (here `toml`)
reads/writes against. Used for `TrangoConfig` (`crates/app/src/config.rs`),
trango's persisted settings (whisper model, Ollama model, etc.). Chosen
as the ecosystem's de facto standard: every format crate targets `serde`'s
traits, keeping the config format swappable later.

No pitfalls encountered — a small, single-struct config with no
versioning concerns.
