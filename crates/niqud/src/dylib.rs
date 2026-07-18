//! Locates a usable `libonnxruntime.so` and initializes `ort`'s global
//! environment with it exactly once, so a normal user never has to set
//! `ORT_DYLIB_PATH` by hand — see `docs/src/developer/technology/ort.md`.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::error::NiqudError;

/// Directories Debian/Ubuntu's `libonnxruntime1.23`/`libonnxruntime-dev`
/// packages install into. Only the *directories* are fixed here — the
/// exact versioned filename (e.g. `libonnxruntime.so.1.23`) varies by
/// installed package version, so each is scanned for anything matching.
const ONNXRUNTIME_SEARCH_DIRS: &[&str] = &[
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
    "/usr/lib",
    "/usr/local/lib",
];

/// Parses the version suffix after `"libonnxruntime.so."` into numeric
/// components for correct ordering — plain string comparison would rank
/// `.so.1.9` above `.so.1.23` (`'9' > '2'` lexicographically), which is
/// backwards.
fn version_key(name: &str) -> Vec<u64> {
    name.trim_start_matches("libonnxruntime.so.")
        .split('.')
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

/// Searches `dirs` for a `libonnxruntime.so*` file, preferring an
/// unversioned `libonnxruntime.so` (only shipped by `-dev`-style
/// packages) over a versioned one, and the numerically highest versioned
/// filename (e.g. `.so.1.24` over `.so.1.23`) if only versioned ones are
/// found.
fn find_onnxruntime_dylib_in(dirs: &[&Path]) -> Option<PathBuf> {
    let mut best: Option<(String, PathBuf)> = None;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if name == "libonnxruntime.so" {
                return Some(entry.path());
            }
            if name.starts_with("libonnxruntime.so.") {
                let is_better = match &best {
                    Some((best_name, _)) => version_key(&name) > version_key(best_name),
                    None => true,
                };
                if is_better {
                    best = Some((name, entry.path()));
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

/// [`find_onnxruntime_dylib_in`] over [`ONNXRUNTIME_SEARCH_DIRS`].
fn find_onnxruntime_dylib() -> Option<PathBuf> {
    let dirs: Vec<&Path> = ONNXRUNTIME_SEARCH_DIRS.iter().map(Path::new).collect();
    find_onnxruntime_dylib_in(&dirs)
}

static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

/// Initializes `ort`'s global environment exactly once (subsequent calls
/// return the same cached result), pointed at `ORT_DYLIB_PATH` if set
/// (an escape hatch for a custom build, not something a normal install
/// needs) or else a dylib found by [`find_onnxruntime_dylib`]. Returns a
/// clear error immediately — without ever calling into `ort` — if
/// neither resolves to anything, since `ort`'s own dylib-not-found/
/// version-mismatch failure modes have been observed to hang rather than
/// error cleanly (see `docs/src/developer/technology/ort.md`); callers
/// still need their own timeout around anything that *does* reach `ort`
/// (a found-but-incompatible library can still hang there).
pub fn ensure_ort_initialized() -> Result<(), NiqudError> {
    let dylib_path = std::env::var_os("ORT_DYLIB_PATH")
        .map(PathBuf::from)
        .or_else(find_onnxruntime_dylib)
        .ok_or_else(|| {
            NiqudError::ModelLoadFailed(
                "libonnxruntime.so not found (checked /usr/lib/x86_64-linux-gnu, \
                 /usr/lib/aarch64-linux-gnu, /usr/lib, /usr/local/lib). Install \
                 libonnxruntime1.23 (or newer) via your package manager — see \
                 docs/src/developer/technology/ort.md."
                    .to_string(),
            )
        })?;

    ORT_INIT
        .get_or_init(|| {
            // EnvironmentBuilder::commit() returns bool, not Result — false
            // just means an environment was already committed (e.g. by an
            // earlier call, since ort guards this globally too), not an
            // error; only init_from() itself can fail (bad/missing dylib).
            ort::init_from(&dylib_path)
                .map(|builder| {
                    builder.commit();
                })
                .map_err(|err| err.to_string())
        })
        .clone()
        .map_err(NiqudError::ModelLoadFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), b"").expect("failed to write fixture file");
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("trango-test-niqud-dylib-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        dir
    }

    #[test]
    fn test_find_prefers_unversioned_symlink_name() {
        // Given: a directory with both a versioned file and the
        //        unversioned name (as -dev packages ship)
        // When:  searching for the dylib
        // Then:  the unversioned one wins
        let dir = temp_dir("prefers-unversioned");
        touch(&dir, "libonnxruntime.so.1.23");
        touch(&dir, "libonnxruntime.so");

        let found = find_onnxruntime_dylib_in(&[&dir]);

        assert_eq!(found, Some(dir.join("libonnxruntime.so")));
        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_find_picks_the_highest_versioned_name_numerically() {
        // Given: only versioned files, no unversioned symlink — including
        //        a pair (1.9 vs 1.23) where plain string comparison would
        //        pick the wrong one ('9' > '2' lexicographically, even
        //        though 23 > 9 numerically)
        // When:  searching for the dylib
        // Then:  the numerically highest version wins
        let dir = temp_dir("picks-highest-version");
        touch(&dir, "libonnxruntime.so.1.9");
        touch(&dir, "libonnxruntime.so.1.20");
        touch(&dir, "libonnxruntime.so.1.23");

        let found = find_onnxruntime_dylib_in(&[&dir]);

        assert_eq!(found, Some(dir.join("libonnxruntime.so.1.23")));
        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_find_returns_none_when_nothing_matches() {
        // Given: a directory with unrelated files only
        // When:  searching for the dylib
        // Then:  None comes back, not a panic
        let dir = temp_dir("returns-none");
        touch(&dir, "libsomethingelse.so.1");

        assert_eq!(find_onnxruntime_dylib_in(&[&dir]), None);
        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_find_skips_missing_directories_without_erroring() {
        // Given: a search path that doesn't exist at all
        // When:  searching for the dylib
        // Then:  None comes back rather than panicking on the read_dir error
        let missing = Path::new("/no/such/trango-test-onnxruntime-dir");

        assert_eq!(find_onnxruntime_dylib_in(&[missing]), None);
    }
}
