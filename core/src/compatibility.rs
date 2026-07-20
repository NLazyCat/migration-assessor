pub mod matching;
pub mod matrix;
pub mod types;

pub use matrix::*;
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compatibility_level_ordering() {
        assert!(CompatibilityLevel::Full > CompatibilityLevel::Partial);
        assert!(CompatibilityLevel::Partial > CompatibilityLevel::None);
        assert!(CompatibilityLevel::None > CompatibilityLevel::Unknown);
    }

    #[test]
    fn test_compatibility_level_numeric_score() {
        assert!((CompatibilityLevel::Full.numeric_score() - 1.0).abs() < 1e-6);
        assert!((CompatibilityLevel::Partial.numeric_score() - 0.5).abs() < 1e-6);
        assert!((CompatibilityLevel::None.numeric_score() - 0.0).abs() < 1e-6);
        assert!((CompatibilityLevel::Unknown.numeric_score() - 0.3).abs() < 1e-6);
    }

    #[test]
    fn test_parse_compatibility_str() {
        assert_eq!(parse_compatibility_str("full"), CompatibilityLevel::Full);
        assert_eq!(
            parse_compatibility_str("partial"),
            CompatibilityLevel::Partial
        );
        assert_eq!(parse_compatibility_str("none"), CompatibilityLevel::None);
        assert_eq!(
            parse_compatibility_str("anything_else"),
            CompatibilityLevel::Unknown
        );
        assert_eq!(parse_compatibility_str(""), CompatibilityLevel::Unknown);
    }

    #[test]
    fn test_compatibility_matrix_loads_ts_to_rust_entries() {
        let matrix = CompatibilityMatrix::new("typescript".to_string(), "rust".to_string());
        assert!(
            matrix.built_in.contains_key("express"),
            "express should be in TS registry"
        );
        assert!(
            matrix.built_in.contains_key("axios"),
            "axios should be in TS registry"
        );
        assert!(
            !matrix.built_in.contains_key("axum"),
            "axum is a Rust lib, not in TS registry"
        );
    }

    #[test]
    fn test_compatibility_matrix_loads_rust_to_ts_entries() {
        let matrix = CompatibilityMatrix::new("rust".to_string(), "typescript".to_string());
        assert!(
            matrix.built_in.contains_key("axum"),
            "axum should be in Rust registry"
        );
        assert!(
            matrix.built_in.contains_key("tokio"),
            "tokio should be in Rust registry"
        );
        assert!(
            !matrix.built_in.contains_key("express"),
            "express is a TS lib, not in Rust registry"
        );
    }

    #[test]
    fn test_detect_dep_changes_added() {
        let matrix = CompatibilityMatrix::new("typescript".to_string(), "rust".to_string());
        let old_deps = vec![];
        let new_deps = vec![crate::deps::ResolvedDependency {
            name: "express".to_string(),
            version: "4.18.0".to_string(),
            resolved: None,
            dependencies: vec![],
            children: vec![],
            dep_type: "prod".to_string(),
        }];

        let changes = matrix.detect_dep_changes(&old_deps, &new_deps);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].package, "express");
        assert_eq!(changes[0].change_type, "added");
    }

    #[test]
    fn test_detect_dep_changes_removed() {
        let matrix = CompatibilityMatrix::new("typescript".to_string(), "rust".to_string());
        let old_deps = vec![crate::deps::ResolvedDependency {
            name: "express".to_string(),
            version: "4.18.0".to_string(),
            resolved: None,
            dependencies: vec![],
            children: vec![],
            dep_type: "prod".to_string(),
        }];
        let new_deps = vec![];

        let changes = matrix.detect_dep_changes(&old_deps, &new_deps);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].package, "express");
        assert_eq!(changes[0].change_type, "removed");
    }
}