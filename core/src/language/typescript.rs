use super::{AstNode, Diagnostic, DiffAnalyzer, Language, ParsedFile};
use crate::deps::typescript as deps_ts;
use crate::parser::typescript as parser_ts;
use crate::symbols::typescript as symbols_ts;
use oxc_allocator::Allocator;
use oxc_ast::ast::{Expression, Statement};
use oxc_parser::Parser;
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
    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String> {
        let source_type = crate::util::detect_source_type(Some(Path::new(&parsed.file_path)));
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, &parsed.source, source_type).parse();
        let mut imports = Vec::new();
        for stmt in &ret.program.body {
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
    }

    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)> {
        let source_type = crate::util::detect_source_type(Some(Path::new(&parsed.file_path)));
        let allocator = Allocator::default();
        let ret = Parser::new(&allocator, &parsed.source, source_type).parse();
        let mut calls = Vec::new();
        for stmt in &ret.program.body {
            self.visit_statement(stmt, &mut calls);
        }
        calls.sort();
        calls.dedup();
        calls
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
