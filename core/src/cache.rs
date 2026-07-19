use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Current cache schema version. Bump this whenever the JSON schema produced by
/// symbol or reference extractors changes so that stale entries are invalidated.
pub const CACHE_VERSION: u32 = 1;

/// Version of the migration-assessor tool. Included in cache keys so that
/// upgrades invalidate previous cache entries.
pub const TOOL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Content-addressed key for cached per-file analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheKey {
    pub file_hash: String,
    pub parser_version: String,
    pub tool_version: String,
    pub cache_version: u32,
}

impl CacheKey {
    /// Build a cache key for a source file.
    pub fn for_file(path: &Path, parser_version: &str, tool_version: &str) -> anyhow::Result<Self> {
        let content = std::fs::read(path)?;
        let file_hash = hex_hash(&content);
        Ok(Self {
            file_hash,
            parser_version: parser_version.to_string(),
            tool_version: tool_version.to_string(),
            cache_version: CACHE_VERSION,
        })
    }

    /// Stable digest used for cache entry file paths.
    pub fn digest(&self) -> String {
        let json = serde_json::to_string(self).expect("CacheKey serializes to JSON");
        hex_hash(json.as_bytes())
    }
}

fn hex_hash(data: &[u8]) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(data);
    format!("{:016x}", hasher.finish())
}

/// On-disk content-addressed cache for per-file analysis results.
pub struct AnalysisCache {
    root: PathBuf,
}

impl AnalysisCache {
    /// Open or create a cache rooted at `<project_root>/.migration-cache`.
    pub fn new(project_root: &Path) -> anyhow::Result<Self> {
        Self::new_namespaced(project_root, "default")
    }

    /// Open or create a cache rooted at `<project_root>/.migration-cache/<namespace>`.
    ///
    /// Namespaces allow independent stages (e.g. symbol extraction and reference
    /// extraction) to share a cache root without racing on the same entry files.
    pub fn new_namespaced(project_root: &Path, namespace: &str) -> anyhow::Result<Self> {
        let root = project_root.join(".migration-cache").join(namespace);
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Read a cached JSON value, if present and valid.
    pub fn get(&self, key: &CacheKey) -> Option<serde_json::Value> {
        let path = self.entry_path(key);
        if !path.exists() {
            return None;
        }
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Store a JSON value in the cache.
    pub fn put(&self, key: &CacheKey, value: &serde_json::Value) -> anyhow::Result<()> {
        let path = self.entry_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(value)?)?;
        Ok(())
    }

    fn entry_path(&self, key: &CacheKey) -> PathBuf {
        let digest = key.digest();
        let prefix = &digest[..2];
        let suffix = &digest[2..];
        self.root.join(prefix).join(format!("{}.json", suffix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let mut dir = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            dir.push(format!(
                "migration-core-cache-test-{}-{}",
                std::process::id(),
                nanos
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn cache_key_same_content_same_key() {
        let tmp = TempDir::new();
        let path = tmp.path().join("source.ts");
        std::fs::write(&path, "export const x = 1;").unwrap();

        let key1 = CacheKey::for_file(&path, "oxc-0.140.0", "0.1.0").unwrap();
        let key2 = CacheKey::for_file(&path, "oxc-0.140.0", "0.1.0").unwrap();

        assert_eq!(key1.digest(), key2.digest());
    }

    #[test]
    fn cache_key_different_content_different_key() {
        let tmp = TempDir::new();
        let path_a = tmp.path().join("a.ts");
        let path_b = tmp.path().join("b.ts");
        std::fs::write(&path_a, "export const x = 1;").unwrap();
        std::fs::write(&path_b, "export const x = 2;").unwrap();

        let key_a = CacheKey::for_file(&path_a, "oxc-0.140.0", "0.1.0").unwrap();
        let key_b = CacheKey::for_file(&path_b, "oxc-0.140.0", "0.1.0").unwrap();

        assert_ne!(key_a.digest(), key_b.digest());
    }

    #[test]
    fn cache_key_parser_version_changes_digest() {
        let tmp = TempDir::new();
        let path = tmp.path().join("source.ts");
        std::fs::write(&path, "export const x = 1;").unwrap();

        let key1 = CacheKey::for_file(&path, "oxc-0.140.0", "0.1.0").unwrap();
        let key2 = CacheKey::for_file(&path, "oxc-0.140.1", "0.1.0").unwrap();

        assert_ne!(key1.digest(), key2.digest());
    }

    #[test]
    fn analysis_cache_round_trip() {
        let tmp = TempDir::new();
        let cache = AnalysisCache::new(tmp.path()).unwrap();

        let key = CacheKey {
            file_hash: "abc".to_string(),
            parser_version: "oxc-0.140.0".to_string(),
            tool_version: "0.1.0".to_string(),
            cache_version: CACHE_VERSION,
        };
        let value = serde_json::json!({"module": "src/index.ts", "symbols": []});

        assert!(cache.get(&key).is_none());
        cache.put(&key, &value).unwrap();
        let cached = cache.get(&key).unwrap();
        assert_eq!(cached, value);
    }

    #[test]
    fn analysis_cache_miss_on_parser_version_change() {
        let tmp = TempDir::new();
        let cache = AnalysisCache::new(tmp.path()).unwrap();

        let key1 = CacheKey {
            file_hash: "abc".to_string(),
            parser_version: "oxc-0.140.0".to_string(),
            tool_version: "0.1.0".to_string(),
            cache_version: CACHE_VERSION,
        };
        let value = serde_json::json!({"module": "src/index.ts", "symbols": []});
        cache.put(&key1, &value).unwrap();

        let key2 = CacheKey {
            file_hash: key1.file_hash.clone(),
            parser_version: "oxc-0.140.1".to_string(),
            tool_version: key1.tool_version.clone(),
            cache_version: key1.cache_version,
        };
        assert!(cache.get(&key2).is_none());
    }

    #[test]
    fn analysis_cache_creates_sharded_directory() {
        let tmp = TempDir::new();
        let cache = AnalysisCache::new(tmp.path()).unwrap();

        let key = CacheKey {
            file_hash: "abc".to_string(),
            parser_version: "oxc-0.140.0".to_string(),
            tool_version: "0.1.0".to_string(),
            cache_version: CACHE_VERSION,
        };
        cache.put(&key, &serde_json::json!({})).unwrap();

        let path = cache.entry_path(&key);
        assert!(path.exists());
        assert_eq!(
            path.parent().unwrap().parent().unwrap(),
            tmp.path().join(".migration-cache")
        );
    }
}
