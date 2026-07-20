use std::path::{Path, PathBuf};

pub fn get_changed_files(project_root: &Path, from_version: &str, to_version: &str) -> anyhow::Result<Vec<String>> {
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

pub fn get_file_at_version(project_root: &Path, version: &str, file_path: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{}:{}", version, file_path)])
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Failed to get file {} at version {}", file_path, version));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub fn fetch_latest_version(repo: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["ls-remote", "--tags", repo])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git ls-remote failed for {}: {}", repo, stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    let mut tags: Vec<String> = Vec::new();
    for line in &lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let ref_str = parts[1];

        if let Some(tag) = ref_str.strip_prefix("refs/tags/") {
            if tag.ends_with("^{}") {
                continue;
            }
            tags.push(tag.to_string());
        }
    }

    tags.sort_by(|a, b| {
        let a_ver = a.trim_start_matches('v');
        let b_ver = b.trim_start_matches('v');
        let a_parts: Vec<&str> = a_ver.split('.').collect();
        let b_parts: Vec<&str> = b_ver.split('.').collect();

        for (ap, bp) in a_parts.iter().zip(b_parts.iter()) {
            match (ap.parse::<u64>(), bp.parse::<u64>()) {
                (Ok(an), Ok(bn)) if an != bn => return an.cmp(&bn),
                _ => {}
            }
        }
        a_parts.len().cmp(&b_parts.len()).then_with(|| a.cmp(b))
    });

    let latest = tags
        .last()
        .cloned()
        .or_else(|| {
            let head_output = std::process::Command::new("git")
                .args(["ls-remote", repo, "HEAD"])
                .output()
                .ok()?;
            if !head_output.status.success() {
                return None;
            }
            let stdout = String::from_utf8_lossy(&head_output.stdout);
            stdout.split_whitespace().next().map(|s| s.to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("No tags or refs found in remote {}", repo))?;

    Ok(latest)
}

pub fn create_temp_dir() -> anyhow::Result<PathBuf> {
    let base = std::env::temp_dir().join("_mig_diff");
    let mut i = 0u64;
    loop {
        let dir = base.join(i.to_string());
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
            return Ok(dir);
        }
        i += 1;
    }
}

pub fn is_analyzable_file(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "rs")
}