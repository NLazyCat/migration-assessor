use crate::deps::ResolvedDependency;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Ordered compatibility level.
///
/// The ordering reflects desirability:
/// - `Full` > `Partial` > `None` > `Unknown`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityLevel {
    Unknown,
    None,
    Partial,
    Full,
}

impl PartialOrd for CompatibilityLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CompatibilityLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher discriminant = higher compatibility
        // Unknown=0, None=1, Partial=2, Full=3
        (*self as u8).cmp(&(*other as u8))
    }
}

impl CompatibilityLevel {
    /// Return a numeric score in [0.0, 1.0].
    ///
    /// - Full    → 1.0
    /// - Partial → 0.5
    /// - None    → 0.0
    /// - Unknown → 0.3 (conservative default)
    pub fn numeric_score(&self) -> f64 {
        match self {
            Self::Full => 1.0,
            Self::Partial => 0.5,
            Self::None => 0.0,
            Self::Unknown => 0.3,
        }
    }
}

impl std::fmt::Display for CompatibilityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "full"),
            Self::Partial => write!(f, "partial"),
            Self::None => write!(f, "none"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

impl Default for CompatibilityLevel {
    fn default() -> Self {
        Self::Unknown
    }
}

impl<'de> serde::Deserialize<'de> for CompatibilityLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_lowercase().as_str() {
            "full" => Ok(Self::Full),
            "partial" => Ok(Self::Partial),
            "none" => Ok(Self::None),
            _ => Ok(Self::Unknown),
        }
    }
}

impl Serialize for CompatibilityLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityEntry {
    pub source_language: String,
    pub target_language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equivalent: Option<String>,
    pub compatibility: CompatibilityLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepChangeInfo {
    pub package: String,
    pub old_version: Option<String>,
    pub new_version: Option<String>,
    pub change_type: String, // added, removed, upgraded, downgraded
    pub compatibility_before: Option<CompatibilityLevel>,
    pub compatibility_now: Option<CompatibilityLevel>,
    pub equivalent: Option<String>,
    pub needs_review: bool,
}

/// Describes the impact of a dependency change on the source codebase.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyImpact {
    /// The package name that changed.
    pub package: String,
    /// How many modules/files in the source repo import this dependency.
    pub affected_module_count: usize,
    /// Names of affected modules (first 20, truncated).
    pub affected_modules: Vec<String>,
    /// Whether the impact is considered high (many files affected + low compatibility).
    pub is_high_impact: bool,
}

impl Default for DependencyImpact {
    fn default() -> Self {
        Self {
            package: String::new(),
            affected_module_count: 0,
            affected_modules: vec![],
            is_high_impact: false,
        }
    }
}

/// Parse "source->target.package" TOML key into components.
fn parse_language_pair_key(key: &str) -> Option<(&str, &str, &str)> {
    // Expected format: "source->target.package"
    let arrow_pos = key.find("->")?;
    let source_lang = &key[..arrow_pos];
    let rest = &key[arrow_pos + 2..];

    let dot_pos = rest.find('.')?;
    let target_lang = &rest[..dot_pos];
    let package = &rest[dot_pos + 1..];

    if source_lang.is_empty() || target_lang.is_empty() || package.is_empty() {
        return None;
    }
    Some((source_lang, target_lang, package))
}

pub struct CompatibilityMatrix {
    source_language: String,
    target_language: String,
    built_in: HashMap<String, CompatibilityEntry>,
    overrides: HashMap<String, CompatibilityEntry>,
}

/// Raw toml representation of a single dependency mapping.
#[derive(Debug, Clone, Deserialize)]
struct RawEntry {
    equivalent: Option<String>,
    compatibility: Option<String>,
    note: Option<String>,
    tags: Option<Vec<String>>,
}

impl CompatibilityMatrix {
    /// Build a matrix keyed by `source->target.package`.
    ///
    /// Only entries matching `(source_language, target_language)` are loaded into
    /// the built-in map (keyed by package name alone for fast lookup).
    pub fn new(source_language: String, target_language: String) -> Self {
        let mut built_in = HashMap::new();

        let bundled = include_str!("compatibility_data.toml");
        if let Ok(table) = bundled.parse::<toml::Table>() {
            for (key, value) in table {
                let raw: RawEntry = match value.try_into() {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                // Parse the language-pair key format: "source->target.package"
                if let Some((src, tgt, pkg_name)) = parse_language_pair_key(&key) {
                    if src == source_language && tgt == target_language {
                        let compat_level = raw
                            .compatibility
                            .as_deref()
                            .map(parse_compatibility_str)
                            .unwrap_or(CompatibilityLevel::Unknown);

                        built_in.insert(
                            pkg_name.to_string(),
                            CompatibilityEntry {
                                source_language: source_language.clone(),
                                target_language: target_language.clone(),
                                equivalent: raw.equivalent,
                                compatibility: compat_level,
                                note: raw.note,
                                tags: raw.tags,
                            },
                        );
                    }
                }
            }
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
            let compat_level = entry
                .compatibility
                .as_deref()
                .map(parse_compatibility_str)
                .unwrap_or(CompatibilityLevel::Unknown);

            self.overrides.insert(
                name,
                CompatibilityEntry {
                    source_language: self.source_language.clone(),
                    target_language: self.target_language.clone(),
                    equivalent: entry.equivalent,
                    compatibility: compat_level,
                    note: entry.note,
                    tags: None,
                },
            );
        }

        Ok(())
    }

    /// Evaluate all given dependencies and return a per-package compatibility map.
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
                        compatibility: CompatibilityLevel::Unknown,
                        note: Some("No compatibility mapping available.".to_string()),
                        tags: None,
                    },
                );
            }
        }

        result
    }

    /// Look up a single dependency's compatibility entry.
    pub fn lookup(&self, package: &str) -> Option<&CompatibilityEntry> {
        self.overrides
            .get(package)
            .or_else(|| self.built_in.get(package))
    }

    /// Detect changes in dependencies and their compatibility impact.
    ///
    /// `old_deps` are from the analyzed version, `new_deps` from the latest version.
    ///
    /// The `needs_review` heuristic:
    /// - `added`: needs review if compatibility is unknown or none
    /// - `removed`: needs review if compatibility is full or partial (worth checking impact)
    /// - `upgraded`/`downgraded`: needs review if compatibility is partial or unknown
    pub fn detect_dep_changes(
        &self,
        old_deps: &[ResolvedDependency],
        new_deps: &[ResolvedDependency],
    ) -> Vec<DepChangeInfo> {
        let old_map: HashMap<&str, &ResolvedDependency> =
            old_deps.iter().map(|d| (d.name.as_str(), d)).collect();
        let new_map: HashMap<&str, &ResolvedDependency> =
            new_deps.iter().map(|d| (d.name.as_str(), d)).collect();

        let mut changes = Vec::new();
        let mut all_names: Vec<&str> = old_map.keys().chain(new_map.keys()).copied().collect();
        all_names.sort();
        all_names.dedup();

        for name in all_names {
            let old_dep = old_map.get(name);
            let new_dep = new_map.get(name);

            let change_type = match (old_dep, new_dep) {
                (None, Some(_)) => "added",
                (Some(_), None) => "removed",
                (Some(old), Some(new)) if old.version != new.version => {
                    if old.version < new.version {
                        "upgraded"
                    } else {
                        "downgraded"
                    }
                }
                _ => continue,
            };

            let old_entry = old_dep.and_then(|d| self.lookup(d.name.as_str()));
            let new_entry = new_dep.and_then(|d| self.lookup(d.name.as_str()));

            // Determine whether this change needs human review.
            let needs_review = match change_type {
                "added" => new_entry
                    .map(|e| {
                        e.compatibility == CompatibilityLevel::Unknown
                            || e.compatibility == CompatibilityLevel::None
                    })
                    // If no mapping exists, the new dep is unknown → needs review.
                    .unwrap_or(true),
                "removed" => {
                    // Removed deps need review only if they were fully compatible
                    // (might need replacement strategy). Unknown/none removals are expected.
                    old_entry
                        .map(|e| {
                            e.compatibility == CompatibilityLevel::Full
                                || e.compatibility == CompatibilityLevel::Partial
                        })
                        .unwrap_or(false)
                }
                // upgraded / downgraded
                _ => new_entry
                    .map(|e| {
                        e.compatibility == CompatibilityLevel::Partial
                            || e.compatibility == CompatibilityLevel::Unknown
                    })
                    .unwrap_or(true),
            };

            changes.push(DepChangeInfo {
                package: name.to_string(),
                old_version: old_dep.map(|d| d.version.clone()),
                new_version: new_dep.map(|d| d.version.clone()),
                change_type: change_type.to_string(),
                compatibility_before: old_entry.map(|e| e.compatibility),
                compatibility_now: new_entry.map(|e| e.compatibility),
                equivalent: new_entry.and_then(|e| e.equivalent.clone()),
                needs_review,
            });
        }

        changes
    }

    /// Analyze the impact of dependency changes on the source codebase.
    ///
    /// This takes the detected changes and a mapping of `(package → [module_names])`
    /// to determine how many files are affected by each change.
    pub fn analyze_impact(
        &self,
        dep_changes: &[DepChangeInfo],
        package_modules: &HashMap<String, Vec<String>>,
    ) -> Vec<DependencyImpact> {
        let mut impacts = Vec::new();

        for change in dep_changes {
            let affected_modules = package_modules
                .get(&change.package)
                .cloned()
                .unwrap_or_default();

            let affected_module_count = affected_modules.len();
            let compat = change
                .compatibility_now
                .unwrap_or(CompatibilityLevel::Unknown);
            let is_high_impact = affected_module_count > 5
                && (compat == CompatibilityLevel::Unknown
                    || compat == CompatibilityLevel::None);

            impacts.push(DependencyImpact {
                package: change.package.clone(),
                affected_module_count,
                affected_modules: affected_modules.into_iter().take(20).collect(),
                is_high_impact,
            });
        }

        impacts
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CompatibilityOverrideFile {
    dependencies: HashMap<String, OverrideEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct OverrideEntry {
    equivalent: Option<String>,
    compatibility: Option<String>,
    note: Option<String>,
}

fn parse_compatibility_str(s: &str) -> CompatibilityLevel {
    match s.trim().to_lowercase().as_str() {
        "full" => CompatibilityLevel::Full,
        "partial" => CompatibilityLevel::Partial,
        "none" => CompatibilityLevel::None,
        _ => CompatibilityLevel::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_language_pair_key_valid() {
        let (src, tgt, pkg) = parse_language_pair_key("typescript->rust.express").unwrap();
        assert_eq!(src, "typescript");
        assert_eq!(tgt, "rust");
        assert_eq!(pkg, "express");
    }

    #[test]
    fn test_parse_language_pair_key_invalid() {
        assert!(parse_language_pair_key("typescript").is_none());
        assert!(parse_language_pair_key("typescript->").is_none());
        assert!(parse_language_pair_key("->rust.express").is_none());
    }

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
        // Known entries should be present
        assert!(matrix.built_in.contains_key("express"));
        assert!(matrix.built_in.contains_key("axios"));
        assert!(matrix.built_in.contains_key("lodash"));
        // Rust→TS entries should NOT be loaded
        assert!(!matrix.built_in.contains_key("axum"));
        assert!(!matrix.built_in.contains_key("tokio"));
    }

    #[test]
    fn test_compatibility_matrix_loads_rust_to_ts_entries() {
        let matrix = CompatibilityMatrix::new("rust".to_string(), "typescript".to_string());
        assert!(matrix.built_in.contains_key("axum"));
        assert!(matrix.built_in.contains_key("tokio"));
        assert!(matrix.built_in.contains_key("serde"));
        // TS→Rust entries should NOT be loaded
        assert!(!matrix.built_in.contains_key("express"));
        assert!(!matrix.built_in.contains_key("axios"));
    }

    #[test]
    fn test_compatibility_level_default() {
        let entry = CompatibilityEntry {
            source_language: "typescript".to_string(),
            target_language: "rust".to_string(),
            equivalent: None,
            compatibility: CompatibilityLevel::Unknown,
            note: None,
            tags: None,
        };
        // Serialize and deserialize preserves unknown
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: CompatibilityEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.compatibility, CompatibilityLevel::Unknown);
    }

    #[test]
    fn test_detect_dep_changes_needs_review_heuristic() {
        let matrix = CompatibilityMatrix::new("typescript".to_string(), "rust".to_string());

        // Added dep with full compatibility → no review needed
        let changes = matrix.detect_dep_changes(
            &[],
            &[ResolvedDependency {
                name: "uuid".to_string(),
                version: "1.0.0".to_string(),
                resolved: None,
                dependencies: vec![],
                children: vec![],
                dep_type: "prod".to_string(),
            }],
        );
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, "added");
        assert!(!changes[0].needs_review); // uuid is "full" compatibility

        // Added dep with unknown compatibility → needs review
        let changes = matrix.detect_dep_changes(
            &[],
            &[ResolvedDependency {
                name: "unknown-pkg".to_string(),
                version: "1.0.0".to_string(),
                resolved: None,
                dependencies: vec![],
                children: vec![],
                dep_type: "prod".to_string(),
            }],
        );
        assert_eq!(changes.len(), 1);
        assert!(changes[0].needs_review);

        // Removed dep with full compat → needs review
        let changes = matrix.detect_dep_changes(
            &[ResolvedDependency {
                name: "uuid".to_string(),
                version: "1.0.0".to_string(),
                resolved: None,
                dependencies: vec![],
                children: vec![],
                dep_type: "prod".to_string(),
            }],
            &[],
        );
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, "removed");
        assert!(changes[0].needs_review); // uuid is "full" → removal matters

        // Removed dep with "none" compat → no review needed
        let changes = matrix.detect_dep_changes(
            &[ResolvedDependency {
                name: "lodash".to_string(),
                version: "4.0.0".to_string(),
                resolved: None,
                dependencies: vec![],
                children: vec![],
                dep_type: "prod".to_string(),
            }],
            &[],
        );
        assert_eq!(changes.len(), 1);
        assert!(!changes[0].needs_review); // lodash is "none" → expected removal

        // Upgraded dep with full compat → no review needed
        let changes = matrix.detect_dep_changes(
            &[ResolvedDependency {
                name: "uuid".to_string(),
                version: "1.0.0".to_string(),
                resolved: None,
                dependencies: vec![],
                children: vec![],
                dep_type: "prod".to_string(),
            }],
            &[ResolvedDependency {
                name: "uuid".to_string(),
                version: "2.0.0".to_string(),
                resolved: None,
                dependencies: vec![],
                children: vec![],
                dep_type: "prod".to_string(),
            }],
        );
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, "upgraded");
        assert!(!changes[0].needs_review); // uuid is "full", version bump only
    }

    #[test]
    fn test_analyze_impact() {
        let matrix = CompatibilityMatrix::new("typescript".to_string(), "rust".to_string());

        let changes = vec![DepChangeInfo {
            package: "axios".to_string(),
            old_version: Some("1.0.0".to_string()),
            new_version: Some("2.0.0".to_string()),
            change_type: "upgraded".to_string(),
            compatibility_before: Some(CompatibilityLevel::Partial),
            compatibility_now: Some(CompatibilityLevel::Partial),
            equivalent: Some("reqwest".to_string()),
            needs_review: true,
        }];

        let mut package_modules = HashMap::new();
        package_modules.insert(
            "axios".to_string(),
            vec![
                "src/http/client.ts".to_string(),
                "src/api/users.ts".to_string(),
            ],
        );

        let impacts = matrix.analyze_impact(&changes, &package_modules);
        assert_eq!(impacts.len(), 1);
        assert_eq!(impacts[0].package, "axios");
        assert_eq!(impacts[0].affected_module_count, 2);
        // 2 files, compat is Partial → not high impact
        assert!(!impacts[0].is_high_impact);
    }
}
