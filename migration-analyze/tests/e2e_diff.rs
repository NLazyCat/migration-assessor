use assert_cmd::Command;
use predicates::prelude::*;

mod common;

#[test]
fn test_e2e_diff_detects_changes_between_commits() {
    let (_tmp_dir, project_root) = common::setup_project();

    // First, run analyze to generate the base report
    let bin = common::binary_path();
    let mut analyze_cmd = Command::new(&bin);
    analyze_cmd.current_dir(&project_root).arg("analyze");
    analyze_cmd.assert().success();

    // Initialize a git repo in the test source with two commits
    let source_dir = project_root.join("test");

    run_git(&source_dir, &["init"]);
    run_git(&source_dir, &["add", "."]);
    run_git(
        &source_dir,
        &[
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=test",
            "commit",
            "-m",
            "initial",
        ],
    );

    // Make a change to a source file
    let index_path = source_dir.join("src").join("index.ts");
    let original_content = std::fs::read_to_string(&index_path).expect("read index.ts");
    let modified = original_content.replace(
        "// This line intentionally left blank for diff testing",
        "// DIFF TEST MARKER: changed for e2e diff test",
    );
    std::fs::write(&index_path, &modified).expect("write modified index.ts");

    run_git(&source_dir, &["add", "."]);
    run_git(
        &source_dir,
        &[
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=test",
            "commit",
            "-m",
            "modify index.ts",
        ],
    );

    // Get the commit hashes
    let from_hash = run_git(&source_dir, &["rev-parse", "HEAD~1"]).expect("get from commit hash");
    let to_hash = run_git(&source_dir, &["rev-parse", "HEAD"]).expect("get to commit hash");

    // Update migration.toml with source path (use forward slashes to avoid TOML escape issues)
    let source_path = source_dir.to_string_lossy().replace('\\', "/");
    let config_content = format!(
        r#"[project]
source = "{}"
source_repo = ""
source_branch = "main"
source_version = "{}"
target_language = "rust"
source_language = "typescript"

[skip]
framework = false
"#,
        source_path,
        from_hash.trim()
    );
    std::fs::write(project_root.join("migration.toml"), &config_content)
        .expect("write migration.toml");

    // Run diff
    let mut diff_cmd = Command::new(&bin);
    diff_cmd
        .current_dir(&project_root)
        .arg("diff")
        .arg("--new-version")
        .arg(to_hash.trim());

    diff_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Changed files"))
        .stdout(predicate::str::contains("Affected files"));

    // Verify diff output files
    let diff_dir = project_root.join("test-migration").join("diffs");
    assert!(diff_dir.exists(), "diffs/ directory must exist");

    // Check for diff report JSON file
    let diff_files: Vec<_> = std::fs::read_dir(&diff_dir)
        .expect("read diff dir")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("diff-") && n.ends_with(".json"))
                .unwrap_or(false)
        })
        .collect();
    assert!(
        !diff_files.is_empty(),
        "At least one diff report JSON should be created"
    );

    // Verify latest.json exists and mirrors the dated report
    let latest_path = diff_dir.join("latest.json");
    assert!(latest_path.exists(), "latest.json must exist");

    // Verify affected-files.json exists and has structure
    let affected_path = diff_dir.join("affected-files.json");
    assert!(affected_path.exists(), "affected-files.json must exist");
    let affected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&affected_path).expect("read affected"))
            .expect("parse affected-files.json");
    assert!(
        affected.get("triggered_by").is_some(),
        "affected-files.json should have 'triggered_by'"
    );
    assert!(
        affected.get("affected_files").is_some(),
        "affected-files.json should have 'affected_files'"
    );
}

fn run_git(dir: &std::path::Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}
