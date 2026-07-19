//! Small `std::process::Command` helpers shared by this crate's external
//! tool wrappers (`espeak-ng`, `ffmpeg`) — mirrors `subtitle::generate`'s
//! own `run_command`/`last_stderr_line`, duplicated here rather than
//! shared across crates for two small helpers (same tradeoff already
//! made elsewhere in this codebase, e.g. `contains_hebrew` between
//! `word-analysis` and `niqud`).

use std::io;
use std::process::Command;

/// Runs `command`, retrying briefly (up to 4 times, 20ms apart) if the OS
/// reports `ExecutableFileBusy` (errno `ETXTBSY`) — a transient race that
/// can happen if the target binary was written to disk moments earlier
/// (its write handle not fully released yet when exec is attempted; this
/// crate's own tests hit it occasionally, writing a fresh fake binary
/// immediately before running it) rather than an installed system binary
/// that's been sitting on disk unchanged.
pub(crate) fn run_command(command: &mut Command) -> io::Result<std::process::Output> {
    for attempt in 0..5 {
        match command.output() {
            Err(err) if attempt < 4 && err.kind() == io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            result => return result,
        }
    }
    unreachable!()
}

/// The last non-empty line of `stderr` — external tools' real error tends
/// to be the final line after loader/setup chatter, so showing just that
/// keeps `PracticeAudioError::GenerationFailed`'s message readable
/// instead of dumping the whole log.
pub(crate) fn last_stderr_line(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no error output")
        .trim()
        .to_string()
}
