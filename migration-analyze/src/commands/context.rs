use migration_core::config::Config;
use migration_core::db;
use migration_core::output_paths;
use migration_core::util;
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[allow(dead_code)]
pub struct ProjectContext {
    pub project_root: PathBuf,
    pub migration_folder: PathBuf,
    pub report_dir: PathBuf,
    pub config: Config,
    // caches for lazily-loaded JSON
    project_meta: Mutex<Option<serde_json::Value>>,
    index: Mutex<Option<serde_json::Value>>,
    scores: Mutex<Option<serde_json::Value>>,
    dag: Mutex<Option<serde_json::Value>>,
    // SQLite connection (lazily opened)
    db: Mutex<Option<Connection>>,
}

impl ProjectContext {
    pub fn load(project_root: impl AsRef<Path>) -> anyhow::Result<Self> {
        // IMPORTANT: canonicalize() yields \\?\ extended paths on Windows
        // that break TOML reading and path-prefix logic.
        // The path is already resolved by the caller via resolve_project_path(),
        // so just normalize path components without hitting the filesystem.
        let project_root = util::normalize_path(project_root.as_ref());
        let migration_folder = Self::detect_migration_folder(&project_root)?;
        let report_dir = migration_folder.join("report");
        let config = if let Some(p) = Self::find_config(&project_root) {
            Config::load(&p)?
        } else {
            Config::default()
        };

        Ok(Self {
            project_root,
            migration_folder,
            report_dir,
            config,
            project_meta: Mutex::new(None),
            index: Mutex::new(None),
            scores: Mutex::new(None),
            dag: Mutex::new(None),
            db: Mutex::new(None),
        })
    }

    fn detect_migration_folder(project_root: &Path) -> anyhow::Result<PathBuf> {
        if let Ok(entries) = std::fs::read_dir(project_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with("-migration") && path.join("report").exists() {
                    return Ok(path);
                }
            }
        }
        anyhow::bail!(
            "No migration folder (*-migration/) found in {}",
            project_root.display()
        )
    }

    fn find_config(project_root: &Path) -> Option<PathBuf> {
        let p = project_root.join("migration.toml");
        if p.exists() { Some(p) } else { None }
    }

    /// Open or retrieve the cached SQLite database connection.
    pub fn db(&self) -> anyhow::Result<std::sync::MutexGuard<'_, Option<Connection>>> {
        let mut guard = self.db.lock().unwrap();
        if guard.is_some() {
            return Ok(guard);
        }
        let db_path = self.report_dir.join(output_paths::DB);
        if db_path.exists() {
            let conn = db::open_or_create(&db_path)?;
            *guard = Some(conn);
        }
        Ok(guard)
    }

    pub fn project_meta(&self) -> anyhow::Result<serde_json::Value> {
        Self::read_json_cached(
            &self.report_dir.join(output_paths::PROJECT),
            &self.project_meta,
        )
    }

    pub fn overview(&self) -> anyhow::Result<serde_json::Value> {
        Self::read_json_cached(&self.report_dir.join(output_paths::OVERVIEW), &self.index)
    }

    pub fn index(&self) -> anyhow::Result<serde_json::Value> {
        self.overview()
    }

    pub fn scores(&self) -> anyhow::Result<serde_json::Value> {
        Self::read_json_cached(&self.report_dir.join(output_paths::SCORES), &self.scores)
    }

    pub fn dag(&self) -> anyhow::Result<serde_json::Value> {
        if let Some(v) = self.dag.lock().unwrap().as_ref() {
            return Ok(v.clone());
        }
        let nodes: serde_json::Value = self.load_json(output_paths::graph::NODES)?;
        let edges: serde_json::Value = self.load_json(output_paths::graph::EDGES)?;
        let merged = serde_json::json!({ "nodes": nodes, "edges": edges });
        *self.dag.lock().unwrap() = Some(merged.clone());
        Ok(merged)
    }

    pub fn report_path(&self, relative: &str) -> PathBuf {
        self.report_dir.join(relative)
    }

    pub fn load_json<T: serde::de::DeserializeOwned>(&self, relative: &str) -> anyhow::Result<T> {
        let path = self.report_path(relative);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))
    }

    /// Load the aggregated reverse reference index from per-file shards.
    ///
    /// Each shard under `references/reverse/<path>.json` uses symbol-only
    /// top-level keys; this method reconstructs full `path:symbol` keys so the
    /// returned value has the same schema as the old monolithic
    /// `references/reverse.json`.
    pub fn load_reverse_index<T: serde::de::DeserializeOwned>(&self) -> anyhow::Result<T> {
        let mut aggregated = serde_json::Map::new();
        let base = self.report_path("references/reverse");
        Self::collect_reverse_shards(&base, &base, &mut aggregated)?;
        serde_json::from_value(serde_json::Value::Object(aggregated))
            .map_err(|e| anyhow::anyhow!("Failed to deserialize aggregated reverse index: {}", e))
    }

    fn collect_reverse_shards(
        base: &Path,
        current: &Path,
        out: &mut serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<()> {
        if !current.exists() {
            return Ok(());
        }
        for entry in std::fs::read_dir(current)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                Self::collect_reverse_shards(base, &path, out)?;
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.ends_with(".json") {
                continue;
            }
            let relative = path.strip_prefix(base).unwrap_or(&path);
            let relative_str = relative.to_string_lossy().replace('\\', "/");
            let Some(file_key) = relative_str.strip_suffix(".json") else {
                continue;
            };
            let text = std::fs::read_to_string(&path)
                .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
            let shard: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))?;
            if let Some(obj) = shard.as_object() {
                for (symbol, refs) in obj {
                    let full_key = format!("{}:{}", file_key, symbol);
                    out.insert(full_key, refs.clone());
                }
            }
        }
        Ok(())
    }

    fn read_json_cached(
        path: &Path,
        cache: &Mutex<Option<serde_json::Value>>,
    ) -> anyhow::Result<serde_json::Value> {
        if let Some(v) = cache.lock().unwrap().as_ref() {
            return Ok(v.clone());
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))?;
        *cache.lock().unwrap() = Some(value.clone());
        Ok(value)
    }
}


