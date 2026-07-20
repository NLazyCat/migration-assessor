use super::{AstNode, DiffAnalyzer, Language, ParsedFile};
use crate::deps::rust as deps_rust;
use crate::symbols::rust as symbols_rust;
use std::path::Path;
use syn::visit::Visit;

pub struct RustLanguage;

impl Language for RustLanguage {
    fn name(&self) -> &str {
        "rust"
    }

    fn file_extensions(&self) -> &[&str] {
        &["rs"]
    }

    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile<'_>> {
        let file: syn::File = syn::parse_file(source)?;

        Ok(ParsedFile {
            source: source.to_string(),
            file_path: file_path.to_string(),
            language: "rust".to_string(),
            ast: AstNode::Rust(file),
            diagnostics: Vec::new(),
        })
    }

    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(crate::symbols::SymbolIndex, crate::symbols::ApiContract)> {
        let relative = Path::new(&parsed.file_path);
        let module = relative.to_string_lossy().replace('\\', "/");
        let (index, contract) = symbols_rust::extract(&module, &parsed.source)?;
        Ok((index, contract))
    }

    fn extract_references(
        &self,
        parsed: &ParsedFile,
    ) -> anyhow::Result<(crate::references::ForwardIndex, crate::references::ReverseIndex)> {
        let mut forward: crate::references::ForwardIndex = Default::default();
        let mut reverse: crate::references::ReverseIndex = Default::default();

        if let AstNode::Rust(file) = &parsed.ast {
            let mut visitor = UseVisitor { imports: Vec::new() };
            visitor.visit_file(file);

            for import in &visitor.imports {
                let ref_id = format!("{}:use:{}", parsed.file_path, import);
                forward.insert(ref_id.clone(), vec![]);
                reverse.insert(import.clone(), vec![]);
            }
        }

        Ok((forward, reverse))
    }

    fn resolve_dependencies(&self, project_root: &Path) -> anyhow::Result<Vec<crate::deps::ResolvedDependency>> {
        deps_rust::resolve(project_root)
    }

    fn diff_analyzer(&self) -> &dyn DiffAnalyzer {
        &RustDiffAnalyzer
    }

    fn detect_project_type(&self, project_root: &Path) -> bool {
        project_root.join("Cargo.toml").exists()
    }
}

struct UseVisitor {
    imports: Vec<String>,
}

impl<'ast> Visit<'ast> for UseVisitor {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let path_str = use_tree_to_string(&node.tree);
        if !path_str.is_empty() {
            self.imports.push(path_str);
        }
    }
}

fn use_tree_to_string(tree: &syn::UseTree) -> String {
    match tree {
        syn::UseTree::Path(path) => {
            let prefix = path.ident.to_string();
            let rest = use_tree_to_string(&path.tree);
            if rest.is_empty() { prefix } else { format!("{}::{}", prefix, rest) }
        }
        syn::UseTree::Name(name) => name.ident.to_string(),
        syn::UseTree::Rename(rename) => rename.ident.to_string(),
        syn::UseTree::Glob(_) => "*".to_string(),
        syn::UseTree::Group(group) => {
            let items: Vec<String> = group.items.iter().map(use_tree_to_string).collect();
            items.join(", ")
        }
    }
}

pub struct RustDiffAnalyzer;

impl DiffAnalyzer for RustDiffAnalyzer {
    fn diff_files(
        &self,
        old_parsed: &ParsedFile,
        new_parsed: &ParsedFile,
    ) -> anyhow::Result<crate::diff::FileDiffResult> {
        let (old_index, _) = RustLanguage.extract_symbols(old_parsed)?;
        let (new_index, _) = RustLanguage.extract_symbols(new_parsed)?;

        let mapping = crate::diff::mapping::build_symbol_mapping(&old_index, &new_index);

        let mut file_result = crate::diff::FileDiffResult {
            file: old_parsed.file_path.clone(),
            status: "modified".to_string(),
            symbol_changes: Vec::new(),
            import_changes: Vec::new(),
            doc_changes: Vec::new(),
        };

        for (old_id, new_id) in &mapping.renamed {
            let old_sym = old_index.symbols.iter().find(|s| &s.id == old_id).unwrap();
            let new_sym = new_index.symbols.iter().find(|s| &s.id == new_id).unwrap();

            file_result.symbol_changes.push(crate::diff::SymbolChange {
                symbol: new_sym.name.clone(),
                kind: new_sym.kind.clone(),
                change_type: "renamed".to_string(),
                severity: "compatible".to_string(),
                old_name: Some(old_sym.name.clone()),
                rename_confidence: Some(mapping.confidence.get(old_id).copied().unwrap_or(0.75)),
                details: Vec::new(),
                old_line_range: Some(old_sym.line_range),
                new_line_range: Some(new_sym.line_range),
            });
        }

        for sym in &mapping.added {
            file_result.symbol_changes.push(crate::diff::SymbolChange {
                symbol: sym.name.clone(),
                kind: sym.kind.clone(),
                change_type: "added".to_string(),
                severity: "compatible".to_string(),
                old_name: None,
                rename_confidence: None,
                details: Vec::new(),
                old_line_range: None,
                new_line_range: Some(sym.line_range),
            });
        }

        for sym in &mapping.removed {
            file_result.symbol_changes.push(crate::diff::SymbolChange {
                symbol: sym.name.clone(),
                kind: sym.kind.clone(),
                change_type: "removed".to_string(),
                severity: "breaking".to_string(),
                old_name: None,
                rename_confidence: None,
                details: Vec::new(),
                old_line_range: Some(sym.line_range),
                new_line_range: None,
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

        let old_set: std::collections::HashSet<_> = old_imports.iter().collect();
        let new_set: std::collections::HashSet<_> = new_imports.iter().collect();

        for pkg in &new_set - &old_set {
            file_result.import_changes.push(crate::diff::ImportChange {
                change_type: "added".to_string(),
                package: pkg.clone(),
                old_path: None,
                new_path: None,
                is_external: true,
                compatibility: None,
            });
        }

        for pkg in &old_set - &new_set {
            file_result.import_changes.push(crate::diff::ImportChange {
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
        old_sym: &crate::symbols::Symbol,
        new_sym: &crate::symbols::Symbol,
        _old_ast: &AstNode,
        _new_ast: &AstNode,
    ) -> anyhow::Result<Vec<crate::diff::SymbolChange>> {
        let mut changes = Vec::new();

        if let Some(sig_changes) = crate::diff::signature::diff(old_sym, new_sym) {
            changes.extend(sig_changes);
        }

        if let Some(val_change) = crate::diff::logic::diff_value(old_sym, new_sym) {
            changes.push(val_change);
        }

        if let Some(doc_change) = crate::diff::doc::diff(old_sym, new_sym) {
            let mut sc = crate::diff::SymbolChange {
                symbol: new_sym.name.clone(),
                kind: new_sym.kind.clone(),
                change_type: "modified".to_string(),
                severity: "compatible".to_string(),
                old_name: None,
                rename_confidence: None,
                details: Vec::new(),
                old_line_range: Some(old_sym.line_range),
                new_line_range: Some(new_sym.line_range),
            };
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

    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String> {
        if let AstNode::Rust(file) = &parsed.ast {
            let mut visitor = UseVisitor { imports: Vec::new() };
            visitor.visit_file(file);
            visitor.imports.sort();
            visitor.imports.dedup();
            visitor.imports
        } else {
            Vec::new()
        }
    }

    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)> {
        if let AstNode::Rust(file) = &parsed.ast {
            let mut visitor = CallVisitor { calls: Vec::new(), current_fn: String::new() };
            visitor.visit_file(file);
            visitor.calls.sort();
            visitor.calls.dedup();
            visitor.calls
        } else {
            Vec::new()
        }
    }
}

struct CallVisitor {
    calls: Vec<(String, String)>,
    current_fn: String,
}

impl<'ast> Visit<'ast> for CallVisitor {
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        let old_fn = self.current_fn.clone();
        self.current_fn = node.sig.ident.to_string();
        syn::visit::visit_item_fn(self, node);
        self.current_fn = old_fn;
    }

    fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
        let callee = self.extract_callee_name(&node.func);
        if !callee.is_empty() {
            self.calls.push((self.current_fn.clone(), callee));
        }
        syn::visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        let method_name = node.method.to_string();
        self.calls.push((self.current_fn.clone(), method_name));
        syn::visit::visit_expr_method_call(self, node);
    }
}

impl CallVisitor {
    fn extract_callee_name(&self, expr: &syn::Expr) -> String {
        match expr {
            syn::Expr::Path(path) => {
                path.path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default()
            }
            syn::Expr::Call(call) => self.extract_callee_name(&call.func),
            _ => String::new(),
        }
    }
}
