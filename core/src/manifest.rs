use crate::db;
use crate::spec_writer::MigrationSpec;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

// ── Exported-symbols checklist ────────────────────────────────────────

/// Per-module symbol checklist entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleSymbolEntry {
    /// Source symbol name (TypeScript).
    pub source: String,
    /// Expected target symbol name (Rust).
    pub target: String,
    /// Symbol kind.
    pub kind: String,
    /// `true` if the symbol has been detected in generated code, `false` if
    /// checked and missing, `null` if not yet verified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub present: Option<bool>,
}

/// One module in the symbol checklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleChecklist {
    /// Source path (e.g. "src/types/api.ts").
    pub module: String,
    /// Target path (e.g. "src/types/api.rs").
    pub target: String,
    /// Migration layer.
    pub layer: usize,
    /// Migration effort label.
    pub effort: String,
    /// Migration status.
    pub status: String,
    /// Symbols this module should export.
    pub symbols: Vec<ModuleSymbolEntry>,
}

/// Full exported-symbols checklist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolChecklist {
    pub summary: String,
    pub modules: Vec<ModuleChecklist>,
}

// ── TODO list ─────────────────────────────────────────────────────────

/// One pending migration item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub module: String,
    pub target: String,
    pub layer: usize,
    pub effort: String,
    pub depends_on_count: usize,
}

/// AI-facing todo list — only shows what's left to do.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoList {
    pub summary: String,
    pub completion_pct: f64,
    pub remaining: Vec<TodoItem>,
}

// ── Module progress snapshot ──────────────────────────────────────────

/// Quick numeric progress snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleProgress {
    pub total: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub verified: usize,
    pub failed: usize,
    pub completion_pct: f64,
}

// ── Generation functions ─────────────────────────────────────────────

/// Build the full symbol checklist from spec files + DB progress.
pub fn build_symbol_checklist(
    db: &Connection,
    report_dir: &Path,
) -> anyhow::Result<SymbolChecklist> {
    let tasks = db::list_tasks(db, None)?;
    let mut modules = Vec::new();

    for task in &tasks {
        let spec_path = report_dir
            .join("spec")
            .join(format!("{}.json", task.file_path));
        let spec: MigrationSpec = match std::fs::read_to_string(&spec_path)
            .and_then(|s| serde_json::from_str(&s).map_err(|e| std::io::Error::other(e)))
        {
            Ok(s) => s,
            Err(_) => continue, // skip if spec not readable
        };

        // Determine if each symbol has been verified as present
        let symbols: Vec<ModuleSymbolEntry> = spec
            .symbols
            .iter()
            .map(|sym| ModuleSymbolEntry {
                source: sym.name.clone(),
                target: sym.target_name.clone(),
                kind: sym.kind.clone(),
                present: if task.status == "verified" {
                    Some(true)
                } else if task.status == "failed" {
                    Some(false)
                } else {
                    None
                },
            })
            .collect();

        modules.push(ModuleChecklist {
            module: task.file_path.clone(),
            target: task.file_path.replace(".ts", ".rs").replace(".js", ".rs"),
            layer: task.layer as usize,
            effort: task.migration_effort.clone(),
            status: task.status.clone(),
            symbols,
        });
    }

    let verified_count = tasks.iter().filter(|t| t.status == "verified").count();
    let total = tasks.len();
    let pct = if total > 0 {
        verified_count as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    Ok(SymbolChecklist {
        summary: format!(
            "{}/{} modules verified — {:2.1}% complete",
            verified_count, total, pct
        ),
        modules,
    })
}

/// Build the todo list — only pending / in-progress items.
pub fn build_todo_list(db: &Connection, report_dir: &Path) -> anyhow::Result<TodoList> {
    let tasks = db::list_tasks(db, None)?;
    let total = tasks.len();
    let verified = tasks.iter().filter(|t| t.status == "verified").count();

    // Collect remaining items (not yet verified)
    let remaining: Vec<TodoItem> = tasks
        .iter()
        .filter(|t| t.status != "verified")
        .map(|task| {
            // Read spec to get dependency info
            let spec_path = report_dir
                .join("spec")
                .join(format!("{}.json", task.file_path));
            let dep_count = std::fs::read_to_string(&spec_path)
                .ok()
                .and_then(|s| serde_json::from_str::<MigrationSpec>(&s).ok())
                .map(|spec| spec.imports.relative.len())
                .unwrap_or(0);

            TodoItem {
                module: task.file_path.clone(),
                target: task.file_path.replace(".ts", ".rs").replace(".js", ".rs"),
                layer: task.layer as usize,
                effort: task.migration_effort.clone(),
                depends_on_count: dep_count,
            }
        })
        .collect();

    let pct = if total > 0 {
        verified as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    Ok(TodoList {
        summary: format!(
            "{}/{} modules complete — {} remaining. Priority: migrate layer-0 modules first.",
            verified, total, remaining.len()
        ),
        completion_pct: pct,
        remaining,
    })
}

/// Build a quick numeric progress snapshot from the DB.
pub fn build_module_progress(db: &Connection) -> anyhow::Result<ModuleProgress> {
    let tasks = db::list_tasks(db, None)?;
    let total = tasks.len();
    let pending = tasks.iter().filter(|t| t.status == "pending").count();
    let in_progress = tasks.iter().filter(|t| t.status == "in_progress").count();
    let verified = tasks.iter().filter(|t| t.status == "verified").count();
    let failed = tasks.iter().filter(|t| t.status == "failed").count();

    let pct = if total > 0 {
        verified as f64 / total as f64 * 100.0
    } else {
        0.0
    };

    Ok(ModuleProgress {
        total,
        pending,
        in_progress,
        verified,
        failed,
        completion_pct: pct,
    })
}

/// Refresh all three manifest files on disk.
/// Call this after marking tasks completed so AI sees up-to-date data.
pub fn refresh_all(db: &Connection, report_dir: &Path) -> anyhow::Result<serde_json::Value> {
    std::fs::create_dir_all(report_dir.join("manifest"))?;

    let checklist = build_symbol_checklist(db, report_dir)?;
    std::fs::write(
        report_dir.join(crate::output_paths::manifest::SYMBOLS_CHECKLIST),
        serde_json::to_string_pretty(&checklist)?,
    )?;

    let todo = build_todo_list(db, report_dir)?;
    std::fs::write(
        report_dir.join(crate::output_paths::manifest::TODO_LIST),
        serde_json::to_string_pretty(&todo)?,
    )?;

    let progress = build_module_progress(db)?;
    std::fs::write(
        report_dir.join(crate::output_paths::manifest::MODULE_PROGRESS),
        serde_json::to_string_pretty(&progress)?,
    )?;

    Ok(serde_json::json!({
        "progress": serde_json::to_value(&progress)?,
        "todo_list": serde_json::to_value(&todo)?,
    }))
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, open_or_create};
    use crate::scores::{ModuleReadiness, ScoreBreakdown};
    use crate::spec_writer::MigrationSpec;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Connection) {
        let dir = TempDir::new().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();
        let modules = vec![
            ModuleReadiness {
                module: "src/types.ts".into(),
                score: 90.0,
                rank: 1,
                in_degree: 0,
                complexity: 0.1,
                external_compatibility: 1.0,
                cycle_count: 0,
                has_tests: false,
                migration_effort: "trivial".into(),
                breakdown: ScoreBreakdown::default(),
            },
            ModuleReadiness {
                module: "src/utils.ts".into(),
                score: 70.0,
                rank: 2,
                in_degree: 1,
                complexity: 0.3,
                external_compatibility: 1.0,
                cycle_count: 0,
                has_tests: true,
                migration_effort: "moderate".into(),
                breakdown: ScoreBreakdown::default(),
            },
        ];
        db::write_modules(&conn, &modules).unwrap();
        db::init_task_queue(&conn).unwrap();
        (dir, conn)
    }

    fn write_mini_spec(report_dir: &Path, file: &str, symbols: Vec<(String, String, String)>) {
        let spec_dir = report_dir.join("spec");
        std::fs::create_dir_all(&spec_dir).unwrap();

        let spec = MigrationSpec {
            file: file.into(),
            target_path: file.replace(".ts", ".rs"),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: symbols
                .into_iter()
                .map(|(name, target, kind)| crate::spec_writer::SpecSymbol {
                    name,
                    kind,
                    visibility: "Public".into(),
                    line_range: [1, 1],
                    signature: None,
                    target_name: target,
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                })
                .collect(),
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        let json = serde_json::to_string_pretty(&spec).unwrap();
        let spec_file = spec_dir.join(format!("{}.json", file));
        if let Some(parent) = spec_file.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(spec_file, json).unwrap();
    }

    #[test]
    fn test_build_symbol_checklist_all_pending() {
        let (_dir, conn) = setup();
        let report_dir = _dir.path().join("report");
        std::fs::create_dir_all(&report_dir).unwrap();

        write_mini_spec(
            &report_dir,
            "src/types.ts",
            vec![
                ("User".into(), "user".into(), "interface".into()),
                ("formatDate".into(), "format_date".into(), "function".into()),
            ],
        );
        write_mini_spec(&report_dir, "src/utils.ts", vec![]);

        let checklist = build_symbol_checklist(&conn, &report_dir).unwrap();
        assert_eq!(checklist.modules.len(), 2);
        assert!(checklist.summary.contains("0/2"));
        // types module should have 2 symbols (both null = not checked)
        let types = checklist.modules.iter().find(|m| m.module == "src/types.ts").unwrap();
        assert_eq!(types.symbols.len(), 2);
        assert!(types.symbols[0].present.is_none());
        assert!(types.symbols[1].present.is_none());
    }

    #[test]
    fn test_build_symbol_checklist_with_verified() {
        let (_dir, conn) = setup();
        let report_dir = _dir.path().join("report");
        std::fs::create_dir_all(&report_dir).unwrap();

        write_mini_spec(
            &report_dir,
            "src/types.ts",
            vec![("User".into(), "user".into(), "interface".into())],
        );
        write_mini_spec(&report_dir, "src/utils.ts", vec![]);

        // Mark types.ts as verified
        db::record_verification(&conn, "src/types.ts", true, 0.95).unwrap();

        let checklist = build_symbol_checklist(&conn, &report_dir).unwrap();
        let types = checklist.modules.iter().find(|m| m.module == "src/types.ts").unwrap();
        assert_eq!(types.status, "verified");
        assert_eq!(types.symbols[0].present, Some(true));
        assert!(checklist.summary.contains("1/2"));
    }

    #[test]
    fn test_build_todo_list() {
        let (_dir, conn) = setup();
        let report_dir = _dir.path().join("report");
        std::fs::create_dir_all(&report_dir).unwrap();

        write_mini_spec(&report_dir, "src/types.ts", vec![]);
        write_mini_spec(&report_dir, "src/utils.ts", vec![]);

        // Mark types as verified → only utils remains
        db::record_verification(&conn, "src/types.ts", true, 0.95).unwrap();

        let todo = build_todo_list(&conn, &report_dir).unwrap();
        assert_eq!(todo.remaining.len(), 1);
        assert_eq!(todo.remaining[0].module, "src/utils.ts");
        assert!(todo.summary.contains("1/2"));
    }

    #[test]
    fn test_build_module_progress() {
        let (_dir, conn) = setup();
        db::record_verification(&conn, "src/types.ts", true, 0.95).unwrap();

        let progress = build_module_progress(&conn).unwrap();
        assert_eq!(progress.total, 2);
        assert_eq!(progress.verified, 1);
        assert_eq!(progress.pending, 1);
        assert!((progress.completion_pct - 50.0).abs() < f64::EPSILON);
    }
}
