use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::graph::{Cycle, CycleDetectionResult, Edge};
use crate::scores::ModuleReadiness;

// ── Types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub file_path: String,
    pub layer: i64,
    pub status: String,
    pub migration_effort: String,
    pub score: f64,
    pub in_degree: i64,
    pub total_modules: usize,
    pub completed_count: usize,
    pub verified_count: usize,
    pub failed_count: usize,
    pub pending_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressSummary {
    pub total: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub verified: usize,
    pub failed: usize,
    pub completion_percentage: f64,
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Open or create the SQLite database at `db_path`.
/// Runs schema migrations automatically.
pub fn open_or_create(db_path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Write a batch of module readiness scores (inside a transaction).
pub fn write_modules(conn: &Connection, modules: &[ModuleReadiness]) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch("DELETE FROM modules;")?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO modules (path, layer, score, rank, in_degree, complexity,
             external_compatibility, cycle_count, has_tests, migration_effort)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for m in modules {
            stmt.execute(params![
                m.module,
                0,
                m.score,
                m.rank as i64,
                m.in_degree as i64,
                m.complexity,
                m.external_compatibility,
                m.cycle_count as i64,
                m.has_tests,
                m.migration_effort,
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Write dependency edges (inside a transaction).
pub fn write_edges(conn: &Connection, edges: &[Edge]) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch("DELETE FROM edges;")?;
    {
        let mut stmt = tx.prepare("INSERT INTO edges (source, target) VALUES (?1, ?2)")?;
        for e in edges {
            stmt.execute(params![e.from, e.to])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Write detected cycles (inside a transaction).
pub fn write_cycles(conn: &Connection, cycles: &CycleDetectionResult) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch("DELETE FROM cycles;")?;
    {
        let mut stmt = tx.prepare("INSERT INTO cycles (members) VALUES (?1)")?;
        for c in &cycles.cycles {
            let members = serde_json::to_string(&c.nodes)?;
            stmt.execute(params![members])?;
        }
    }
    tx.execute(
        "INSERT OR REPLACE INTO metadata (key, value) VALUES ('cycle_has_cycles', ?1)",
        params![if cycles.has_cycles { "true" } else { "false" }],
    )?;
    let self_loops_json = serde_json::to_string(&cycles.self_loops)?;
    tx.execute(
        "INSERT OR REPLACE INTO metadata (key, value) VALUES ('cycle_self_loops', ?1)",
        params![self_loops_json],
    )?;
    tx.commit()?;
    Ok(())
}

/// Write a metadata key-value pair.
pub fn write_metadata(conn: &Connection, key: &str, value: &str) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

// ── Read operations (used by boundaries, summary) ────────────────────────────

/// Read all modules ordered by rank.
pub fn read_modules(conn: &Connection) -> anyhow::Result<Vec<ModuleReadiness>> {
    let mut stmt = conn.prepare(
        "SELECT path, score, rank, in_degree, complexity, external_compatibility,
         cycle_count, has_tests, migration_effort
         FROM modules ORDER BY rank ASC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ModuleReadiness {
            module: row.get(0)?,
            score: row.get(1)?,
            rank: row.get::<_, i64>(2)? as usize,
            in_degree: row.get::<_, i64>(3)? as usize,
            complexity: row.get(4)?,
            external_compatibility: row.get(5)?,
            cycle_count: row.get::<_, i64>(6)? as usize,
            has_tests: row.get(7)?,
            migration_effort: row.get(8)?,
            breakdown: crate::scores::ScoreBreakdown {
                in_degree_score: 0.0,
                complexity_score: 0.0,
                external_compatibility_score: 0.0,
                cycle_score: 0.0,
                test_coverage_score: 0.0,
            },
        })
    })?;
    let mut modules = Vec::new();
    for row in rows {
        modules.push(row?);
    }
    Ok(modules)
}

/// Read all edges.
pub fn read_edges(conn: &Connection) -> anyhow::Result<Vec<Edge>> {
    let mut stmt = conn.prepare("SELECT source, target FROM edges")?;
    let rows = stmt.query_map([], |row| {
        Ok(Edge {
            from: row.get(0)?,
            to: row.get(1)?,
        })
    })?;
    let mut edges = Vec::new();
    for row in rows {
        edges.push(row?);
    }
    Ok(edges)
}

/// Read cycle detection result.
pub fn read_cycles(conn: &Connection) -> anyhow::Result<CycleDetectionResult> {
    let mut stmt = conn.prepare("SELECT members FROM cycles")?;
    let cycles: Vec<Cycle> = stmt
        .query_map([], |row| {
            let members: String = row.get(0)?;
            let nodes: Vec<String> = serde_json::from_str(&members).unwrap_or_default();
            Ok(Cycle { nodes })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let has_cycles = !cycles.is_empty();
    let self_loops: Vec<String> = conn
        .query_row(
            "SELECT value FROM metadata WHERE key = 'cycle_self_loops'",
            [],
            |row| {
                let val: String = row.get(0)?;
                Ok(serde_json::from_str(&val).unwrap_or_default())
            },
        )
        .unwrap_or_default();

    Ok(CycleDetectionResult {
        has_cycles,
        cycles,
        self_loops,
    })
}

/// Read a metadata value by key.
pub fn read_metadata(conn: &Connection, key: &str) -> anyhow::Result<Option<String>> {
    let mut stmt = conn.prepare("SELECT value FROM metadata WHERE key = ?1")?;
    let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
    match rows.next() {
        Some(Ok(val)) => Ok(Some(val)),
        _ => Ok(None),
    }
}

// ── Task progress operations ────────────────────────────────────────────────

/// Initialize the task queue: insert all modules from the `modules` table
/// as pending tasks, ordered by layer then score (descending).
pub fn init_task_queue(conn: &Connection) -> anyhow::Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute_batch("DELETE FROM task_progress;")?;
    tx.execute_batch(
        "INSERT OR IGNORE INTO task_progress (file_path, layer, status, migration_effort, score, in_degree)
         SELECT path, layer, 'pending', migration_effort, score, in_degree
         FROM modules
         ORDER BY layer ASC, in_degree DESC;",
    )?;
    tx.commit()?;
    Ok(())
}

/// Get the next pending task (lowest layer, highest in_degree first).
pub fn next_pending_task(conn: &Connection) -> anyhow::Result<Option<TaskInfo>> {
    let task = conn
        .query_row(
            "SELECT t.file_path, t.layer, t.status, t.migration_effort, t.score, t.in_degree
             FROM task_progress t
             WHERE t.status = 'pending'
             ORDER BY t.layer ASC, t.in_degree DESC
             LIMIT 1",
            [],
            |row| {
                Ok(TaskInfo {
                    file_path: row.get(0)?,
                    layer: row.get(1)?,
                    status: row.get(2)?,
                    migration_effort: row.get(3)?,
                    score: row.get(4)?,
                    in_degree: row.get(5)?,
                    total_modules: 0,
                    completed_count: 0,
                    verified_count: 0,
                    failed_count: 0,
                    pending_count: 0,
                })
            },
        )
        .ok();
    // Fill in aggregate counts
    if let Some(mut t) = task {
        let total: usize =
            conn.query_row("SELECT COUNT(*) FROM task_progress", [], |row| row.get(0))?;
        let pending: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?;
        let verified: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'verified'",
            [],
            |row| row.get(0),
        )?;
        let failed: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )?;
        t.total_modules = total;
        t.pending_count = pending;
        t.verified_count = verified;
        t.failed_count = failed;
        t.completed_count = verified + failed;
        Ok(Some(t))
    } else {
        Ok(None)
    }
}

/// List all tasks with optional status filter. Supports "many tasks at once".
pub fn list_tasks(conn: &Connection, status_filter: Option<&str>) -> anyhow::Result<Vec<TaskInfo>> {
    let (sql, param_val): (String, Option<String>) = match status_filter {
        Some(s) => (
            "SELECT t.file_path, t.layer, t.status, t.migration_effort, t.score, t.in_degree
             FROM task_progress t WHERE t.status = ?1
             ORDER BY t.layer ASC, t.in_degree DESC"
                .to_string(),
            Some(s.to_string()),
        ),
        None => (
            "SELECT t.file_path, t.layer, t.status, t.migration_effort, t.score, t.in_degree
             FROM task_progress t
             ORDER BY t.layer ASC, t.in_degree DESC"
                .to_string(),
            None,
        ),
    };
    let mut stmt = conn.prepare(&sql)?;
    // query_map closures must be unified
    let params: Vec<String> = match &param_val {
        Some(val) => vec![val.clone()],
        None => vec![],
    };
    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        Ok(TaskInfo {
            file_path: row.get(0)?,
            layer: row.get(1)?,
            status: row.get(2)?,
            migration_effort: row.get(3)?,
            score: row.get(4)?,
            in_degree: row.get(5)?,
            total_modules: 0,
            completed_count: 0,
            verified_count: 0,
            failed_count: 0,
            pending_count: 0,
        })
    })?;
    let mut tasks = Vec::new();
    for row in rows {
        tasks.push(row?);
    }
    // Fill in aggregate counts (same for all tasks)
    if !tasks.is_empty() {
        let total: usize =
            conn.query_row("SELECT COUNT(*) FROM task_progress", [], |row| row.get(0))?;
        let pending: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?;
        let verified: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'verified'",
            [],
            |row| row.get(0),
        )?;
        let failed: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )?;
        for t in &mut tasks {
            t.total_modules = total;
            t.pending_count = pending;
            t.verified_count = verified;
            t.failed_count = failed;
            t.completed_count = verified + failed;
        }
    }
    Ok(tasks)
}

/// Mark a task as in_progress.
pub fn start_task(conn: &Connection, file_path: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE task_progress SET status = 'in_progress' WHERE file_path = ?1",
        params![file_path],
    )?;
    Ok(())
}

/// Batch-mark one or more tasks as done (verified).
/// Resets `in_progress` status for the given files if they were in progress.
pub fn mark_tasks_done(conn: &Connection, file_paths: &[&str]) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let tx = conn.unchecked_transaction()?;
    {
        let mut stmt = tx.prepare(
            "UPDATE task_progress
             SET status = 'verified', verified_at = ?1, similarity = 1.0,
                 failure_count = failure_count
             WHERE file_path = ?2 AND status != 'verified'",
        )?;
        for fp in file_paths {
            stmt.execute(params![now, fp])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Record a verification result for a task.
pub fn record_verification(
    conn: &Connection,
    file_path: &str,
    passed: bool,
    similarity: f64,
) -> anyhow::Result<()> {
    let status = if passed { "verified" } else { "failed" };
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE task_progress
         SET status = ?1, verified_at = ?2, similarity = ?3,
             failure_count = CASE WHEN ?4 THEN failure_count + 1 ELSE failure_count END
         WHERE file_path = ?5",
        params![status, now, similarity, !passed, file_path],
    )?;
    Ok(())
}

/// Get overall progress summary.
pub fn get_progress(conn: &Connection) -> anyhow::Result<ProgressSummary> {
    let total: usize =
        conn.query_row("SELECT COUNT(*) FROM task_progress", [], |row| row.get(0))?;
    let pending: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_progress WHERE status = 'pending'",
        [],
        |row| row.get(0),
    )?;
    let in_progress: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_progress WHERE status = 'in_progress'",
        [],
        |row| row.get(0),
    )?;
    let verified: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_progress WHERE status = 'verified'",
        [],
        |row| row.get(0),
    )?;
    let failed: usize = conn.query_row(
        "SELECT COUNT(*) FROM task_progress WHERE status = 'failed'",
        [],
        |row| row.get(0),
    )?;
    let pct = if total > 0 {
        (verified as f64 + failed as f64) / total as f64 * 100.0
    } else {
        0.0
    };
    Ok(ProgressSummary {
        total,
        pending,
        in_progress,
        verified,
        failed,
        completion_percentage: pct,
    })
}

/// Get the current in_progress task.
pub fn get_current_task(conn: &Connection) -> anyhow::Result<Option<TaskInfo>> {
    let task = conn
        .query_row(
            "SELECT t.file_path, t.layer, t.status, t.migration_effort, t.score, t.in_degree
             FROM task_progress t
             WHERE t.status = 'in_progress'
             LIMIT 1",
            [],
            |row| {
                Ok(TaskInfo {
                    file_path: row.get(0)?,
                    layer: row.get(1)?,
                    status: row.get(2)?,
                    migration_effort: row.get(3)?,
                    score: row.get(4)?,
                    in_degree: row.get(5)?,
                    total_modules: 0,
                    completed_count: 0,
                    verified_count: 0,
                    failed_count: 0,
                    pending_count: 0,
                })
            },
        )
        .ok();
    // Fill in aggregate counts if found
    if let Some(mut t) = task {
        let total: usize =
            conn.query_row("SELECT COUNT(*) FROM task_progress", [], |row| row.get(0))?;
        let pending: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'pending'",
            [],
            |row| row.get(0),
        )?;
        let verified: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'verified'",
            [],
            |row| row.get(0),
        )?;
        let failed: usize = conn.query_row(
            "SELECT COUNT(*) FROM task_progress WHERE status = 'failed'",
            [],
            |row| row.get(0),
        )?;
        t.total_modules = total;
        t.pending_count = pending;
        t.verified_count = verified;
        t.failed_count = failed;
        t.completed_count = verified + failed;
        Ok(Some(t))
    } else {
        Ok(None)
    }
}

// ── Schema ──────────────────────────────────────────────────────────────────

const SCHEMA_SQL: &str = "
CREATE TABLE IF NOT EXISTS metadata (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS modules (
    path                    TEXT PRIMARY KEY,
    layer                   INTEGER NOT NULL DEFAULT 0,
    score                   REAL NOT NULL,
    rank                    INTEGER NOT NULL,
    in_degree               INTEGER NOT NULL DEFAULT 0,
    complexity              REAL NOT NULL DEFAULT 0.0,
    external_compatibility  REAL NOT NULL DEFAULT 1.0,
    cycle_count             INTEGER NOT NULL DEFAULT 0,
    has_tests               INTEGER NOT NULL DEFAULT 0,
    migration_effort        TEXT NOT NULL DEFAULT 'unknown'
);

CREATE TABLE IF NOT EXISTS edges (
    source  TEXT NOT NULL,
    target  TEXT NOT NULL,
    PRIMARY KEY (source, target)
);

CREATE TABLE IF NOT EXISTS cycles (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    members TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_progress (
    file_path     TEXT PRIMARY KEY,
    layer         INTEGER NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending',
    migration_effort TEXT NOT NULL DEFAULT 'unknown',
    score         REAL NOT NULL DEFAULT 0.0,
    in_degree     INTEGER NOT NULL DEFAULT 0,
    verified_at   TEXT,
    similarity    REAL,
    failure_count INTEGER NOT NULL DEFAULT 0
);";

fn migrate(conn: &Connection) -> anyhow::Result<()> {
    let version: i64 = conn
        .query_row(
            "SELECT COALESCE((SELECT value FROM metadata WHERE key = 'schema_version'), '0')",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "0".to_string())
        .parse()
        .unwrap_or(0);

    if version < 1 {
        conn.execute_batch(SCHEMA_SQL)?;
        write_metadata(conn, "schema_version", "1")?;
    }

    if version < 2 {
        // Add task_progress table
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS task_progress (
                file_path     TEXT PRIMARY KEY,
                layer         INTEGER NOT NULL,
                status        TEXT NOT NULL DEFAULT 'pending',
                migration_effort TEXT NOT NULL DEFAULT 'unknown',
                score         REAL NOT NULL DEFAULT 0.0,
                verified_at   TEXT,
                similarity    REAL,
                failure_count INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        write_metadata(conn, "schema_version", "2")?;
    }

    if version < 3 {
        // Add in_degree column to task_progress (may already exist in fresh DBs)
        let _ = conn.execute_batch(
            "ALTER TABLE task_progress ADD COLUMN in_degree INTEGER NOT NULL DEFAULT 0;",
        );
        write_metadata(conn, "schema_version", "3")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_or_create_creates_schema() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let conn = open_or_create(&db_path).unwrap();

        // Verify tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();
        assert!(tables.contains(&"metadata".to_string()));
        assert!(tables.contains(&"modules".to_string()));
        assert!(tables.contains(&"edges".to_string()));
        assert!(tables.contains(&"cycles".to_string()));
    }

    #[test]
    fn test_write_and_read_modules() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        let modules = vec![ModuleReadiness {
            module: "src/main.ts".into(),
            score: 85.0,
            rank: 1,
            in_degree: 3,
            complexity: 0.2,
            external_compatibility: 1.0,
            cycle_count: 0,
            has_tests: true,
            migration_effort: "trivial".into(),
            breakdown: crate::scores::ScoreBreakdown {
                in_degree_score: 25.0,
                complexity_score: 20.0,
                external_compatibility_score: 18.0,
                cycle_score: 15.0,
                test_coverage_score: 10.0,
            },
        }];

        write_modules(&conn, &modules).unwrap();
        let read = read_modules(&conn).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0].module, "src/main.ts");
        assert_eq!(read[0].score, 85.0);
        assert_eq!(read[0].rank, 1);
    }

    #[test]
    fn test_write_and_read_edges() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        let edges = vec![
            Edge {
                from: "a.ts".into(),
                to: "b.ts".into(),
            },
            Edge {
                from: "b.ts".into(),
                to: "c.ts".into(),
            },
        ];

        write_edges(&conn, &edges).unwrap();
        let read = read_edges(&conn).unwrap();
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].from, "a.ts");
    }

    #[test]
    fn test_write_and_read_cycles() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        let cycles = CycleDetectionResult {
            has_cycles: true,
            cycles: vec![Cycle {
                nodes: vec!["a.ts".into(), "b.ts".into()],
            }],
            self_loops: vec!["c.ts".into()],
        };

        write_cycles(&conn, &cycles).unwrap();
        let read = read_cycles(&conn).unwrap();
        assert!(read.has_cycles);
        assert_eq!(read.cycles.len(), 1);
        assert!(read.self_loops.contains(&"c.ts".to_string()));
    }

    #[test]
    fn test_write_and_read_metadata() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        write_metadata(&conn, "key1", "value1").unwrap();
        let val = read_metadata(&conn, "key1").unwrap();
        assert_eq!(val, Some("value1".into()));

        // Insert or replace
        write_metadata(&conn, "key1", "value2").unwrap();
        let val = read_metadata(&conn, "key1").unwrap();
        assert_eq!(val, Some("value2".into()));
    }

    #[test]
    fn test_init_task_queue_from_scores() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        // Insert some modules
        let modules = vec![
            ModuleReadiness {
                module: "src/a.ts".into(),
                score: 80.0,
                rank: 1,
                in_degree: 0,
                complexity: 0.1,
                external_compatibility: 1.0,
                cycle_count: 0,
                has_tests: true,
                migration_effort: "trivial".into(),
                breakdown: crate::scores::ScoreBreakdown::default(),
            },
            ModuleReadiness {
                module: "src/b.ts".into(),
                score: 60.0,
                rank: 2,
                in_degree: 1,
                complexity: 0.3,
                external_compatibility: 0.8,
                cycle_count: 0,
                has_tests: false,
                migration_effort: "moderate".into(),
                breakdown: crate::scores::ScoreBreakdown::default(),
            },
        ];
        write_modules(&conn, &modules).unwrap();

        init_task_queue(&conn).unwrap();

        // Check task count
        let count: usize =
            conn.query_row("SELECT COUNT(*) FROM task_progress", [], |row| row.get(0))
                .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_next_pending_task_returns_highest_in_degree() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        // Insert modules with different layers, scores and in_degree
        let modules = vec![
            ModuleReadiness {
                module: "src/high.ts".into(),
                score: 50.0,
                rank: 2,
                in_degree: 2,  // higher in_degree → should come first
                complexity: 0.5,
                external_compatibility: 0.5,
                cycle_count: 0,
                has_tests: false,
                migration_effort: "heavy".into(),
                breakdown: crate::scores::ScoreBreakdown::default(),
            },
            ModuleReadiness {
                module: "src/low.ts".into(),
                score: 90.0,
                rank: 1,
                in_degree: 0,  // lower in_degree → should come second
                complexity: 0.1,
                external_compatibility: 1.0,
                cycle_count: 0,
                has_tests: true,
                migration_effort: "trivial".into(),
                breakdown: crate::scores::ScoreBreakdown::default(),
            },
        ];
        write_modules(&conn, &modules).unwrap();
        init_task_queue(&conn).unwrap();

        let next = next_pending_task(&conn).unwrap().expect("should have task");
        // Layer 0 (default) first, then highest in_degree
        assert_eq!(next.file_path, "src/high.ts");
        assert_eq!(next.migration_effort, "heavy");
        assert_eq!(next.total_modules, 2);
        assert_eq!(next.pending_count, 2);
    }

    #[test]
    fn test_record_verification_updates_status() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        // Insert a module and init queue
        let modules = vec![ModuleReadiness {
            module: "src/test.ts".into(),
            score: 75.0,
            rank: 1,
            in_degree: 0,
            complexity: 0.2,
            external_compatibility: 1.0,
            cycle_count: 0,
            has_tests: true,
            migration_effort: "trivial".into(),
            breakdown: crate::scores::ScoreBreakdown::default(),
        }];
        write_modules(&conn, &modules).unwrap();
        init_task_queue(&conn).unwrap();

        // Record verification: passed
        record_verification(&conn, "src/test.ts", true, 0.95).unwrap();
        let status: String = conn
            .query_row(
                "SELECT status FROM task_progress WHERE file_path = 'src/test.ts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(status, "verified");

        let progress = get_progress(&conn).unwrap();
        assert_eq!(progress.total, 1);
        assert_eq!(progress.verified, 1);
        assert_eq!(progress.completion_percentage, 100.0);
    }

    #[test]
    fn test_list_tasks_filter() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        let modules = vec![
            ModuleReadiness {
                module: "src/a.ts".into(),
                score: 80.0,
                rank: 1,
                in_degree: 0,
                complexity: 0.1,
                external_compatibility: 1.0,
                cycle_count: 0,
                has_tests: true,
                migration_effort: "trivial".into(),
                breakdown: crate::scores::ScoreBreakdown::default(),
            },
            ModuleReadiness {
                module: "src/b.ts".into(),
                score: 70.0,
                rank: 2,
                in_degree: 1,
                complexity: 0.2,
                external_compatibility: 1.0,
                cycle_count: 0,
                has_tests: false,
                migration_effort: "moderate".into(),
                breakdown: crate::scores::ScoreBreakdown::default(),
            },
        ];
        write_modules(&conn, &modules).unwrap();
        init_task_queue(&conn).unwrap();

        // Mark one as verified
        record_verification(&conn, "src/a.ts", true, 0.95).unwrap();

        let pending = list_tasks(&conn, Some("pending")).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].file_path, "src/b.ts");

        let verified = list_tasks(&conn, Some("verified")).unwrap();
        assert_eq!(verified.len(), 1);
        assert_eq!(verified[0].file_path, "src/a.ts");

        let all = list_tasks(&conn, None).unwrap();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_get_current_task() {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();

        let modules = vec![ModuleReadiness {
            module: "src/test.ts".into(),
            score: 75.0,
            rank: 1,
            in_degree: 0,
            complexity: 0.2,
            external_compatibility: 1.0,
            cycle_count: 0,
            has_tests: true,
            migration_effort: "trivial".into(),
            breakdown: crate::scores::ScoreBreakdown::default(),
        }];
        write_modules(&conn, &modules).unwrap();
        init_task_queue(&conn).unwrap();

        // No in_progress yet
        assert!(get_current_task(&conn).unwrap().is_none());

        start_task(&conn, "src/test.ts").unwrap();
        let current = get_current_task(&conn).unwrap().expect("should exist");
        assert_eq!(current.file_path, "src/test.ts");
        assert_eq!(current.status, "in_progress");
    }
}
