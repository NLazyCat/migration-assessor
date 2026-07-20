pub mod javascript;
pub mod module_map;
pub mod rust;
pub mod typescript;

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::project::SourceLanguage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedDependency {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub children: Vec<ResolvedDependency>,
    #[serde(default = "default_dep_type")]
    pub dep_type: String,
}

fn default_dep_type() -> String {
    "prod".to_string()
}

pub fn resolve_dependencies(
    root: &Path,
    source_language: SourceLanguage,
) -> anyhow::Result<Vec<ResolvedDependency>> {
    match source_language {
        SourceLanguage::TypeScript => typescript::resolve(root),
        SourceLanguage::Rust => rust::resolve(root),
        SourceLanguage::JavaScript => javascript::resolve(root),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SourceLanguage;

    #[test]
    fn test_resolved_dependency_defaults() {
        let dep = ResolvedDependency {
            name: "test".into(),
            version: "1.0".into(),
            resolved: None,
            dependencies: vec![],
            children: vec![],
            dep_type: "prod".into(),
        };
        assert_eq!(dep.name, "test");
        assert_eq!(dep.version, "1.0");
    }

    #[test]
    fn test_default_dep_type() {
        assert_eq!(default_dep_type(), "prod");
    }

    #[test]
    fn test_resolved_dependency_with_children() {
        let child = ResolvedDependency {
            name: "child-lib".into(),
            version: "0.5".into(),
            resolved: None,
            dependencies: vec![],
            children: vec![],
            dep_type: "prod".into(),
        };
        let parent = ResolvedDependency {
            name: "parent".into(),
            version: "2.0".into(),
            resolved: Some("https://example.com/parent".into()),
            dependencies: vec!["child-lib".into()],
            children: vec![child],
            dep_type: "prod".into(),
        };
        assert_eq!(parent.resolved.as_deref(), Some("https://example.com/parent"));
        assert_eq!(parent.children.len(), 1);
        assert_eq!(parent.children[0].name, "child-lib");
    }

    #[test]
    fn test_resolve_dependencies_ts() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), r#"{"dependencies": {"lodash": "^4.0"}}"#).unwrap();
        let deps = resolve_dependencies(dir.path(), SourceLanguage::TypeScript).unwrap();
        let lodash = deps.iter().find(|d| d.name == "lodash");
        assert!(lodash.is_some());
        assert_eq!(lodash.unwrap().version, "^4.0");
    }

    #[test]
    fn test_resolve_dependencies_ts_no_package_json() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = resolve_dependencies(dir.path(), SourceLanguage::TypeScript);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_dependencies_ts_with_dev_deps() {
        let dir = tempfile::TempDir::new().unwrap();
        let pkg = r#"{
            "dependencies": {"react": "^18.0"},
            "devDependencies": {"jest": "^29.0"}
        }"#;
        std::fs::write(dir.path().join("package.json"), pkg).unwrap();
        let deps = resolve_dependencies(dir.path(), SourceLanguage::TypeScript).unwrap();
        assert_eq!(deps.len(), 2);
        assert!(deps.iter().any(|d| d.name == "react" && d.dep_type == "prod"));
        assert!(deps.iter().any(|d| d.name == "jest" && d.dep_type == "dev"));
    }
}
