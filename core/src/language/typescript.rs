use super::{AstNode, Diagnostic, DiffAnalyzer, Language, ParsedFile};
use crate::deps::typescript as deps_ts;
use crate::parser::typescript as parser_ts;
use crate::symbols::typescript as symbols_ts;
use oxc_ast::ast::{Expression, Statement};
use std::path::Path;

pub struct TypeScriptLanguage;

impl Language for TypeScriptLanguage {
    fn name(&self) -> &str {
        "typescript"
    }

    fn file_extensions(&self) -> &[&str] {
        &["ts", "tsx", "js", "jsx"]
    }

    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile<'_>> {
        let diagnostics: Vec<Diagnostic> = Vec::new();

        Ok(ParsedFile {
            source: source.to_string(),
            file_path: file_path.to_string(),
            language: "typescript".to_string(),
            ast: AstNode::Other(serde_json::json!({})),
            diagnostics,
        })
    }

    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(crate::symbols::SymbolIndex, crate::symbols::ApiContract)> {
        let relative = Path::new(&parsed.file_path);
        let module = relative.to_string_lossy().replace('\\', "/");
        let (index, contract) = symbols_ts::extract(&module, &parsed.source, Some(relative))?;
        Ok((index, contract))
    }

    fn extract_references(
        &self,
        parsed: &ParsedFile,
    ) -> anyhow::Result<(crate::references::ForwardIndex, crate::references::ReverseIndex)> {
        let bindings = parser_ts::parse_references(&parsed.source, Some(Path::new(&parsed.file_path)))?;
        let _file = Path::new(&parsed.file_path);
        
        let mut forward: crate::references::ForwardIndex = Default::default();
        let mut reverse: crate::references::ReverseIndex = Default::default();
        
        for import in &bindings.external_imports {
            let ref_id = format!("{}:import:{}", parsed.file_path, import);
            forward.insert(ref_id.clone(), vec![]);
            reverse.insert(import.clone(), vec![]);
        }
        
        Ok((forward, reverse))
    }

    fn resolve_dependencies(&self, project_root: &Path) -> anyhow::Result<Vec<crate::deps::ResolvedDependency>> {
        deps_ts::resolve(project_root)
    }

    fn diff_analyzer(&self) -> &dyn DiffAnalyzer {
        &TypeScriptDiffAnalyzer
    }

    fn detect_project_type(&self, project_root: &Path) -> bool {
        project_root.join("tsconfig.json").exists() || project_root.join("package.json").exists()
    }
}

pub struct TypeScriptDiffAnalyzer;

impl DiffAnalyzer for TypeScriptDiffAnalyzer {
    fn diff_files(
        &self,
        old_parsed: &ParsedFile,
        new_parsed: &ParsedFile,
    ) -> anyhow::Result<crate::diff::FileDiffResult> {
        let (old_index, _) = TypeScriptLanguage.extract_symbols(old_parsed)?;
        let (new_index, _) = TypeScriptLanguage.extract_symbols(new_parsed)?;
        
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
        if let AstNode::TypeScript(program) = &parsed.ast {
            let mut imports = Vec::new();
            for stmt in &program.body {
                match stmt {
                    Statement::ImportDeclaration(import) => {
                        let src = import.source.value.to_string();
                        if !src.starts_with('.') && !src.starts_with('/') {
                            imports.push(src);
                        }
                    }
                    Statement::ExportNamedDeclaration(export) => {
                        if let Some(source) = &export.source {
                            let src = source.value.to_string();
                            if !src.starts_with('.') && !src.starts_with('/') {
                                imports.push(src);
                            }
                        }
                    }
                    Statement::ExportAllDeclaration(export) => {
                        let src = export.source.value.to_string();
                        if !src.starts_with('.') && !src.starts_with('/') {
                            imports.push(src);
                        }
                    }
                    _ => {}
                }
            }
            imports.sort();
            imports.dedup();
            imports
        } else {
            Vec::new()
        }
    }

    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)> {
        if let AstNode::TypeScript(program) = &parsed.ast {
            let mut calls = Vec::new();
            for stmt in &program.body {
                self.visit_statement(stmt, &mut calls);
            }
            calls.sort();
            calls.dedup();
            calls
        } else {
            Vec::new()
        }
    }
}

impl TypeScriptDiffAnalyzer {
    fn visit_statement(&self, stmt: &Statement, calls: &mut Vec<(String, String)>) {
        match stmt {
            Statement::FunctionDeclaration(func) => {
                if let Some(id) = &func.id
                    && let Some(body) = &func.body {
                        self.visit_function_body(body, id.name.to_string(), calls);
                }
            }
            Statement::VariableDeclaration(var_decl) => {
                for decl in &var_decl.declarations {
                    if let Some(init) = &decl.init {
                        self.visit_expression(init, "".to_string(), calls);
                    }
                }
            }
            Statement::BlockStatement(block) => {
                for stmt in &block.body {
                    self.visit_statement(stmt, calls);
                }
            }
            Statement::IfStatement(if_stmt) => {
                self.visit_expression(&if_stmt.test, "".to_string(), calls);
                self.visit_statement(&if_stmt.consequent, calls);
                if let Some(alt) = &if_stmt.alternate {
                    self.visit_statement(alt, calls);
                }
            }
            Statement::ForStatement(for_stmt) => {
                self.visit_statement(&for_stmt.body, calls);
            }
            Statement::WhileStatement(while_stmt) => {
                self.visit_statement(&while_stmt.body, calls);
            }
            Statement::ReturnStatement(ret_stmt) => {
                if let Some(arg) = &ret_stmt.argument {
                    self.visit_expression(arg, "".to_string(), calls);
                }
            }
            _ => {}
        }
    }

    fn visit_expression(&self, expr: &Expression, context: String, calls: &mut Vec<(String, String)>) {
        match expr {
            Expression::CallExpression(call) => {
                let callee = match &call.callee {
                    oxc_ast::ast::Expression::Identifier(id) => id.name.to_string(),
                    oxc_ast::ast::Expression::StaticMemberExpression(member) => {
                        match &member.object {
                            oxc_ast::ast::Expression::Identifier(obj_id) => {
                                format!("{}.{}", obj_id.name, member.property.name)
                            }
                            _ => "unknown".to_string(),
                        }
                    }
                    _ => "unknown".to_string(),
                };
                calls.push((context, callee));
            }
            Expression::ArrowFunctionExpression(_arrow) => {}
            Expression::FunctionExpression(_func) => {}
            Expression::SequenceExpression(_seq) => {}
            _ => {}
        }
    }

    fn visit_function_body(&self, _body: &oxc_ast::ast::FunctionBody, _context: String, _calls: &mut Vec<(String, String)>) {
    }
}
