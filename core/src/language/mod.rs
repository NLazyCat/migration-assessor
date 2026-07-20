pub mod registry;
pub mod rust;
pub mod typescript;

pub use registry::LanguageRegistry;

use crate::deps::ResolvedDependency;
use crate::references::{ForwardIndex, ReverseIndex};
use crate::symbols::{ApiContract, Symbol, SymbolIndex};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub severity: DiagnosticSeverity,
}

pub enum AstNode<'a> {
    TypeScript(oxc_ast::ast::Program<'a>),
    Rust(syn::File),
    Other(serde_json::Value),
}

pub struct ParsedFile<'a> {
    pub source: String,
    pub file_path: String,
    pub language: String,
    pub ast: AstNode<'a>,
    pub diagnostics: Vec<Diagnostic>,
}

pub trait DiffAnalyzer: Send + Sync + 'static {
    fn diff_files(
        &self,
        old_parsed: &ParsedFile,
        new_parsed: &ParsedFile,
    ) -> anyhow::Result<super::diff::FileDiffResult>;

    fn diff_symbols(
        &self,
        old_sym: &Symbol,
        new_sym: &Symbol,
        old_ast: &AstNode,
        new_ast: &AstNode,
    ) -> anyhow::Result<Vec<super::diff::SymbolChange>>;

    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String>;

    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)>;
}

pub trait Language: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn file_extensions(&self) -> &[&str];

    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile>;

    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(SymbolIndex, ApiContract)>;

    fn extract_references(&self, parsed: &ParsedFile) -> anyhow::Result<(ForwardIndex, ReverseIndex)>;

    fn resolve_dependencies(&self, project_root: &Path) -> anyhow::Result<Vec<ResolvedDependency>>;

    fn diff_analyzer(&self) -> &dyn DiffAnalyzer;

    fn detect_project_type(&self, project_root: &Path) -> bool;
}
