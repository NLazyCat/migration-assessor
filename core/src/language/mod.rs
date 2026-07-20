pub mod javascript;
pub mod registry;
pub mod rust;
pub mod typescript;

pub use registry::LanguageRegistry;

use crate::deps::ResolvedDependency;
use crate::diff::{FileDiffResult, ImportChange, SymbolChange};
use crate::references::{ForwardIndex, ReverseIndex};
use crate::symbols::{ApiContract, Symbol, SymbolIndex};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
    ) -> anyhow::Result<FileDiffResult> {
        let lang_registry = LanguageRegistry::get();
        let lang_name = &old_parsed.language;
        let language = lang_registry
            .get_language(lang_name)
            .ok_or_else(|| anyhow::anyhow!("Language {} not found", lang_name))?;

        let (old_index, _) = language.extract_symbols(old_parsed)?;
        let (new_index, _) = language.extract_symbols(new_parsed)?;

        let mapping = crate::diff::mapping::build_symbol_mapping(&old_index, &new_index);

        let mut file_result = FileDiffResult {
            file: old_parsed.file_path.clone(),
            status: "modified".to_string(),
            symbol_changes: Vec::new(),
            import_changes: Vec::new(),
            doc_changes: Vec::new(),
        };

        for (old_id, new_id) in &mapping.renamed {
            let old_sym = old_index.symbols.iter().find(|s| &s.id == old_id).unwrap();
            let new_sym = new_index.symbols.iter().find(|s| &s.id == new_id).unwrap();

            file_result.symbol_changes.push(SymbolChange {
                symbol: new_sym.name.clone(),
                kind: new_sym.kind.clone(),
                change_type: "renamed".to_string(),
                severity: "compatible".to_string(),
                old_name: Some(old_sym.name.clone()),
                rename_confidence: Some(mapping.confidence.get(old_id).copied().unwrap_or(0.75)),
                details: Vec::new(),
                old_line_range: Some(old_sym.line_range),
                new_line_range: Some(new_sym.line_range),
                old_source: None,
                new_source: None,
                target_file: None,
                target_symbol: Some(new_sym.name.clone()),
                target_child: None,
                target_line_range: None,
            });
        }

        for sym in &mapping.added {
            file_result.symbol_changes.push(SymbolChange {
                symbol: sym.name.clone(),
                kind: sym.kind.clone(),
                change_type: "added".to_string(),
                severity: "compatible".to_string(),
                old_name: None,
                rename_confidence: None,
                details: Vec::new(),
                old_line_range: None,
                new_line_range: Some(sym.line_range),
                old_source: None,
                new_source: None,
                target_file: None,
                target_symbol: None,
                target_child: None,
                target_line_range: None,
            });
        }

        for sym in &mapping.removed {
            file_result.symbol_changes.push(SymbolChange {
                symbol: sym.name.clone(),
                kind: sym.kind.clone(),
                change_type: "removed".to_string(),
                severity: "breaking".to_string(),
                old_name: None,
                rename_confidence: None,
                details: Vec::new(),
                old_line_range: Some(sym.line_range),
                new_line_range: None,
                old_source: None,
                new_source: None,
                target_file: None,
                target_symbol: None,
                target_child: None,
                target_line_range: None,
            });
        }

        for (old_sym, new_sym) in &mapping.stable {
            if let Some(changes) = crate::diff::signature::diff(old_sym, new_sym) {
                file_result.symbol_changes.extend(changes);
            }

            if let Some(val_change) = crate::diff::logic::diff_value(old_sym, new_sym) {
                file_result.symbol_changes.push(val_change);
            }

            if let Some(doc_change) = crate::diff::doc::diff(old_sym, new_sym) {
                file_result.doc_changes.push(doc_change);
            }
        }

        let old_imports = self.extract_imports(old_parsed);
        let new_imports = self.extract_imports(new_parsed);

        let old_set: HashSet<_> = old_imports.iter().collect();
        let new_set: HashSet<_> = new_imports.iter().collect();

        for pkg in &new_set - &old_set {
            file_result.import_changes.push(ImportChange {
                change_type: "added".to_string(),
                package: pkg.clone(),
                old_path: None,
                new_path: None,
                is_external: true,
                compatibility: None,
            });
        }

        for pkg in &old_set - &new_set {
            file_result.import_changes.push(ImportChange {
                change_type: "removed".to_string(),
                package: pkg.clone(),
                old_path: None,
                new_path: None,
                is_external: true,
                compatibility: None,
            });
        }

        Ok(file_result)
    }

    fn diff_symbols(
        &self,
        old_sym: &Symbol,
        new_sym: &Symbol,
        _old_ast: &AstNode,
        _new_ast: &AstNode,
    ) -> anyhow::Result<Vec<SymbolChange>> {
        let mut changes = Vec::new();

        if let Some(sig_changes) = crate::diff::signature::diff(old_sym, new_sym) {
            changes.extend(sig_changes);
        }

        if let Some(val_change) = crate::diff::logic::diff_value(old_sym, new_sym) {
            changes.push(val_change);
        }

        if let Some(doc_change) = crate::diff::doc::diff(old_sym, new_sym) {
            let mut sc = SymbolChange::new(
                new_sym.name.clone(),
                new_sym.kind.clone(),
                "modified".to_string(),
                "compatible".to_string(),
                Some(old_sym.line_range),
                Some(new_sym.line_range),
                Vec::new(),
            );
            sc.details.push(crate::diff::ChangeDetail {
                aspect: "documentation".to_string(),
                change_type: doc_change.change_type.clone(),
                description: doc_change.change_type.clone(),
                old_value: doc_change.old_doc.clone(),
                new_value: doc_change.new_doc.clone(),
                migration_note: None,
            });
            changes.push(sc);
        }

        Ok(changes)
    }

    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String>;

    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)>;
}

pub trait Language: Send + Sync + 'static {
    fn name(&self) -> &str;

    fn file_extensions(&self) -> &[&str];

    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile<'_>>;

    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(SymbolIndex, ApiContract)>;

    fn extract_references(&self, parsed: &ParsedFile) -> anyhow::Result<(ForwardIndex, ReverseIndex)>;

    fn resolve_dependencies(&self, project_root: &Path) -> anyhow::Result<Vec<ResolvedDependency>>;

    fn diff_analyzer(&self) -> &dyn DiffAnalyzer;

    fn detect_project_type(&self, project_root: &Path) -> bool;
}
