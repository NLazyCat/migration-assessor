use std::path::{Component, Path, PathBuf};

use oxc_span::SourceType;

/// Normalize `.` and `..` path segments without touching the filesystem.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    result.push("..");
                }
            }
            other => result.push(other),
        }
    }
    result
}

/// Detect the oxc SourceType from a file path extension.
pub fn detect_source_type(file_path: Option<&Path>) -> SourceType {
    match file_path.and_then(|p| p.extension().and_then(|e| e.to_str())) {
        Some("tsx") => SourceType::tsx(),
        Some("ts") => SourceType::ts(),
        Some("mts") | Some("cts") => SourceType::default()
            .with_typescript(true)
            .with_module(true),
        _ => SourceType::ts(),
    }
}
