use crate::deps::ResolvedDependency;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityEntry {
    pub source_language: String,
    pub target_language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equivalent: Option<String>,
    pub compatibility: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub struct CompatibilityMatrix {
    source_language: String,
    target_language: String,
    built_in: HashMap<String, CompatibilityEntry>,
    overrides: HashMap<String, CompatibilityEntry>,
}

impl CompatibilityMatrix {
    pub fn new(source_language: String, target_language: String) -> Self {
        let mut built_in = HashMap::new();

        if source_language == "typescript" && target_language == "rust" {
            built_in.insert(
                "date-fns".to_string(),
                CompatibilityEntry {
                    source_language: source_language.clone(),
                    target_language: target_language.clone(),
                    equivalent: Some("chrono".to_string()),
                    compatibility: "partial".to_string(),
                    note: Some("Most date operations map; timezone handling differs.".to_string()),
                },
            );
            built_in.insert(
                "lodash".to_string(),
                CompatibilityEntry {
                    source_language: source_language.clone(),
                    target_language: target_language.clone(),
                    equivalent: None,
                    compatibility: "none".to_string(),
                    note: Some(
                        "Must be reimplemented or replaced with idiomatic Rust.".to_string(),
                    ),
                },
            );
            built_in.insert(
                "axios".to_string(),
                CompatibilityEntry {
                    source_language: source_language.clone(),
                    target_language: target_language.clone(),
                    equivalent: Some("reqwest".to_string()),
                    compatibility: "partial".to_string(),
                    note: Some("Async API differs; error handling models differ.".to_string()),
                },
            );
        }

        Self {
            source_language,
            target_language,
            built_in,
            overrides: HashMap::new(),
        }
    }

    pub fn load_overrides(&mut self, path: &Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(path)?;
        let override_file: CompatibilityOverrideFile = toml::from_str(&content)?;

        for (name, entry) in override_file.dependencies {
            self.overrides.insert(
                name,
                CompatibilityEntry {
                    source_language: self.source_language.clone(),
                    target_language: self.target_language.clone(),
                    equivalent: entry.equivalent,
                    compatibility: entry.compatibility,
                    note: entry.note,
                },
            );
        }

        Ok(())
    }

    pub fn evaluate(
        &self,
        dependencies: &[ResolvedDependency],
    ) -> HashMap<String, CompatibilityEntry> {
        let mut result = HashMap::new();

        for dep in dependencies {
            if let Some(entry) = self.overrides.get(&dep.name) {
                result.insert(dep.name.clone(), entry.clone());
            } else if let Some(entry) = self.built_in.get(&dep.name) {
                result.insert(dep.name.clone(), entry.clone());
            } else {
                result.insert(
                    dep.name.clone(),
                    CompatibilityEntry {
                        source_language: self.source_language.clone(),
                        target_language: self.target_language.clone(),
                        equivalent: None,
                        compatibility: "unknown".to_string(),
                        note: Some("No compatibility mapping available.".to_string()),
                    },
                );
            }
        }

        result
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CompatibilityOverrideFile {
    dependencies: HashMap<String, OverrideEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct OverrideEntry {
    equivalent: Option<String>,
    compatibility: String,
    note: Option<String>,
}
