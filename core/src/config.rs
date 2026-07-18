use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub project: ProjectConfig,
    #[serde(default)]
    pub output: OutputConfig,
    #[serde(default)]
    pub scoring: ScoringConfig,
    #[serde(default)]
    pub compatibility: CompatibilityConfig,
    #[serde(default)]
    pub skip: SkipConfig,
    #[serde(default)]
    pub mapping: MappingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Source project path
    #[serde(default)]
    pub source: Option<String>,
    /// Source git repository URL
    #[serde(default)]
    pub source_repo: Option<String>,
    /// Source git branch
    #[serde(default)]
    pub source_branch: Option<String>,
    /// Source git version (tag or commit hash) currently being analyzed
    #[serde(default)]
    pub source_version: Option<String>,
    /// Target/new project path
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, alias = "source_language")]
    pub source_lang: Option<String>,
    #[serde(default = "default_target_lang", alias = "target_language")]
    pub target_lang: String,
    /// If true, fail on the first error instead of collecting all errors
    #[serde(default)]
    pub strict: bool,
    #[serde(default)]
    pub ignore: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkipConfig {
    #[serde(default)]
    pub framework: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    #[serde(default = "default_output_dir")]
    pub directory: String,
    #[serde(default = "default_true")]
    pub split_by_directory: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoringConfig {
    #[serde(default)]
    pub weights: ScoreWeights,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreWeights {
    pub in_degree: u32,
    pub complexity: u32,
    pub compatibility: u32,
    pub cycles: u32,
    pub tests: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityConfig {
    pub overrides_file: Option<String>,
}

/// File path mapping from source to target.
/// Used when source files don't map 1:1 to target paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingConfig {
    #[serde(default)]
    pub override_list: Vec<MappingEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MappingEntry {
    pub from: String,
    pub to: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            project: ProjectConfig::default(),
            output: OutputConfig::default(),
            scoring: ScoringConfig::default(),
            compatibility: CompatibilityConfig::default(),
            skip: SkipConfig::default(),
            mapping: MappingConfig::default(),
        }
    }
}

impl Default for SkipConfig {
    fn default() -> Self {
        Self { framework: false }
    }
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            source: None,
            source_repo: None,
            source_branch: None,
            source_version: None,
            target: None,
            source_lang: None,
            target_lang: "rust".to_string(),
            strict: false,
            ignore: vec![],
            exclude: vec![],
        }
    }
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            directory: default_output_dir(),
            split_by_directory: default_true(),
        }
    }
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            weights: ScoreWeights::default(),
        }
    }
}

impl Default for ScoreWeights {
    fn default() -> Self {
        Self {
            in_degree: 30,
            complexity: 25,
            compatibility: 20,
            cycles: 15,
            tests: 10,
        }
    }
}

impl Default for CompatibilityConfig {
    fn default() -> Self {
        Self {
            overrides_file: Some(".migration-assessor-compat.toml".to_string()),
        }
    }
}

impl Default for MappingConfig {
    fn default() -> Self {
        Self {
            override_list: vec![],
        }
    }
}

fn default_target_lang() -> String {
    "rust".to_string()
}

fn default_output_dir() -> String {
    ".migration".to_string()
}

fn default_true() -> bool {
    true
}

const VALID_LANGUAGES: &[&str] = &["typescript", "rust", "python", "go", "java", "kotlin"];

impl Config {
    pub fn load(path: &PathBuf) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        config.validate()?;
        Ok(config)
    }

    pub fn default_with_target(target: String) -> Self {
        let mut config = Config::default();
        config.project.target_lang = target;
        config
    }

    /// Validate configuration fields, returning errors for invalid values.
    pub fn validate(&self) -> anyhow::Result<()> {
        if let Some(ref lang) = self.project.source_lang {
            if !VALID_LANGUAGES.contains(&lang.as_str()) {
                anyhow::bail!(
                    "Invalid source_language '{}'. Valid values: {}",
                    lang,
                    VALID_LANGUAGES.join(", ")
                );
            }
        }
        if !VALID_LANGUAGES.contains(&self.project.target_lang.as_str()) {
            anyhow::bail!(
                "Invalid target_language '{}'. Valid values: {}",
                self.project.target_lang,
                VALID_LANGUAGES.join(", ")
            );
        }
        Ok(())
    }
}
