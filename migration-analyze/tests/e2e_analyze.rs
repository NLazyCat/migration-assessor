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
        .stdout(predicate::str::contains("References extracted"))
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

    // Verify key report files
    for expected_file in &[
        "manifest.json",
        "project.json",
        "overview.json",
        "index.html",
        "errors.json",
        "scores.json",
        "external/packages.json",
        "external/compatibility.json",
        "graph/nodes.json",
        "graph/edges.json",
        "graph/cycles.json",
    ] {
        let path = report_dir.join(expected_file);
        assert!(
            path.exists(),
            "Expected report file missing: {}",
            expected_file
        );
    }

    // manifest.json should list core files and all of them must exist
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(report_dir.join("manifest.json")).expect("read manifest.json"),
    )
    .expect("parse manifest.json");
    assert_eq!(manifest["schemaVersion"], "1.0.0");
    assert!(manifest["generatedAt"].as_str().is_some());
    assert!(manifest["toolVersion"].as_str().is_some());
    let files = manifest["files"]
        .as_object()
        .expect("manifest files object");
    for (_key, path_val) in files {
        let rel = path_val.as_str().expect("manifest file path string");
        assert!(
            report_dir.join(rel).exists(),
            "manifest lists missing file: {}",
            rel
        );
    }

    // Monolithic reference indexes should no longer be written
    assert!(
        !report_dir.join("references/forward.json").exists(),
        "references/forward.json should not exist"
    );
    assert!(
        !report_dir.join("references/reverse.json").exists(),
        "references/reverse.json should not exist"
    );

    // Per-file reference shards must exist for at least one module
    let forward_dir = report_dir.join("references").join("forward");
    let reverse_dir = report_dir.join("references").join("reverse");
    assert!(
        forward_dir.exists(),
        "references/forward/ directory must exist"
    );
    assert!(
        reverse_dir.exists(),
        "references/reverse/ directory must exist"
    );
    let shard_files: Vec<_> = walk_shard_files(&forward_dir);
    assert!(
        !shard_files.is_empty(),
        "At least one per-file reference shard should exist"
    );

    // Verify overview.json contains entries
    let index_content =
        std::fs::read_to_string(report_dir.join("overview.json")).expect("read overview.json");
    let index: serde_json::Value =
        serde_json::from_str(&index_content).expect("parse overview.json");
    assert!(
        index.as_object().map(|m| m.len()).unwrap_or(0) > 0,
        "overview.json should have at least one entry"
    );

    // Verify scores.json
    let scores_content =
        std::fs::read_to_string(report_dir.join("scores.json")).expect("read scores.json");
    let scores: serde_json::Value =
        serde_json::from_str(&scores_content).expect("parse scores.json");
    let scores_arr = scores.as_array().expect("scores should be an array");
    assert!(!scores_arr.is_empty(), "scores should not be empty");

    // Verify each score entry has expected fields
    for entry in scores_arr.iter().take(3) {
        assert!(
            entry.get("module").is_some(),
            "score entry should have 'module'"
        );
        assert!(
            entry.get("score").is_some(),
            "score entry should have 'score'"
        );
        assert!(
            entry.get("rank").is_some(),
            "score entry should have 'rank'"
        );
    }

    // Verify project.json metadata
    let project_meta =
        std::fs::read_to_string(report_dir.join("project.json")).expect("read project.json");
    let meta: serde_json::Value = serde_json::from_str(&project_meta).expect("parse project.json");
    assert_eq!(meta["sourceLanguage"], "typescript");
    assert_eq!(meta["targetLanguage"], "rust");
    assert!(meta["filesAnalyzed"].as_u64().unwrap_or(0) > 0);

    // Verify config was written to migration folder
    let mig_config = migration_dir.join("config").join("migration.toml");
    assert!(
        mig_config.exists(),
        "config/migration.toml in migration folder"
    );

    // Verify source files were mirrored
    let original_files_count =
        count_files_recursive(report_dir.parent().unwrap(), &["report", "config", "diffs"]);
    assert!(
        original_files_count > 0,
        "Migration folder should have mirrored source files"
    );
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

fn walk_shard_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut result = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(walk_shard_files(&path));
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".json") {
                    result.push(path);
                }
            }
        }
    }
    result
}
