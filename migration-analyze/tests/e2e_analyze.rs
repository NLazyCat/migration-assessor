use assert_cmd::Command;
use predicates::prelude::*;

mod common;

#[test]
fn test_e2e_analyze_full_pipeline() {
    let (_tmp_dir, project_root) = common::setup_project();

    let bin = common::binary_path();
    let mut cmd = Command::new(&bin);
    cmd.current_dir(&project_root).arg("analyze");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Mirrored"))
        .stdout(predicate::str::contains("migration.db"))
        .stdout(predicate::str::contains("spec files"))
        .stdout(predicate::str::contains("Migration scores"));

    // Verify migration folder was created
    let migration_dir = project_root.join("test-migration");
    assert!(
        migration_dir.exists(),
        "test-migration folder should be created"
    );

    // Verify report directory structure
    let report_dir = migration_dir.join("report");
    assert!(report_dir.exists(), "report/ directory must exist");

    // Verify key report files (new output structure)
    for expected_file in &[
        "manifest.json",
        "migration.db",
        "index.html",
        "spec/migration_order.json",
    ] {
        let path = report_dir.join(expected_file);
        assert!(
            path.exists(),
            "Expected report file missing: {}",
            expected_file
        );
    }

    // Verify spec files exist for each source file
    for spec_file in &[
        "spec/src/types.ts.json",
        "spec/src/utils.ts.json",
        "spec/src/index.ts.json",
    ] {
        let path = report_dir.join(spec_file);
        assert!(
            path.exists(),
            "Expected spec file missing: {}",
            spec_file
        );
    }

    // manifest.json should list core metadata
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(report_dir.join("manifest.json")).expect("read manifest.json"),
    )
    .expect("parse manifest.json");
    assert_eq!(manifest["schemaVersion"], "1.0.0");
    assert!(manifest["generatedAt"].as_str().is_some());
    assert!(manifest["toolVersion"].as_str().is_some());
    assert!(manifest["files"]["database"].as_str().is_some());

    // Verify SQLite database contains data
    let conn = migration_core::db::open_or_create(&report_dir.join("migration.db")).unwrap();
    let modules = migration_core::db::read_modules(&conn).unwrap();
    assert!(!modules.is_empty(), "SQLite should have module scores");
    let edges = migration_core::db::read_edges(&conn).unwrap();
    assert!(!edges.is_empty(), "SQLite should have dependency edges");

    // Verify migration_order.json is valid
    let order_content =
        std::fs::read_to_string(report_dir.join("spec/migration_order.json")).expect("read migration_order.json");
    let order: serde_json::Value =
        serde_json::from_str(&order_content).expect("parse migration_order.json");
    assert!(
        order["order"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "migration_order.json should have a non-empty order array"
    );

    // Verify spec JSON has the expected fields
    let spec_content = std::fs::read_to_string(
        report_dir.join("spec/src/utils.ts.json"),
    ).expect("read spec");
    let spec: serde_json::Value =
        serde_json::from_str(&spec_content).expect("parse spec");
    assert_eq!(spec["file"], "src/utils.ts");
    assert_eq!(spec["target_path"], "src/utils.rs");
    assert!(
        spec["symbols"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "spec should have symbols"
    );
    assert!(spec["imports"]["relative"].as_array().map(|a| !a.is_empty()).unwrap_or(false),
        "spec should have relative imports for src/utils.ts");
    assert!(
        spec["source"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "spec should have source code embedded"
    );

    // Verify config was written to migration folder
    let mig_config = migration_dir.join("config").join("migration.toml");
    assert!(
        mig_config.exists(),
        "config/migration.toml in migration folder"
    );

    // Verify source files were mirrored
    let original_files_count =
        count_files_excluding(&migration_dir, &["report", "config", "diffs"]);
    assert!(
        original_files_count > 0,
        "Migration folder should have mirrored source files"
    );
}

#[test]
fn test_e2e_task_queue_after_analyze() {
    let (_tmp_dir, project_root) = common::setup_project();

    let bin = common::binary_path();
    let mut cmd = assert_cmd::Command::new(&bin);
    cmd.current_dir(&project_root).arg("analyze");
    cmd.assert().success();

    let migration_dir = project_root.join("test-migration");
    let report_dir = migration_dir.join("report");
    let db_path = report_dir.join("migration.db");
    assert!(db_path.exists(), "migration.db should exist");

    let conn = migration_core::db::open_or_create(&db_path).unwrap();

    // Initialize the task queue from modules
    migration_core::db::init_task_queue(&conn).unwrap();

    // Check task count matches module count
    let total: usize = conn
        .query_row("SELECT COUNT(*) FROM task_progress", [], |row| row.get(0))
        .unwrap();
    assert!(total > 0, "Task queue should have at least one task");

    // Get the first pending task (layer 0, highest score first)
    let task = migration_core::db::next_pending_task(&conn)
        .unwrap()
        .expect("should have a pending task");
    assert_eq!(task.status, "pending");
    assert_eq!(task.total_modules, total);

    // The first task should be a layer-0 module (types.ts has no deps)
    assert_eq!(task.file_path, "src/types.ts");

    // Verify the spec JSON exists and contains source code
    let spec_path = report_dir.join("spec").join("src/types.ts.json");
    assert!(spec_path.exists(), "spec file should exist");
    let spec_content = std::fs::read_to_string(&spec_path).unwrap();
    let spec: serde_json::Value = serde_json::from_str(&spec_content).unwrap();
    assert!(
        spec["source"].as_str().map(|s| !s.is_empty()).unwrap_or(false),
        "spec should contain source code"
    );
    assert_eq!(spec["target_path"], "src/types.rs");
}

fn count_files_excluding(dir: &std::path::Path, exclude: &[&str]) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if exclude.contains(&name) {
                continue;
            }
            if path.is_file() {
                count += 1;
            } else if path.is_dir() {
                count += count_files_excluding(&path, exclude);
            }
        }
    }
    count
}
