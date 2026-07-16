//! Persists a subtitle's word-by-word analysis to a JSON sidecar file next
//! to it, so re-opening the same video/subtitle reuses the already-computed
//! translations instead of calling Ollama again — both for a single
//! Ctrl+A lookup and for the "Analyze all sentences" batch loop.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::entry::WordAnalysis;

/// A subtitle's cached word-by-word analyses, keyed by `Cue::index`, plus
/// the Ollama model that produced them (kept around for reference — not
/// currently used to invalidate the cache, since re-analyzing with a
/// different model is left to a "delete the cache file" user action rather
/// than automatic invalidation).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnalysisCache {
    /// Name of the Ollama model used to produce these entries.
    #[serde(default)]
    pub model: String,
    /// Per-cue analyses, keyed by `Cue::index`.
    #[serde(default)]
    pub entries: HashMap<u32, WordAnalysis>,
}

/// The cache file path for `subtitle_path`: same folder and stem, with the
/// extension replaced by `.wordanalysis.json` — e.g. `subs.srt` ->
/// `subs.wordanalysis.json`.
pub fn cache_path_for(subtitle_path: &Path) -> PathBuf {
    subtitle_path.with_extension("wordanalysis.json")
}

/// Reads and parses `path` into an `AnalysisCache`. Returns
/// `AnalysisCache::default()` — not an error — if the file doesn't exist,
/// can't be read, or doesn't parse as valid JSON, logging a warning in the
/// latter two cases: a missing or corrupt cache file shouldn't stop word
/// analysis from working, it should just start from empty (same
/// reasoning as `crates/app/src/config.rs`'s `load_from`).
pub fn load_cache(path: &Path) -> AnalysisCache {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return AnalysisCache::default(),
        Err(err) => {
            tracing::warn!(?path, %err, "failed to read word-analysis cache file");
            return AnalysisCache::default();
        }
    };
    match serde_json::from_str(&contents) {
        Ok(cache) => cache,
        Err(err) => {
            tracing::warn!(?path, %err, "failed to parse word-analysis cache file");
            AnalysisCache::default()
        }
    }
}

/// Serializes `cache` to `path` as JSON, creating its parent directory if
/// needed. Errors are logged, not propagated — losing a cache write
/// shouldn't interrupt whatever analysis run produced it (the same
/// reasoning as `config.rs`'s `save_to`); the next run just re-analyzes
/// whatever didn't make it to disk.
pub fn save_cache(path: &Path, cache: &AnalysisCache) {
    if let Some(parent) = path.parent() {
        if let Err(err) = std::fs::create_dir_all(parent) {
            tracing::warn!(?parent, %err, "failed to create word-analysis cache directory");
            return;
        }
    }
    let contents = match serde_json::to_string_pretty(cache) {
        Ok(contents) => contents,
        Err(err) => {
            tracing::warn!(%err, "failed to serialize word-analysis cache");
            return;
        }
    };
    if let Err(err) = std::fs::write(path, contents) {
        tracing::warn!(?path, %err, "failed to write word-analysis cache file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::WordEntry;

    #[test]
    fn test_cache_path_for_replaces_extension() {
        // Given/When/Then: an .srt sidecar path gets a .wordanalysis.json cache path
        assert_eq!(
            cache_path_for(Path::new("/videos/subs.srt")),
            PathBuf::from("/videos/subs.wordanalysis.json")
        );
    }

    #[test]
    fn test_cache_path_for_replaces_non_srt_extension_too() {
        // Given/When/Then: whatever the subtitle's extension is, it's
        //        swapped for .wordanalysis.json, not appended
        assert_eq!(
            cache_path_for(Path::new("/videos/subs.vtt")),
            PathBuf::from("/videos/subs.wordanalysis.json")
        );
    }

    #[test]
    fn test_load_cache_missing_file_returns_default() {
        // Given: a path that doesn't exist
        // When:  loading it
        // Then:  an empty cache comes back, not an error
        let cache = load_cache(Path::new("/no/such/trango-word-analysis-test/cache.json"));

        assert_eq!(cache, AnalysisCache::default());
    }

    #[test]
    fn test_load_cache_corrupt_file_returns_default() {
        // Given: a file that exists but isn't valid JSON
        // When:  loading it
        // Then:  an empty cache comes back, not a panic
        let dir = std::env::temp_dir().join("trango-test-word-analysis-cache-corrupt");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("failed to create temp test dir");
        let path = dir.join("cache.json");
        std::fs::write(&path, b"this is not { valid json").expect("failed to write fixture file");

        let cache = load_cache(&path);

        assert_eq!(cache, AnalysisCache::default());

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }

    #[test]
    fn test_save_then_load_round_trips() {
        // Given: a cache with two cue entries, saved to a temp file
        // When:  loading it back
        // Then:  the loaded cache matches what was saved
        let dir = std::env::temp_dir().join("trango-test-word-analysis-cache-round-trip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("cache.json");
        let mut entries = HashMap::new();
        entries.insert(
            0,
            WordAnalysis {
                words: vec![WordEntry {
                    word: "hola".to_string(),
                    translation: "hi".to_string(),
                    pronunciation: "OH-lah".to_string(),
                }],
            },
        );
        let cache = AnalysisCache {
            model: "llama3.1:8b".to_string(),
            entries,
        };

        save_cache(&path, &cache);
        let loaded = load_cache(&path);

        assert_eq!(loaded, cache);

        std::fs::remove_dir_all(&dir).expect("failed to clean up temp test dir");
    }
}
