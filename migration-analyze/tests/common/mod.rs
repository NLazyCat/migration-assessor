use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Path to the calc-test fixture directory.
#[allow(dead_code)]
const FIXTURE_DIR: &str = r"C:\Users\16017\Documents\AI\calc-test";

/// Sets up a temporary project directory by copying the calc-test fixture.
///
/// Returns a TempDir containing:
///   - migration.toml (minimal config)
///   - test/ (source TypeScript repo)
///
/// The caller can then run `migration-analyze` commands inside this directory.
#[allow(dead_code)]
pub fn setup_project() -> (TempDir, PathBuf) {
    let fixture = PathBuf::from(FIXTURE_DIR);
    assert!(
        fixture.exists(),
        "Fixture directory not found: {}",
        fixture.display()
    );

    let tmp_dir = TempDir::new().expect("failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    // Copy the source TypeScript repo (excluding node_modules and .git)
    copy_dir_filtered(&fixture.join("test"), &project_root.join("test"), &[
        "node_modules",
        ".git",
    ]);

    // Create a minimal migration.toml
    let config = r#"# Migration Assessor Configuration
[project]
source = "test"
target_language = "rust"
source_language = "typescript"

[skip]
framework = false
"#;
    std::fs::write(project_root.join("migration.toml"), config).expect("failed to write migration.toml");

    (tmp_dir, project_root)
}

/// Copy a directory recursively, skipping entries whose names are in `exclude`.
#[allow(dead_code)]
fn copy_dir_filtered(src: &Path, dst: &Path, exclude: &[&str]) {
    std::fs::create_dir_all(dst).expect("failed to create dir");

    for entry in std::fs::read_dir(src).expect("failed to read dir") {
        let entry = entry.expect("failed to read entry");
        let name = entry.file_name();
        if exclude.contains(&name.to_str().unwrap_or("")) {
            continue;
        }
        let file_type = entry.file_type().expect("failed to get file type");
        if file_type.is_dir() {
            copy_dir_filtered(&entry.path(), &dst.join(&name), exclude);
        } else {
            std::fs::copy(&entry.path(), &dst.join(&name)).expect("failed to copy file");
        }
    }
}

/// Returns the path to the compiled `migration-analyze` binary.
/// Uses `cargo` to find the binary location.
pub fn binary_path() -> PathBuf {
    let output = std::process::Command::new("cargo")
        .args(["build", "-p", "migration-analyze", "--message-format=json"])
        .current_dir(r"C:\Users\16017\Documents\AI\migration-assessor")
        .output()
        .ok();

    if let Some(output) = output {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(line) {
                if obj.get("reason").and_then(|v| v.as_str()) == Some("compiler-artifact") {
                    if let Some(executable) = obj.get("executable").and_then(|v| v.as_str()) {
                        if executable.contains("migration-analyze") {
                            return PathBuf::from(executable);
                        }
                    }
                }
            }
        }
    }

    // Fallback: use the known debug build path
    let fallback = PathBuf::from(
        r"C:\Users\16017\Documents\AI\migration-assessor\target\debug\migration-analyze.exe",
    );
    assert!(fallback.exists(), "Binary not found at fallback path");
    fallback
}
