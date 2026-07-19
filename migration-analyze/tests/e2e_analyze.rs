use assert_cmd::Command;
use predicates::prelude::*;

mod common;

#[test]
fn test_e2e_analyze_full_pipeline() {
    let (_tmp_dir, project_root) = common::setup_project();

    let bin = common::binary_path();
    let mut cmd = Command::new(&bin);
    cmd.current_dir(&project_root)
        .arg("analyze");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Mirrored"))
        .stdout(predicate::str::contains("References extracted"))
        .stdout(predicate::str::contains("readiness scores"));

    // Verify migration folder was created
    let migration_dir = project_root.join("test-migration");
    assert!(migration_dir.exists(), "test-migration folder should be created");

    // Verify report directory structure
    let report_dir = migration_dir.join("report");
    assert!(report_dir.exists(), "report/ directory must exist");

    // Verify key report files
    for expected_file in &[
        "project.json",
        "index.json",
        "errors.json",
        "scores.json",
        "external-deps/resolved.json",
        "external-deps/compatibility.json",
        "internal-deps/dag.json",
        "internal-deps/cycles.json",
        "references/forward.json",
        "references/reverse.json",
    ] {
        let path = report_dir.join(expected_file);
        assert!(path.exists(), "Expected report file missing: {}", expected_file);
    }

    // Verify index.json contains entries
    let index_content = std::fs::read_to_string(report_dir.join("index.json"))
        .expect("read index.json");
    let index: serde_json::Value = serde_json::from_str(&index_content)
        .expect("parse index.json");
    assert!(
        index.as_object().map(|m| m.len()).unwrap_or(0) > 0,
        "index.json should have at least one entry"
    );

    // Verify scores.json
    let scores_content = std::fs::read_to_string(report_dir.join("scores.json"))
        .expect("read scores.json");
    let scores: serde_json::Value = serde_json::from_str(&scores_content)
        .expect("parse scores.json");
    let scores_arr = scores.as_array().expect("scores should be an array");
    assert!(!scores_arr.is_empty(), "scores should not be empty");

    // Verify each score entry has expected fields
    for entry in scores_arr.iter().take(3) {
        assert!(entry.get("module").is_some(), "score entry should have 'module'");
        assert!(entry.get("score").is_some(), "score entry should have 'score'");
        assert!(entry.get("rank").is_some(), "score entry should have 'rank'");
    }

    // Verify project.json metadata
    let project_meta = std::fs::read_to_string(report_dir.join("project.json"))
        .expect("read project.json");
    let meta: serde_json::Value = serde_json::from_str(&project_meta)
        .expect("parse project.json");
    assert_eq!(meta["sourceLanguage"], "typescript");
    assert_eq!(meta["targetLanguage"], "rust");
    assert!(meta["filesAnalyzed"].as_u64().unwrap_or(0) > 0);

    // Verify config was written to migration folder
    let mig_config = migration_dir.join("config").join("migration.toml");
    assert!(mig_config.exists(), "config/migration.toml in migration folder");

    // Verify source files were mirrored
    let original_files_count = count_files_recursive(
        report_dir.parent().unwrap(),
        &["report", "config", "diffs"],
    );
    assert!(original_files_count > 0, "Migration folder should have mirrored source files");
}

fn count_files_recursive(dir: &std::path::Path, exclude: &[&str]) -> usize {
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
                count += count_files_recursive(&path, exclude);
            }
        }
    }
    count
}
