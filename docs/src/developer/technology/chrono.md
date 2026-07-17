# chrono

Date/time handling with timezone support. Used in
`crates/app/src/system_audio_capture.rs` to build the default recording
filename (`<date>_<time>.wav`, e.g. `2026-07-17_18-42-05.wav`) from the
local wall-clock time. Chosen because the std library's `SystemTime` has
no timezone-aware formatting at all — `chrono` is the most widely used
crate that does, and trango only needs its `Local::now()` +
`DateTime::format` slice of it.

## Pitfall

`Local::now()` isn't called directly inside the filename-building
function — it's passed in as a parameter (`DateTime<Local>`) so tests can
assert on a fixed timestamp instead of a moving target.
