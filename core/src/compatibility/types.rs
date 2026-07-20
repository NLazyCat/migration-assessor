use serde::{Deserialize, Serialize};
use std::collections::HashSet;

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
        Ok(parse_compatibility_str(&s))
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

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct LibraryEntry {
    #[serde(rename = "type")]
    pub(crate) lib_type: String,
    pub(crate) description: String,
    pub(crate) tags: Vec<String>,
}

pub(crate) struct LanguageRegistry {
    pub(crate) libraries: std::collections::HashMap<String, LibraryEntry>,
}

impl LanguageRegistry {
    pub(crate) fn load(language: &str) -> Self {
        let data = match language {
            "typescript" => include_str!(concat!(env!("OUT_DIR"), "/ts_libraries.toml")),
            "rust" => include_str!(concat!(env!("OUT_DIR"), "/rust_libraries.toml")),
            _ => {
                return Self {
                    libraries: std::collections::HashMap::new(),
                };
            }
        };

        let mut libraries = std::collections::HashMap::new();
        if let Ok(table) = data.parse::<toml::Table>() && let Some(toml::Value::Table(lib_table)) = table.get("library") {
            for (name, value) in lib_table {
                if let Ok(entry) = value.clone().try_into::<LibraryEntry>() {
                    libraries.insert(name.clone(), entry);
                }
            }
        }
        Self { libraries }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompatibilityOverrideFile {
    pub dependencies: std::collections::HashMap<String, OverrideEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OverrideEntry {
    pub packages: Option<std::collections::HashMap<String, String>>,
    pub equivalent: Option<String>,
    pub compatibility: Option<String>,
    pub effort: Option<String>,
    #[serde(default)]
    pub guidance: GuidanceOverride,
    pub note: Option<String>,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(untagged)]
pub enum GuidanceOverride {
    #[default]
    None,
    Single(String),
    Map(std::collections::HashMap<String, String>),
}

pub fn parse_compatibility_str(s: &str) -> CompatibilityLevel {
    match s.trim().to_lowercase().as_str() {
        "full" => CompatibilityLevel::Full,
        "partial" => CompatibilityLevel::Partial,
        "none" => CompatibilityLevel::None,
        _ => CompatibilityLevel::Unknown,
    }
}

pub fn parse_effort_str(s: &str) -> MigrationEffort {
    match s.trim().to_lowercase().as_str() {
        "trivial" => MigrationEffort::Trivial,
        "moderate" => MigrationEffort::Moderate,
        "heavy" => MigrationEffort::Heavy,
        "rewrite" => MigrationEffort::Rewrite,
        _ => MigrationEffort::Unknown,
    }
}

pub fn derive_risk_tags(
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

