use assert_cmd::Command;
use predicates::prelude::*;

mod common;

#[test]
fn test_e2e_init_creates_project_structure() {
    let tmp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let project_root = tmp_dir.path().join("my-project");
    std::fs::create_dir_all(&project_root).expect("failed to create project dir");

    let bin = common::binary_path();
    let mut cmd = Command::new(&bin);
    cmd.current_dir(&project_root)
        .arg("init")
        .arg("calc-migration")
        .arg("--target-lang=rust");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created project"));

    // Verify migration.toml was created
    let config_path = project_root.join("calc-migration").join("migration.toml");
    assert!(config_path.exists(), "migration.toml should be created");

    let config_content = std::fs::read_to_string(&config_path).expect("read config");
    assert!(config_content.contains("target_language"));
    assert!(config_content.contains("rust"));
}

#[test]
fn test_e2e_init_rejects_existing_directory() {
    let tmp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    // Pre-create the directory to trigger the "already exists" error
    std::fs::create_dir_all(project_root.join("existing")).expect("failed to create dir");

    let bin = common::binary_path();
    let mut cmd = Command::new(&bin);
    cmd.current_dir(&project_root).arg("init").arg("existing");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn test_e2e_init_writes_gitignore() {
    let tmp_dir = tempfile::TempDir::new().expect("failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();

    let bin = common::binary_path();
    let mut cmd = Command::new(&bin);
    cmd.current_dir(&project_root)
        .arg("init")
        .arg("my-app")
        .arg("--dir=.");

    cmd.assert().success();

    let gitignore_path = project_root.join("my-app").join(".gitignore");
    assert!(gitignore_path.exists(), ".gitignore should be created");

    let content = std::fs::read_to_string(&gitignore_path).expect("read gitignore");
    assert!(content.contains("*-migration/"));
}
