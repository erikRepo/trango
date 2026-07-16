# thiserror

Derive macro generating `std::error::Error`/`Display` impls for error
enums. Required by CLAUDE.md for library crates (vs. `anyhow` in the
binary/tests). Used for `SubtitleError` (`crates/subtitle/src/error.rs`):
`InvalidFormat`, `IoError` (`#[from] std::io::Error`), `InvalidTiming {
index, start, end }` — the standard low-boilerplate way to define typed,
matchable error enums without hand-writing `Display`/`Error` impls.

## Pitfall

`#[from]` generates a `From` impl — only one variant per source error
type can use it, since the conversion must stay unambiguous.
