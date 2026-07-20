use std::path::Path;

/// List files changed between two git revisions.
pub fn get_changed_files(
    project_root: &Path,
    from_version: &str,
    to_version: &str,
) -> anyhow::Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", from_version, to_version])
        .current_dir(project_root)
        .output()?;

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(files)
}

/// List files with uncommitted changes (staged + unstaged) compared to HEAD.
pub fn get_uncommitted_files(project_root: &Path) -> anyhow::Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", "HEAD"])
        .current_dir(project_root)
        .output()?;

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(files)
}

/// Get the content of a file at a specific git revision.
pub fn get_file_at_version(
    project_root: &Path,
    version: &str,
    file_path: &str,
) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{}:{}", version, file_path)])
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!(
            "Failed to get file {} at version {}",
            file_path,
            version
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    fn setup_git_repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        Command::new("git").args(["init"]).current_dir(dir.path()).output().unwrap();
        Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("file1.ts"), "content v1").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "initial"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        // create a tag for the initial state
        Command::new("git")
            .args(["tag", "v1.0.0"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        fs::write(dir.path().join("file1.ts"), "content v2").unwrap();
        fs::write(dir.path().join("file2.ts"), "new file").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "second"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        Command::new("git")
            .args(["tag", "v2.0.0"])
            .current_dir(dir.path())
            .output()
            .unwrap();
        dir
    }

    #[test]
    fn test_get_changed_files() {
        let dir = setup_git_repo();
        let files = get_changed_files(dir.path(), "v1.0.0", "v2.0.0").unwrap();
        assert!(files.contains(&"file1.ts".to_string()));
        assert!(files.contains(&"file2.ts".to_string()));
    }

    #[test]
    fn test_get_changed_files_no_changes() {
        let dir = setup_git_repo();
        let files = get_changed_files(dir.path(), "v2.0.0", "v2.0.0").unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn test_get_file_at_version() {
        let dir = setup_git_repo();
        let old = get_file_at_version(dir.path(), "v1.0.0", "file1.ts").unwrap();
        assert_eq!(old.trim(), "content v1");
        let new = get_file_at_version(dir.path(), "v2.0.0", "file1.ts").unwrap();
        assert_eq!(new.trim(), "content v2");
    }

    #[test]
    fn test_get_file_at_version_new_file() {
        let dir = setup_git_repo();
        let content = get_file_at_version(dir.path(), "v2.0.0", "file2.ts").unwrap();
        assert_eq!(content.trim(), "new file");
    }

    #[test]
    fn test_get_file_at_version_nonexistent() {
        let dir = setup_git_repo();
        let result = get_file_at_version(dir.path(), "v1.0.0", "file2.ts");
        assert!(result.is_err());
    }
}
