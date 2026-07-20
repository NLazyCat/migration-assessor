use migration_core::recommendation::DependencyRecommendation;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DiffReportOutput {
    pub(crate) generated_at: String,
    pub(crate) source_repo: Option<String>,
    pub(crate) from_version: Option<String>,
    pub(crate) to_version: String,
    pub(crate) files: Vec<String>,
    pub(crate) file_changes: Vec<FileChangeGroup>,
    pub(crate) propagation: PropagationResult,
    pub(crate) summary: SummaryInfo,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SummaryInfo {
    pub(crate) total_files_changed: usize,
    pub(crate) symbols_added: usize,
    pub(crate) symbols_removed: usize,
    pub(crate) symbols_renamed: usize,
    pub(crate) symbols_modified: usize,
    pub(crate) breaking_changes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FileChangeGroup {
    pub(crate) file: String,
    pub(crate) source_attached: bool,
    pub(crate) changes: Vec<SymbolChangeDetail>,
    pub(crate) import_changes: Vec<ImportChangeDetail>,
    pub(crate) doc_changes: Vec<DocChangeDetail>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) recommendations: Vec<DependencyRecommendation>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SymbolChangeDetail {
    pub(crate) symbol: String,
    pub(crate) kind: String,
    pub(crate) change_type: String,
    pub(crate) severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) old_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) rename_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub(crate) details: Vec<ChangeDetailInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) old_line_range: Option<[usize; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) new_line_range: Option<[usize; 2]>,
    /// Full source snippet of the old version of this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) old_source: Option<String>,
    /// Full source snippet of the new version of this symbol
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) new_source: Option<String>,
    /// Target file to modify (from registry or default mapping)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_file: Option<String>,
    /// Target symbol to modify (from registry or default mapping)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_symbol: Option<String>,
    /// Target child/field/method name (context-aware match)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_child: Option<String>,
    /// Line range in target file for the matched symbol/child
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) target_line_range: Option<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ChangeDetailInfo {
    pub(crate) aspect: String,
    pub(crate) change_type: String,
    pub(crate) description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) old_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) new_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) migration_note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ImportChangeDetail {
    pub(crate) change_type: String,
    pub(crate) package: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) old_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) new_path: Option<String>,
    pub(crate) is_external: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DocChangeDetail {
    pub(crate) change_type: String,
    pub(crate) symbol: String,
    pub(crate) is_deprecated: bool,
    pub(crate) has_todo: bool,
    pub(crate) has_safety_note: bool,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PropagationResult {
    pub(crate) triggered_by: Vec<String>,
    pub(crate) affected_files: Vec<String>,
    pub(crate) chain: Vec<PropagationLink>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct PropagationLink {
    pub(crate) from: String,
    pub(crate) to: String,
    pub(crate) via: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ReverseRef {
    pub(crate) symbol: String,
    #[allow(dead_code)]
    pub(crate) location: ReverseLocation,
    pub(crate) kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct ReverseLocation {
    #[allow(dead_code)]
    pub(crate) file: String,
    #[allow(dead_code)]
    pub(crate) line: usize,
    #[allow(dead_code)]
    pub(crate) column: usize,
}

pub(crate) type ReverseIndex = HashMap<String, Vec<ReverseRef>>;
