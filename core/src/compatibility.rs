use crate::deps::ResolvedDependency;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Ordered compatibility level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompatibilityLevel {
    #[default]
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
        (*self as u8).cmp(&(*other as u8))
    }
}

impl CompatibilityLevel {
    pub fn numeric_score(&self) -> f64 {
        match self {
            Self::Full => 1.0,
            Self::Partial => 0.5,
            Self::None => 0.0,
            Self::Unknown => 0.3,
        }
    }

    pub fn default_effort(&self) -> MigrationEffort {
        match self {
            Self::Full => MigrationEffort::Trivial,
            Self::Partial => MigrationEffort::Moderate,
            Self::None => MigrationEffort::Rewrite,
            Self::Unknown => MigrationEffort::Unknown,
        }
    }
}

fn derive_risk_tags(
    effort: MigrationEffort,
    compat: CompatibilityLevel,
    tags: Option<&[String]>,
    guidance: Option<&str>,
) -> Vec<String> {
    let mut risk: Vec<String> = Vec::new();

    match effort {
        MigrationEffort::Rewrite => risk.push("rewrite".to_string()),
        MigrationEffort::Heavy => risk.push("heavy".to_string()),
        _ => {}
    }
    if matches!(compat, CompatibilityLevel::None) {
        risk.push("no-equivalent".to_string());
    }
    if matches!(compat, CompatibilityLevel::Unknown) {
        risk.push("unverified".to_string());
    }

    if let Some(tags) = tags {
        for t in tags {
            let tl = t.to_lowercase();
            if matches!(
                tl.as_str(),
                "async" | "ffi" | "concurrency" | "unsafe" | "crypto" | "realtime" | "ssr"
            ) {
                risk.push(tl);
            }
        }
    }

    if let Some(g) = guidance
        && (g.to_lowercase().contains("unsafe")
            || g.to_lowercase().contains("ffi")
            || g.to_lowercase().contains("no direct"))
        && !risk.iter().any(|r| r == "ffi")
    {
        risk.push("ffi".to_string());
    }

    let mut seen = HashSet::new();
    risk.retain(|r| seen.insert(r.clone()));
    risk
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

/// Relative migration effort to port a dependency to the target language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MigrationEffort {
    #[default]
    Unknown,
    Trivial,
    Moderate,
    Heavy,
    Rewrite,
}

impl MigrationEffort {
    pub fn numeric_score(&self) -> f64 {
        match self {
            Self::Trivial => 0.15,
            Self::Moderate => 0.45,
            Self::Heavy => 0.75,
            Self::Rewrite => 1.0,
            Self::Unknown => 0.5,
        }
    }

    pub fn is_high_impact(&self) -> bool {
        matches!(self, Self::Heavy | Self::Rewrite)
    }
}

impl std::fmt::Display for MigrationEffort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Trivial => "trivial",
            Self::Moderate => "moderate",
            Self::Heavy => "heavy",
            Self::Rewrite => "rewrite",
            Self::Unknown => "unknown",
        };
        write!(f, "{}", s)
    }
}

impl<'de> serde::Deserialize<'de> for MigrationEffort {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(parse_effort_str(&s))
    }
}

impl Serialize for MigrationEffort {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_str())
    }
}

fn parse_effort_str(s: &str) -> MigrationEffort {
    match s.trim().to_lowercase().as_str() {
        "trivial" => MigrationEffort::Trivial,
        "moderate" => MigrationEffort::Moderate,
        "heavy" => MigrationEffort::Heavy,
        "rewrite" => MigrationEffort::Rewrite,
        _ => MigrationEffort::Unknown,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityEntry {
    pub source_language: String,
    pub target_language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equivalent: Option<String>,
    pub compatibility: CompatibilityLevel,
    #[serde(default)]
    pub effort: MigrationEffort,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub risk_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepChangeInfo {
    pub package: String,
    pub old_version: Option<String>,
    pub new_version: Option<String>,
    pub change_type: String,
    pub compatibility_before: Option<CompatibilityLevel>,
    pub compatibility_now: Option<CompatibilityLevel>,
    pub equivalent: Option<String>,
    pub needs_review: bool,
    #[serde(default)]
    pub effort: MigrationEffort,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub risk_tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyImpact {
    pub package: String,
    pub affected_module_count: usize,
    pub affected_modules: Vec<String>,
    pub is_high_impact: bool,
}

/// A single library entry in a language registry.
#[derive(Debug, Clone, Deserialize)]
struct LibraryEntry {
    #[serde(rename = "type")]
    lib_type: String,
    description: String,
    tags: Vec<String>,
}

/// Language-specific library registry loaded from bundled TOML.
struct LanguageRegistry {
    libraries: HashMap<String, LibraryEntry>,
}

impl LanguageRegistry {
    fn load(language: &str) -> Self {
        let data = match language {
            "typescript" => TS_LIBRARIES,
            "rust" => RUST_LIBRARIES,
            _ => {
                return Self {
                    libraries: HashMap::new(),
                };
            }
        };

        let mut libraries = HashMap::new();
        if let Ok(table) = data.parse::<toml::Table>()
            && let Some(toml::Value::Table(lib_table)) = table.get("library")
        {
            for (name, value) in lib_table {
                if let Ok(entry) = value.clone().try_into::<LibraryEntry>() {
                    libraries.insert(name.clone(), entry);
                }
            }
        }
        Self { libraries }
    }
}

static TS_LIBRARIES: &str = include_str!(concat!(env!("OUT_DIR"), "/ts_libraries.toml"));
static RUST_LIBRARIES: &str = include_str!(concat!(env!("OUT_DIR"), "/rust_libraries.toml"));

pub struct CompatibilityMatrix {
    source_language: String,
    target_language: String,
    built_in: HashMap<String, CompatibilityEntry>,
    overrides: HashMap<String, CompatibilityEntry>,
}

impl CompatibilityMatrix {
    /// Build a matrix from language-specific library registries.
    ///
    /// For each library in the source language registry, the best matching
    /// library in the target language registry is found via tag similarity
    /// (Jaccard index) and type matching. The result is pre-computed in
    /// `built_in` for fast lookup.
    pub fn new(source_language: String, target_language: String) -> Self {
        let source_registry = LanguageRegistry::load(&source_language);
        let target_registry = LanguageRegistry::load(&target_language);

        let mut built_in = HashMap::new();

        for (src_name, src_entry) in &source_registry.libraries {
            let best = find_best_match(src_entry, &target_registry.libraries);
            if let Some((tgt_name, score)) = best {
                let compat_level = score_to_compatibility(score);
                let effort = compat_level.default_effort();
                let pct = (score * 100.0) as u32;
                let guidance = Some(format!(
                    "Best match: `{}` (similarity {}%). {}",
                    tgt_name, pct, target_registry.libraries[&tgt_name].description
                ));
                let risk_tags = derive_risk_tags(
                    effort,
                    compat_level,
                    Some(&src_entry.tags),
                    guidance.as_deref(),
                );

                built_in.insert(
                    src_name.clone(),
                    CompatibilityEntry {
                        source_language: source_language.clone(),
                        target_language: target_language.clone(),
                        equivalent: Some(tgt_name.clone()),
                        compatibility: compat_level,
                        effort,
                        guidance,
                        note: None,
                        tags: Some(src_entry.tags.clone()),
                        risk_tags,
                    },
                );
            } else {
                built_in.insert(
                    src_name.clone(),
                    CompatibilityEntry {
                        source_language: source_language.clone(),
                        target_language: target_language.clone(),
                        equivalent: None,
                        compatibility: CompatibilityLevel::Unknown,
                        effort: MigrationEffort::Unknown,
                        guidance: Some("No matching library found in target language.".to_string()),
                        note: Some(format!(
                            "`{}` has no recognizable equivalent in {}.",
                            src_name, target_language
                        )),
                        tags: Some(src_entry.tags.clone()),
                        risk_tags: vec!["unmapped".to_string()],
                    },
                );
            }
        }

        Self {
            source_language,
            target_language,
            built_in,
            overrides: HashMap::new(),
        }
    }

    pub fn source_language(&self) -> &str {
        &self.source_language
    }

    pub fn target_language(&self) -> &str {
        &self.target_language
    }

    pub fn load_overrides(&mut self, path: &Path) -> anyhow::Result<()> {
        if !path.exists() {
            return Ok(());
        }
        let content = fs::read_to_string(path)?;
        let override_file: CompatibilityOverrideFile = toml::from_str(&content)?;

        for (toml_key, entry) in override_file.dependencies {
            let pkg_name = entry
                .packages
                .as_ref()
                .and_then(|p| p.get(&self.source_language))
                .cloned()
                .unwrap_or_else(|| toml_key.clone());

            let compat_level = entry
                .compatibility
                .as_deref()
                .map(parse_compatibility_str)
                .unwrap_or(CompatibilityLevel::Unknown);

            let effort = entry
                .effort
                .as_deref()
                .map(parse_effort_str)
                .unwrap_or_else(|| compat_level.default_effort());

            let target_pkg = entry
                .packages
                .as_ref()
                .and_then(|p| p.get(&self.target_language))
                .or(entry.equivalent.as_ref())
                .cloned();

            let guidance = match &entry.guidance {
                GuidanceOverride::Single(s) => Some(s.clone()),
                GuidanceOverride::Map(m) => {
                    let dir_key = format!("{}_to_{}", self.source_language, self.target_language);
                    m.get(&dir_key)
                        .or_else(|| {
                            if m.len() == 1 {
                                m.values().next()
                            } else {
                                None
                            }
                        })
                        .cloned()
                }
                GuidanceOverride::None => None,
            };

            let risk_tags = derive_risk_tags(
                effort,
                compat_level,
                entry.tags.as_deref(),
                guidance.as_deref(),
            );

            self.overrides.insert(
                pkg_name,
                CompatibilityEntry {
                    source_language: self.source_language.clone(),
                    target_language: self.target_language.clone(),
                    equivalent: target_pkg,
                    compatibility: compat_level,
                    effort,
                    guidance,
                    note: entry.note,
                    tags: entry.tags,
                    risk_tags,
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
                        effort: MigrationEffort::Unknown,
                        guidance: None,
                        note: Some("No compatibility mapping available.".to_string()),
                        tags: None,
                        risk_tags: vec!["unmapped".to_string()],
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

            let needs_review = match change_type {
                "added" => new_entry
                    .map(|e| {
                        e.compatibility == CompatibilityLevel::Unknown
                            || e.compatibility == CompatibilityLevel::None
                    })
                    .unwrap_or(true),
                "removed" => old_entry
                    .map(|e| {
                        e.compatibility == CompatibilityLevel::Full
                            || e.compatibility == CompatibilityLevel::Partial
                    })
                    .unwrap_or(false),
                _ => new_entry
                    .map(|e| {
                        e.compatibility == CompatibilityLevel::Partial
                            || e.compatibility == CompatibilityLevel::Unknown
                    })
                    .unwrap_or(true),
            };

            let entry = new_entry.or(old_entry);
            changes.push(DepChangeInfo {
                package: name.to_string(),
                old_version: old_dep.map(|d| d.version.clone()),
                new_version: new_dep.map(|d| d.version.clone()),
                change_type: change_type.to_string(),
                compatibility_before: old_entry.map(|e| e.compatibility),
                compatibility_now: new_entry.map(|e| e.compatibility),
                equivalent: new_entry.and_then(|e| e.equivalent.clone()),
                needs_review,
                effort: entry.map(|e| e.effort).unwrap_or(MigrationEffort::Unknown),
                guidance: entry.and_then(|e| e.guidance.clone()),
                risk_tags: entry.map(|e| e.risk_tags.clone()).unwrap_or_default(),
            });
        }

        changes
    }

    /// Analyze the impact of dependency changes on the source codebase.
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
                && (compat == CompatibilityLevel::Unknown || compat == CompatibilityLevel::None);

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

// ── Matching algorithm ────────────────────────────────────────────────

/// Find the best matching target library for a source library.
/// Returns `(target_name, score)` where score ∈ [0.0, 1.0].
fn find_best_match(
    src: &LibraryEntry,
    target_registry: &HashMap<String, LibraryEntry>,
) -> Option<(String, f64)> {
    target_registry
        .iter()
        .map(|(name, tgt)| (name.clone(), compute_similarity(src, tgt)))
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .filter(|(_, score)| *score > 0.0)
}

/// Compute similarity between two library entries.
///
/// Score = Jaccard tag similarity × 0.8 + type match bonus × 0.2
fn compute_similarity(src: &LibraryEntry, tgt: &LibraryEntry) -> f64 {
    let tag_sim = jaccard_similarity(&src.tags, &tgt.tags);
    let type_bonus = if src.lib_type == tgt.lib_type {
        1.0
    } else {
        0.0
    };
    tag_sim * 0.8 + type_bonus * 0.2
}

fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    let a_set: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let b_set: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = a_set.intersection(&b_set).count();
    let union = a_set.union(&b_set).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn score_to_compatibility(score: f64) -> CompatibilityLevel {
    if score >= 0.5 {
        CompatibilityLevel::Full
    } else if score >= 0.25 {
        CompatibilityLevel::Partial
    } else if score > 0.0 {
        CompatibilityLevel::None
    } else {
        CompatibilityLevel::Unknown
    }
}

// ── Override loading ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct CompatibilityOverrideFile {
    dependencies: HashMap<String, OverrideEntry>,
}

#[derive(Debug, Clone, Deserialize)]
struct OverrideEntry {
    packages: Option<HashMap<String, String>>,
    equivalent: Option<String>,
    compatibility: Option<String>,
    effort: Option<String>,
    #[serde(default)]
    guidance: GuidanceOverride,
    note: Option<String>,
    tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(untagged)]
enum GuidanceOverride {
    #[default]
    None,
    Single(String),
    Map(HashMap<String, String>),
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
        // Known TS libraries should have matches
        assert!(
            matrix.built_in.contains_key("express"),
            "express should be in TS registry"
        );
        assert!(
            matrix.built_in.contains_key("axios"),
            "axios should be in TS registry"
        );
        // Rust libs should NOT be in the source registry (TS)
        assert!(
            !matrix.built_in.contains_key("axum"),
            "axum is a Rust lib, not in TS registry"
        );
    }

    #[test]
    fn test_compatibility_matrix_loads_rust_to_ts_entries() {
        let matrix = CompatibilityMatrix::new("rust".to_string(), "typescript".to_string());
        // Known Rust libraries should have matches
        assert!(
            matrix.built_in.contains_key("axum"),
            "axum should be in Rust registry"
        );
        assert!(
            matrix.built_in.contains_key("reqwest"),
            "reqwest should be in Rust registry"
        );
        // TS libs should NOT be in the source registry (Rust)
        assert!(
            !matrix.built_in.contains_key("axios"),
            "axios is a TS lib, not in Rust registry"
        );
    }

    #[test]
    fn test_compatibility_level_default() {
        let entry = CompatibilityEntry {
            source_language: "typescript".to_string(),
            target_language: "rust".to_string(),
            equivalent: None,
            compatibility: CompatibilityLevel::Unknown,
            effort: MigrationEffort::Unknown,
            guidance: None,
            note: None,
            tags: None,
            risk_tags: vec![],
        };
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: CompatibilityEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.compatibility, CompatibilityLevel::Unknown);
    }

    #[test]
    fn test_jaccard_similarity() {
        let a = vec![
            "http".to_string(),
            "server".to_string(),
            "async".to_string(),
        ];
        let b = vec![
            "http".to_string(),
            "server".to_string(),
            "routing".to_string(),
        ];
        let sim = jaccard_similarity(&a, &b);
        // intersection = {http, server} = 2, union = {http, server, async, routing} = 4
        assert!((sim - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_compute_similarity() {
        let a = LibraryEntry {
            lib_type: "framework".to_string(),
            description: "test".to_string(),
            tags: vec![
                "http".to_string(),
                "server".to_string(),
                "async".to_string(),
            ],
        };
        let b = LibraryEntry {
            lib_type: "framework".to_string(),
            description: "test".to_string(),
            tags: vec![
                "http".to_string(),
                "server".to_string(),
                "routing".to_string(),
            ],
        };
        // tag sim = 2/4 = 0.5, type match = 1.0
        // score = 0.5 * 0.8 + 1.0 * 0.2 = 0.4 + 0.2 = 0.6
        let score = compute_similarity(&a, &b);
        assert!((score - 0.6).abs() < 1e-6);
    }

    #[test]
    fn test_score_to_compatibility() {
        assert_eq!(score_to_compatibility(0.6), CompatibilityLevel::Full);
        assert_eq!(score_to_compatibility(0.5), CompatibilityLevel::Full);
        assert_eq!(score_to_compatibility(0.4), CompatibilityLevel::Partial);
        assert_eq!(score_to_compatibility(0.25), CompatibilityLevel::Partial);
        assert_eq!(score_to_compatibility(0.1), CompatibilityLevel::None);
        assert_eq!(score_to_compatibility(0.0), CompatibilityLevel::Unknown);
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
        // uuid exists in TS registry and should find a match
        assert!(!changes[0].needs_review);

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

        // Removed dep with good compat → needs review
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
        assert!(changes[0].needs_review);

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
        assert!(!changes[0].needs_review);
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
            effort: MigrationEffort::Moderate,
            guidance: Some("Replace axios calls with reqwest".to_string()),
            risk_tags: vec!["http-client".to_string()],
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
        assert!(!impacts[0].is_high_impact);
    }

    #[test]
    fn test_express_matches_axum() {
        let matrix = CompatibilityMatrix::new("typescript".to_string(), "rust".to_string());
        let entry = matrix.lookup("express");
        assert!(entry.is_some(), "express should have a match");
        let entry = entry.unwrap();
        // express is a web framework, should match a Rust web framework (axum most likely)
        assert!(
            entry.equivalent.is_some(),
            "express should have an equivalent"
        );
        assert!(
            entry.compatibility >= CompatibilityLevel::Partial,
            "express should have at least partial compatibility"
        );
    }

    #[test]
    fn test_prisma_matches_sqlx_or_diesel() {
        let matrix = CompatibilityMatrix::new("typescript".to_string(), "rust".to_string());
        let entry = matrix.lookup("prisma");
        assert!(entry.is_some(), "prisma should have a match");
        let eq = entry.unwrap().equivalent.as_deref().unwrap_or("");
        // prisma is an ORM, should match sqlx or diesel or sea-orm
        assert!(
            eq.contains("sqlx") || eq.contains("diesel") || eq.contains("sea-orm"),
            "prisma should match an ORM, got {}",
            eq
        );
    }

    #[test]
    fn test_reverse_matching_axum_matches_express_or_koa() {
        let matrix = CompatibilityMatrix::new("rust".to_string(), "typescript".to_string());
        let entry = matrix.lookup("axum");
        assert!(entry.is_some(), "axum should have a match");
        let eq = entry.unwrap().equivalent.as_deref().unwrap_or("");
        assert!(
            eq.contains("express") || eq.contains("koa") || eq.contains("fastify"),
            "axum should match a TS web framework, got {}",
            eq
        );
    }
}
