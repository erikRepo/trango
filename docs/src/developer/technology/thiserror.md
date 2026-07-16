# thiserror

## What it is

[`thiserror`](https://docs.rs/thiserror) is a derive macro that generates
`std::error::Error` and `Display` implementations for custom error enums
and structs.

## Why it's needed

`CLAUDE.md` requires `thiserror`-based error types in library crates
(as opposed to `anyhow`, which is reserved for the `trango` binary and
integration tests).

## Why this one

It's the standard low-boilerplate way to define typed, matchable error
enums in Rust library code, without hand-writing `Display`/`Error` impls.

## Usage in this project

Used in `crates/subtitle/src/error.rs` (package `subtitle`) to define
`SubtitleError`:

```rust
#[derive(Debug, Error)]
pub enum SubtitleError {
    #[error("invalid subtitle format: {0}")]
    InvalidFormat(String),

    #[error("failed to read subtitle file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("cue {index}: end time ({end:?}) must be after start time ({start:?})")]
    InvalidTiming { index: u32, start: Duration, end: Duration },
}
```

`InvalidTiming` is currently returned by `Cue::new` when `start >= end`.
`InvalidFormat` and `IoError` are reserved for the `.srt` parser added in
`TODO.md` Vaihe 4.

## Pitfalls

- `#[from]` generates a `From` impl, which enables `?`-based conversion
  from `std::io::Error` — but only one variant per source error type can
  use `#[from]`, since the mapping needs to be unambiguous.
