pub mod analyze;
pub mod boundaries;
pub mod check_updates;
pub mod context;
pub mod diff;
pub mod init;
pub mod report;
pub mod summary;

use migration_core::util;
use std::path::{Path, PathBuf};

/// Resolve a user-supplied path to an absolute path WITHOUT using
/// `canonicalize()` (which on Windows produces `\\?\` extended-length
/// paths that break TOML strings and path-prefix operations).
fn resolve_project_path(input: &str) -> PathBuf {
    let p = std::path::Path::new(input);
    let resolved = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(p))
            .unwrap_or_else(|_| p.to_path_buf())
    };
    // Normalize `.` and `..` segments without hitting the filesystem
    util::normalize_path(&resolved)
}

/// Run a `git` command in `cwd` and return trimmed stdout on success.
fn run_git_cmd(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}
