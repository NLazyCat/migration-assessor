pub mod dependency;
pub mod doc;
pub mod engine;
pub mod logic;
pub mod mapping;
pub mod signature;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffReport {
    pub generated_at: String,
    pub from_version: Option<String>,
    pub to_version: String,
    pub summary: DiffSummary,
    pub file_changes: Vec<FileDiffResult>,
    pub dependency_changes: Vec<DependencyChange>,
    pub propagation: PropagationResult,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffSummary {
    pub total_files_changed: usize,
    pub symbols_added: usize,
    pub symbols_removed: usize,
    pub symbols_renamed: usize,
    pub symbols_modified: usize,
    pub breaking_changes: usize,
    pub new_dependencies: usize,
    pub removed_dependencies: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileDiffResult {
    pub file: String,
    pub status: String,
    pub symbol_changes: Vec<SymbolChange>,
    pub import_changes: Vec<ImportChange>,
    pub doc_changes: Vec<DocChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolChange {
    pub symbol: String,
    pub kind: String,
    pub change_type: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rename_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub details: Vec<ChangeDetail>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_line_range: Option<[usize; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_line_range: Option<[usize; 2]>,
    /// Source code snippet of the old version of this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_source: Option<String>,
    /// Source code snippet of the new version of this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_source: Option<String>,
    /// Target file path (from registry or default mapping)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_file: Option<String>,
    /// Target symbol name (from registry or default mapping)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_symbol: Option<String>,
    /// Target child/field/method name (context-aware match)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_child: Option<String>,
    /// Line range in target file for the matched symbol/child
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_line_range: Option<[usize; 2]>,
}

impl SymbolChange {
    /// Construct a SymbolChange with all optional fields defaulted to None.
    pub fn new(
        symbol: String,
        kind: String,
        change_type: String,
        severity: String,
        old_line_range: Option<[usize; 2]>,
        new_line_range: Option<[usize; 2]>,
        details: Vec<ChangeDetail>,
    ) -> Self {
        Self {
            symbol,
            kind,
            change_type,
            severity,
            old_name: None,
            rename_confidence: None,
            details,
            old_line_range,
            new_line_range,
            old_source: None,
            new_source: None,
            target_file: None,
            target_symbol: None,
            target_child: None,
            target_line_range: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeDetail {
    pub aspect: String,
    pub change_type: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub migration_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportChange {
    pub change_type: String,
    pub package: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_path: Option<String>,
    pub is_external: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compatibility: Option<CompatibilityInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocChange {
    pub change_type: String,
    pub symbol: String,
    pub is_deprecated: bool,
    pub has_todo: bool,
    pub has_safety_note: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_doc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_doc: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyChange {
    pub package: String,
    pub change_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    pub compatibility: CompatibilityInfo,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub affected_modules: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatibilityInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equivalent: Option<String>,
    pub compatibility: String,
    pub effort: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guidance: Option<String>,
    pub is_high_risk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationResult {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub affected_symbols: Vec<AffectedSymbol>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffectedSymbol {
    pub symbol: String,
    pub kind: String,
    pub file: String,
    pub line: usize,
    pub impact: String,
    pub dependency_path: Vec<String>,
}
