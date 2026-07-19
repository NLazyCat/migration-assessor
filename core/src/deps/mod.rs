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
    }
}
