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

/// Extract a source code snippet from a file using 1-indexed line range (inclusive on both ends).
pub fn extract_source_snippet(source: &str, line_range: [usize; 2]) -> String {
    let [start, end] = line_range;
    if start == 0 || start > end {
        return String::new();
    }
    source
        .lines()
        .skip(start - 1)
        .take(end - start + 1)
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Detect the oxc SourceType from a file path extension.
pub fn detect_source_type(file_path: Option<&Path>) -> SourceType {
    file_path
        .map_or(SourceType::ts(), |p| SourceType::from_path(p).unwrap_or(SourceType::ts()))
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
    fn test_detect_source_type_js() {
        let st = detect_source_type(Some(Path::new("foo.js")));
        assert!(!st.is_typescript());
    }

    #[test]
    fn test_detect_source_type_jsx() {
        let st = detect_source_type(Some(Path::new("foo.jsx")));
        assert!(!st.is_typescript());
        assert!(st.is_jsx());
    }

    #[test]
    fn test_detect_source_type_mjs() {
        let st = detect_source_type(Some(Path::new("foo.mjs")));
        assert!(st.is_module());
    }

    #[test]
    fn test_detect_source_type_none_fallback() {
        let st = detect_source_type(None);
        assert!(st.is_typescript());
    }
}
