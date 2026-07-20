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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_normalize_path_removes_dot() {
        let p = Path::new("foo/./bar");
        assert_eq!(normalize_path(p), Path::new("foo/bar"));
    }

    #[test]
    fn test_normalize_path_resolves_double_dot() {
        let p = Path::new("foo/bar/../baz");
        assert_eq!(normalize_path(p), Path::new("foo/baz"));
    }

    #[test]
    fn test_normalize_path_unchanged() {
        let p = Path::new("a/b/c");
        assert_eq!(normalize_path(p), Path::new("a/b/c"));
    }

    #[test]
    fn test_normalize_path_empty_unchanged() {
        let p = Path::new("");
        assert_eq!(normalize_path(p), Path::new(""));
    }

    #[test]
    fn test_detect_source_type_ts() {
        let st = detect_source_type(Some(Path::new("foo.ts")));
        assert!(st.is_typescript());
        assert!(!st.is_jsx());
    }

    #[test]
    fn test_detect_source_type_tsx() {
        let st = detect_source_type(Some(Path::new("foo.tsx")));
        assert!(st.is_typescript());
        assert!(st.is_jsx());
    }

    #[test]
    fn test_detect_source_type_mts() {
        let st = detect_source_type(Some(Path::new("foo.mts")));
        assert!(st.is_typescript());
        assert!(st.is_module());
    }

    #[test]
    fn test_detect_source_type_none_fallback() {
        let st = detect_source_type(None);
        assert!(st.is_typescript());
    }
}
