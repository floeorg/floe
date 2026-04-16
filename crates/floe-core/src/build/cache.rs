//! On-disk cache of per-module analyse results.
//!
//! Each compiled module gets a companion `.cache` file under
//! `.floe/cache/`. The cache records the source fingerprint and every
//! dependency's fingerprint, plus whether the analyse pass reported
//! errors. On the next `floe check` run, `CacheStore::is_fresh` compares
//! fingerprints and lets the caller skip re-analysing modules that
//! haven't changed.
//!
//! The on-disk format is `bincode`. Corrupted files are treated as a
//! miss: reading falls back to `None`, callers re-analyse, and the fresh
//! result overwrites the bad bytes on the next write.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

/// The frozen, serializable form of a module's analyse output — today
/// just enough to skip re-checking on a clean rebuild. Future work
/// (issue follow-up) will extend this with the module's public type /
/// value signatures so downstream modules can type-check against cached
/// imports instead of re-parsing them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInterface {
    /// Content fingerprint of the source file this interface was built
    /// from.
    pub source_hash: u64,
    /// Every `.fl` dependency's path → content fingerprint at the time
    /// this module was analysed. Downstream invalidation reads this to
    /// notice when a dep has changed.
    pub dependency_hashes: HashMap<PathBuf, u64>,
    /// True when the analyse pass emitted at least one `Severity::Error`
    /// diagnostic. Freshness checks refuse to serve cached results for
    /// failing modules so the user sees the diagnostics every time.
    pub had_errors: bool,
}

impl ModuleInterface {
    /// Compute the xxh3 fingerprint of the given bytes.
    pub fn fingerprint(bytes: &[u8]) -> u64 {
        xxh3_64(bytes)
    }
}

/// Read / write cache files rooted at `cache_dir`. One instance per
/// build (`PackageCompiler` holds one) so the directory is created
/// lazily and paths share a stable prefix.
pub struct CacheStore {
    cache_dir: PathBuf,
}

impl CacheStore {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Cache path for a source file. Mirrors the source-relative layout
    /// so nothing collides across directories.
    fn cache_path(&self, relative_source: &Path) -> PathBuf {
        let mut out = self.cache_dir.clone();
        out.push(relative_source);
        out.set_extension("cache");
        out
    }

    /// Try to read a previously-written interface. Returns `None` for a
    /// missing or corrupt file — corruption is recoverable because the
    /// next write overwrites it.
    pub fn read(&self, relative_source: &Path) -> Option<ModuleInterface> {
        let path = self.cache_path(relative_source);
        let bytes = std::fs::read(&path).ok()?;
        bincode::serde::decode_from_slice(&bytes, bincode::config::standard())
            .ok()
            .map(|(iface, _)| iface)
    }

    /// Write an interface, creating parent directories as needed.
    pub fn write(
        &self,
        relative_source: &Path,
        interface: &ModuleInterface,
    ) -> std::io::Result<()> {
        let path = self.cache_path(relative_source);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let bytes = bincode::serde::encode_to_vec(interface, bincode::config::standard())
            .map_err(|e| std::io::Error::other(format!("bincode encode: {e}")))?;
        std::fs::write(&path, bytes)
    }

    /// Check whether the cached interface still matches the current
    /// source text and every dependency on disk. A `true` answer means
    /// the caller may skip re-analysing this module; a `false` answer
    /// means something changed (or the cache is stale) and re-analyse
    /// is required.
    pub fn is_fresh(
        interface: &ModuleInterface,
        current_source: &str,
        dependency_sources: &HashMap<PathBuf, String>,
    ) -> bool {
        if interface.had_errors {
            // Don't serve a failing module from cache — the user needs
            // to see the diagnostics every time.
            return false;
        }
        if ModuleInterface::fingerprint(current_source.as_bytes()) != interface.source_hash {
            return false;
        }
        for (path, hash) in &interface.dependency_hashes {
            match dependency_sources.get(path) {
                Some(dep_source) => {
                    if ModuleInterface::fingerprint(dep_source.as_bytes()) != *hash {
                        return false;
                    }
                }
                // Dependency vanished from disk — treat as changed.
                None => return false,
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn iface(source: &str, dep_hashes: HashMap<PathBuf, u64>, had_errors: bool) -> ModuleInterface {
        ModuleInterface {
            source_hash: ModuleInterface::fingerprint(source.as_bytes()),
            dependency_hashes: dep_hashes,
            had_errors,
        }
    }

    #[test]
    fn cache_round_trips_a_clean_interface() {
        let tmp = TempDir::new().unwrap();
        let store = CacheStore::new(tmp.path().to_path_buf());
        let src = "const x = 42";
        let i = iface(src, HashMap::new(), false);
        store.write(Path::new("x.fl"), &i).unwrap();
        let read = store.read(Path::new("x.fl")).unwrap();
        assert_eq!(read.source_hash, i.source_hash);
        assert!(!read.had_errors);
    }

    #[test]
    fn is_fresh_matches_unchanged_source() {
        let src = "const x = 42";
        let i = iface(src, HashMap::new(), false);
        assert!(CacheStore::is_fresh(&i, src, &HashMap::new()));
    }

    #[test]
    fn is_fresh_rejects_changed_source() {
        let i = iface("const x = 42", HashMap::new(), false);
        assert!(!CacheStore::is_fresh(&i, "const x = 43", &HashMap::new()));
    }

    #[test]
    fn is_fresh_rejects_changed_dependency() {
        let dep_path = PathBuf::from("dep.fl");
        let dep_source = "type A {}";
        let mut dep_hashes = HashMap::new();
        dep_hashes.insert(
            dep_path.clone(),
            ModuleInterface::fingerprint(dep_source.as_bytes()),
        );
        let i = iface("const x = 42", dep_hashes, false);
        let mut current_deps = HashMap::new();
        // Dep now has different content.
        current_deps.insert(dep_path, "type A { b: number }".to_string());
        assert!(!CacheStore::is_fresh(&i, "const x = 42", &current_deps));
    }

    #[test]
    fn is_fresh_rejects_vanished_dependency() {
        let mut dep_hashes = HashMap::new();
        dep_hashes.insert(PathBuf::from("dep.fl"), 42);
        let i = iface("const x = 42", dep_hashes, false);
        assert!(!CacheStore::is_fresh(&i, "const x = 42", &HashMap::new()));
    }

    #[test]
    fn is_fresh_refuses_to_serve_failing_module() {
        let i = iface("const x = 42", HashMap::new(), /* had_errors */ true);
        assert!(!CacheStore::is_fresh(&i, "const x = 42", &HashMap::new()));
    }

    #[test]
    fn corrupt_cache_file_reads_as_none() {
        let tmp = TempDir::new().unwrap();
        let store = CacheStore::new(tmp.path().to_path_buf());
        let cache_path = store.cache_path(Path::new("bad.fl"));
        std::fs::create_dir_all(cache_path.parent().unwrap()).unwrap();
        std::fs::write(&cache_path, b"garbage bytes not bincode").unwrap();
        assert!(store.read(Path::new("bad.fl")).is_none());
    }

    #[test]
    fn missing_cache_file_reads_as_none() {
        let tmp = TempDir::new().unwrap();
        let store = CacheStore::new(tmp.path().to_path_buf());
        assert!(store.read(Path::new("nope.fl")).is_none());
    }
}
