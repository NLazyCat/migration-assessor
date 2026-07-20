use super::{AstNode, Diagnostic, DiffAnalyzer, Language, ParsedFile};
use crate::deps::javascript as deps_js;
use crate::parser::javascript as parser_js;
use crate::symbols::javascript as symbols_js;
use oxc_allocator::Allocator;
use oxc_ast::ast::{CallExpression, Expression, Statement};
use oxc_parser::Parser;
use std::path::Path;

pub struct JavaScriptLanguage;

impl Language for JavaScriptLanguage {
    fn name(&self) -> &str {
        "javascript"
    }

    fn file_extensions(&self) -> &[&str] {
        &["js", "jsx", "mjs", "cjs"]
    }

    fn parse(&self, source: &str, file_path: &str) -> anyhow::Result<ParsedFile<'_>> {
        let diagnostics: Vec<Diagnostic> = Vec::new();

        Ok(ParsedFile {
            source: source.to_string(),
            file_path: file_path.to_string(),
            language: "javascript".to_string(),
            ast: AstNode::Other(serde_json::json!({})),
            diagnostics,
        })
    }

    fn extract_symbols(&self, parsed: &ParsedFile) -> anyhow::Result<(crate::symbols::SymbolIndex, crate::symbols::ApiContract)> {
        let relative = Path::new(&parsed.file_path);
        let module = relative.to_string_lossy().replace('\\', "/");
        let (index, contract) = symbols_js::extract(&module, &parsed.source, Some(relative))?;
        Ok((index, contract))
    }

    fn extract_references(
        &self,
        parsed: &ParsedFile,
    ) -> anyhow::Result<(crate::references::ForwardIndex, crate::references::ReverseIndex)> {
        let bindings = parser_js::parse_references(&parsed.source, Some(Path::new(&parsed.file_path)))?;
        
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
        deps_js::resolve(project_root)
    }

    fn diff_analyzer(&self) -> &dyn DiffAnalyzer {
        &JavaScriptDiffAnalyzer
    }

    fn detect_project_type(&self, project_root: &Path) -> bool {
        project_root.join("package.json").exists()
            && !project_root.join("tsconfig.json").exists()
    }
}

pub struct JavaScriptDiffAnalyzer;

impl JavaScriptDiffAnalyzer {
    fn call_callee_name(&self, call: &CallExpression) -> String {
        match &call.callee {
            Expression::Identifier(id) => id.name.to_string(),
            Expression::StaticMemberExpression(member) => {
                let obj = self.expr_name(&member.object);
                format!("{}.{}", obj, member.property.name)
            }
            _ => "fn".to_string(),
        }
    }

    fn expr_name(&self, expr: &Expression) -> String {
        match expr {
            Expression::Identifier(id) => id.name.to_string(),
            Expression::StringLiteral(s) => s.value.to_string(),
            _ => "expr".to_string(),
        }
    }

    fn walk_statement(&self, stmt: &Statement, calls: &mut Vec<(String, String)>) {
        match stmt {
            Statement::ExpressionStatement(es) => self.walk_expression(&es.expression, calls),
            Statement::ReturnStatement(rs) => {
                if let Some(expr) = &rs.argument {
                    self.walk_expression(expr, calls);
                }
            }
            Statement::IfStatement(is) => {
                self.walk_expression(&is.test, calls);
                self.walk_statement(&is.consequent, calls);
                if let Some(alt) = &is.alternate {
                    self.walk_statement(alt, calls);
                }
            }
            Statement::ForStatement(fs) => self.walk_statement(&fs.body, calls),
            Statement::ForInStatement(fs) => self.walk_statement(&fs.body, calls),
            Statement::ForOfStatement(fs) => self.walk_statement(&fs.body, calls),
            Statement::WhileStatement(ws) => self.walk_statement(&ws.body, calls),
            Statement::DoWhileStatement(dws) => self.walk_statement(&dws.body, calls),
            Statement::SwitchStatement(ss) => {
                for case in &ss.cases {
                    for stmt in &case.consequent {
                        self.walk_statement(stmt, calls);
                    }
                }
            }
            Statement::TryStatement(ts) => {
                self.walk_block(&ts.block.body, calls);
                if let Some(handler) = &ts.handler {
                    self.walk_block(&handler.body.body, calls);
                }
                if let Some(finalizer) = &ts.finalizer {
                    self.walk_block(&finalizer.body, calls);
                }
            }
            Statement::BlockStatement(bs) => self.walk_block(&bs.body, calls),
            Statement::FunctionDeclaration(f) => {
                if let Some(body) = &f.body {
                    self.walk_block(&body.statements, calls);
                }
            }
            _ => {}
        }
    }

    fn walk_block(&self, stmts: &[Statement], calls: &mut Vec<(String, String)>) {
        for stmt in stmts {
            self.walk_statement(stmt, calls);
        }
    }

    fn walk_expression(&self, expr: &Expression, calls: &mut Vec<(String, String)>) {
        match expr {
            Expression::CallExpression(call) => {
                let callee = self.call_callee_name(call);
                calls.push((callee, String::new()));
            }
            Expression::NewExpression(new_expr) => {
                let callee = self.expr_name(&new_expr.callee);
                calls.push((callee, String::new()));
            }
            Expression::ArrowFunctionExpression(arrow) => {
                self.walk_block(&arrow.body.statements, calls);
            }
            _ => {}
        }
    }
}

impl DiffAnalyzer for JavaScriptDiffAnalyzer {
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
            self.walk_statement(stmt, &mut calls);
        }
        calls.sort();
        calls.dedup();
        calls
    }
}
