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
