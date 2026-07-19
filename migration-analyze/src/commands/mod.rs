pub mod analyze;
pub mod boundaries;
pub mod context;
pub mod diff;
pub mod init;
pub mod serve;

use std::path::PathBuf;

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
    normalize_path_components(&resolved)
}

/// Normalize `.` and `..` segments in a path without using canonicalize.
fn normalize_path_components(path: &std::path::Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if result.file_name().is_some() {
                    result.pop();
                } else {
                    result.push("..");
                }
            }
            other => result.push(other),
        }
    }
    result
}
