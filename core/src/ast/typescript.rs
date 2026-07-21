use crate::language::Diagnostic;
use crate::parser::ModuleReferences;
use crate::symbols::{
    ApiContract, ApiExport, Symbol, SymbolParam, Visibility,
};
use crate::util;
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration,
    ExportDefaultDeclarationKind, ExportNamedDeclaration, Expression, Function,
    MethodDefinitionKind, Statement,
};
use oxc_parser::{Parser, ParserReturn};

use super::AstOutput;

/// Parse a TypeScript file and extract symbols, API contracts, and imports
/// in a single AST walk.
pub fn parse(source: &str, file_path: Option<&Path>) -> anyhow::Result<AstOutput> {
    let module = file_path
        .and_then(|p| p.to_str())
        .unwrap_or("unknown")
        .replace('\\', "/");
    let source_type = util::detect_source_type(file_path);

    let allocator = Allocator::default();
    let ParserReturn { program, panicked, .. } =
        Parser::new(&allocator, source, source_type).parse();

    let mut extractor = UnifiedExtractor {
        module: module.clone(),
        source,
        symbols: Vec::new(),
        exports: Vec::new(),
        relative_imports: Vec::new(),
        external_imports: Vec::new(),
    };

    if !panicked {
        for stmt in &program.body {
            extractor.handle_statement(stmt);
            extractor.handle_import_from_statement(stmt);
        }
    }

    // Deduplicate imports
    extractor.relative_imports.sort();
    extractor.relative_imports.dedup();
    extractor.external_imports.sort();
    extractor.external_imports.dedup();

    Ok(AstOutput {
        file_path: module.clone(),
        source: source.to_string(),
        language: "typescript".to_string(),
        symbols: extractor.symbols,
        exports: extractor.exports.clone(),
        api_contract: ApiContract {
            module,
            exports: extractor.exports,
        },
        imports: ModuleReferences {
            relative_imports: extractor.relative_imports,
            external_imports: extractor.external_imports,
        },
        diagnostics: if panicked {
            vec![Diagnostic {
                message: "Parser panicked".to_string(),
                line: 0,
                column: 0,
                severity: crate::language::DiagnosticSeverity::Error,
            }]
        } else {
            vec![]
        },
    })
}

struct UnifiedExtractor<'a> {
    module: String,
    source: &'a str,
    symbols: Vec<Symbol>,
    exports: Vec<ApiExport>,
    relative_imports: Vec<String>,
    external_imports: Vec<String>,
}

impl<'a> UnifiedExtractor<'a> {
    fn symbol_id(&self, name: &str) -> String {
        format!("{}:{}", self.module, name)
    }

    fn line_range(&self, start: u32, end: u32) -> [usize; 2] {
        let start_line = self.source[..start as usize].matches('\n').count() + 1;
        let end_line = self.source[..end as usize].matches('\n').count() + 1;
        [start_line, end_line]
    }

    #[expect(clippy::too_many_arguments)]
    fn add_symbol(
        &mut self,
        name: String,
        kind: &str,
        line_range: [usize; 2],
        children: Vec<Symbol>,
        visibility: Option<Visibility>,
        value: Option<String>,
        signature: Option<String>,
        doc_comment: Option<String>,
        attributes: Vec<String>,
        is_async: Option<bool>,
        return_type: Option<String>,
        params: Option<Vec<SymbolParam>>,
    ) {
        let id = self.symbol_id(&name);
        self.symbols.push(Symbol {
            id,
            name,
            kind: kind.to_string(),
            line_range,
            children,
            partial_analysis: false,
            partial_reason: None,
            visibility,
            value,
            signature,
            doc_comment,
            attributes,
            is_async,
            return_type,
            params,
        });
    }

    fn add_export(&mut self, export: ApiExport) {
        self.exports.push(export);
    }

    // ── Symbol extraction (from symbols/typescript.rs) ──────────────

    fn handle_statement(&mut self, stmt: &Statement<'a>) {
        match stmt {
            Statement::ExportNamedDeclaration(export) => {
                self.handle_export_named(export);
            }
            Statement::ExportDefaultDeclaration(default) => {
                self.handle_export_default(default);
            }
            Statement::FunctionDeclaration(func) => {
                self.handle_function(func, false);
            }
            Statement::ClassDeclaration(class) => {
                self.handle_class(class, false);
            }
            Statement::VariableDeclaration(var_decl) => {
                for decl in &var_decl.declarations {
                    self.handle_var_declarator(decl, false);
                }
            }
            Statement::TSTypeAliasDeclaration(alias) => {
                self.handle_type_alias(alias, false);
            }
            Statement::TSInterfaceDeclaration(interface) => {
                self.handle_interface(interface, false);
            }
            Statement::TSEnumDeclaration(enum_decl) => {
                self.handle_enum(enum_decl, false);
            }
            _ => {}
        }
    }

    // ── Import extraction (from parser/typescript.rs) ──────────────

    fn handle_import_from_statement(&mut self, stmt: &Statement<'a>) {
        match stmt {
            Statement::ImportDeclaration(import) => {
                let src = import.source.value.to_string();
                self.classify_import(&src);
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(source) = &export.source {
                    let src = source.value.to_string();
                    self.classify_import(&src);
                }
            }
            Statement::ExportAllDeclaration(export) => {
                let src = export.source.value.to_string();
                self.classify_import(&src);
            }
            _ => {}
        }
    }

    fn classify_import(&mut self, src: &str) {
        if src.starts_with('.') || src.starts_with('/') {
            self.relative_imports.push(src.to_string());
        } else {
            self.external_imports.push(src.to_string());
        }
    }

    // ── Handlers (transplanted from symbols/typescript.rs) ──────────

    fn handle_export_named(&mut self, export: &ExportNamedDeclaration<'a>) {
        if let Some(decl) = &export.declaration {
            self.handle_declaration(decl);
        }
    }

    fn handle_declaration(&mut self, decl: &Declaration<'a>) {
        match decl {
            Declaration::FunctionDeclaration(func) => self.handle_function(func, true),
            Declaration::ClassDeclaration(class) => self.handle_class(class, true),
            Declaration::VariableDeclaration(var_decl) => {
                for d in &var_decl.declarations {
                    self.handle_var_declarator(d, true);
                }
            }
            Declaration::TSTypeAliasDeclaration(alias) => self.handle_type_alias(alias, true),
            Declaration::TSInterfaceDeclaration(interface) => {
                self.handle_interface(interface, true);
            }
            Declaration::TSEnumDeclaration(enum_decl) => self.handle_enum(enum_decl, true),
            _ => {}
        }
    }

    fn handle_export_default(&mut self, default: &oxc_ast::ast::ExportDefaultDeclaration<'a>) {
        match &default.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(func) => {
                let name = func
                    .id
                    .as_ref()
                    .map(|id| id.name.to_string())
                    .unwrap_or_else(|| "default".to_string());
                self.emit_function(name, func, true);
            }
            ExportDefaultDeclarationKind::ClassDeclaration(class) => {
                let name = class
                    .id
                    .as_ref()
                    .map(|id| id.name.to_string())
                    .unwrap_or_else(|| "default".to_string());
                self.emit_class(name, class, true);
            }
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(interface) => {
                self.handle_interface(interface, true);
            }
            _ => {
                self.add_export(ApiExport {
                    name: "default".to_string(),
                    kind: "default_export".to_string(),
                    generics: vec![],
                    signature: "export default ...".to_string(),
                    params: vec![],
                    return_type: None,
                    description: None,
                    line_range: self.line_range(default.span.start, default.span.end),
                    partial_analysis: true,
                });
            }
        }
    }

    fn handle_function(&mut self, func: &Function<'a>, exported: bool) {
        if let Some(id) = &func.id {
            self.emit_function(id.name.to_string(), func, exported);
        }
    }

    fn emit_function(&mut self, name: String, func: &Function<'a>, exported: bool) {
        let line_range = self.line_range(func.span.start, func.span.end);
        let params = crate::symbols::typescript::extract_params(self.source, func);
        let return_type = func
            .return_type
            .as_ref()
            .map(|ann| crate::symbols::typescript::trim_type_annotation(self.source, ann.span));
        let generics = crate::symbols::typescript::extract_generics_option(&func.type_parameters);
        let sig = crate::symbols::typescript::format_function_signature(self.source, &name, func);

        let symbol_params: Vec<SymbolParam> = params
            .iter()
            .map(|p| SymbolParam {
                name: p.name.clone(),
                ty: p.ty.clone(),
                optional: p.optional,
                default_value: None,
            })
            .collect();

        self.add_symbol(
            name.clone(),
            "function",
            line_range,
            vec![],
            Some(if exported {
                Visibility::Public
            } else {
                Visibility::Private
            }),
            None,
            Some(sig.clone()),
            None,
            Vec::new(),
            Some(func.r#async),
            return_type.clone(),
            Some(symbol_params),
        );
        if !exported {
            return;
        }
        self.add_export(ApiExport {
            name,
            kind: "function".to_string(),
            generics,
            signature: sig,
            params,
            return_type,
            description: None,
            line_range,
            partial_analysis: false,
        });
    }

    fn handle_class(&mut self, class: &Class<'a>, exported: bool) {
        if let Some(id) = &class.id {
            self.emit_class(id.name.to_string(), class, exported);
        }
    }

    fn emit_class(&mut self, name: String, class: &Class<'a>, exported: bool) {
        let line_range = self.line_range(class.span.start, class.span.end);
        let generics = crate::symbols::typescript::extract_generics_option(&class.type_parameters);

        let mut children = Vec::new();
        let mut methods = Vec::new();
        let mut constructor_params = Vec::new();

        for element in &class.body.body {
            match element {
                ClassElement::MethodDefinition(method) => {
                    let method_name = crate::symbols::typescript::prop_key_to_string(&method.key);
                    let method_range = self.line_range(method.span.start, method.span.end);
                    let ps = crate::symbols::typescript::extract_params(self.source, &method.value);
                    let rt = method
                        .value
                        .return_type
                        .as_ref()
                        .map(|ann| {
                            crate::symbols::typescript::trim_type_annotation(self.source, ann.span)
                        });
                    let symbol_params: Vec<SymbolParam> = ps
                        .iter()
                        .map(|p| SymbolParam {
                            name: p.name.clone(),
                            ty: p.ty.clone(),
                            optional: p.optional,
                            default_value: None,
                        })
                        .collect();
                    children.push(Symbol {
                        id: format!("{}:{}", self.symbol_id(&name), method_name),
                        name: method_name.clone(),
                        kind: "method".to_string(),
                        line_range: method_range,
                        children: vec![],
                        partial_analysis: false,
                        partial_reason: None,
                        visibility: Some(Visibility::Public),
                        value: None,
                        signature: None,
                        doc_comment: None,
                        attributes: Vec::new(),
                        is_async: Some(method.value.r#async),
                        return_type: rt.clone(),
                        params: Some(symbol_params),
                    });
                    methods.push(ApiExport {
                        name: method_name,
                        kind: "method".to_string(),
                        generics: crate::symbols::typescript::extract_generics_option(
                            &method.value.type_parameters,
                        ),
                        signature: crate::symbols::typescript::format_method_signature(
                            self.source,
                            &name,
                            &method.key,
                            &method.value,
                        ),
                        params: ps,
                        return_type: rt,
                        description: None,
                        line_range: method_range,
                        partial_analysis: false,
                    });
                }
                ClassElement::PropertyDefinition(prop) => {
                    let prop_name =
                        crate::symbols::typescript::prop_key_to_string(&prop.key);
                    let value = prop.value.as_ref().map(|_e| "".to_string());
                    children.push(Symbol {
                        id: format!("{}:{}", self.symbol_id(&name), prop_name),
                        name: prop_name,
                        kind: "property".to_string(),
                        line_range: self.line_range(prop.span.start, prop.span.end),
                        children: vec![],
                        partial_analysis: false,
                        partial_reason: None,
                        visibility: Some(Visibility::Public),
                        value,
                        signature: None,
                        doc_comment: None,
                        attributes: Vec::new(),
                        is_async: None,
                        return_type: None,
                        params: None,
                    });
                }
                ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => {}
                _ => {}
            }

            if let ClassElement::MethodDefinition(method) = element
                && method.kind == MethodDefinitionKind::Constructor
            {
                constructor_params =
                    crate::symbols::typescript::extract_params(self.source, &method.value);
            }
        }

        self.add_symbol(
            name.clone(),
            "class",
            line_range,
            children,
            Some(if exported {
                Visibility::Public
            } else {
                Visibility::Private
            }),
            None,
            None,
            None,
            Vec::new(),
            None,
            None,
            None,
        );

        if !exported {
            return;
        }

        self.add_export(ApiExport {
            name: name.clone(),
            kind: "class".to_string(),
            generics,
            signature: format!("export class {} {{ ... }}", name),
            params: constructor_params,
            return_type: None,
            description: None,
            line_range,
            partial_analysis: false,
        });
        for m in methods {
            self.add_export(m);
        }
    }

    fn handle_var_declarator(
        &mut self,
        decl: &oxc_ast::ast::VariableDeclarator<'a>,
        exported: bool,
    ) {
        let name = match &decl.id {
            BindingPattern::BindingIdentifier(id) => id.name.to_string(),
            _ => return,
        };

        let line_range = self.line_range(decl.span.start, decl.span.end);
        let ty = "unknown".to_string();
        let sig = format!("export const {}: {}", name, ty);

        let value = decl.init.as_ref().map(|_e| "".to_string());

        let mut params = vec![];
        let mut return_type = None;
        let mut is_async = None;
        let mut symbol_params: Option<Vec<SymbolParam>> = None;

        if let Some(init) = &decl.init {
            if let Expression::ArrowFunctionExpression(arrow) = init {
                params = crate::symbols::typescript::extract_from_arrow_params(self.source, arrow);
                return_type = arrow
                    .return_type
                    .as_ref()
                    .map(|ann| {
                        crate::symbols::typescript::trim_type_annotation(self.source, ann.span)
                    });
                is_async = Some(arrow.r#async);
                symbol_params = Some(
                    params
                        .iter()
                        .map(|p| SymbolParam {
                            name: p.name.clone(),
                            ty: p.ty.clone(),
                            optional: p.optional,
                            default_value: None,
                        })
                        .collect(),
                );
            } else if let Expression::FunctionExpression(func) = init {
                params = crate::symbols::typescript::extract_params(self.source, func);
                return_type = func
                    .return_type
                    .as_ref()
                    .map(|ann| {
                        crate::symbols::typescript::trim_type_annotation(self.source, ann.span)
                    });
                is_async = Some(func.r#async);
                symbol_params = Some(
                    params
                        .iter()
                        .map(|p| SymbolParam {
                            name: p.name.clone(),
                            ty: p.ty.clone(),
                            optional: p.optional,
                            default_value: None,
                        })
                        .collect(),
                );
            }
        }

        self.add_symbol(
            name.clone(),
            "variable",
            line_range,
            vec![],
            Some(if exported {
                Visibility::Public
            } else {
                Visibility::Private
            }),
            value,
            None,
            None,
            Vec::new(),
            is_async,
            return_type.clone(),
            symbol_params,
        );
        if !exported {
            return;
        }

        self.add_export(ApiExport {
            name,
            kind: "variable".to_string(),
            generics: vec![],
            signature: sig,
            params,
            return_type,
            description: None,
            line_range,
            partial_analysis: false,
        });
    }

    fn handle_type_alias(
        &mut self,
        alias: &oxc_ast::ast::TSTypeAliasDeclaration<'a>,
        exported: bool,
    ) {
        let name = alias.id.name.to_string();
        let line_range = self.line_range(alias.span.start, alias.span.end);
        let generics =
            crate::symbols::typescript::extract_generics_option(&alias.type_parameters);

        let type_text = crate::symbols::typescript::extract_type_source(self.source, alias.span);

        self.add_symbol(
            name.clone(),
            "type_alias",
            line_range,
            vec![],
            Some(if exported {
                Visibility::Public
            } else {
                Visibility::Private
            }),
            Some(type_text.clone()),
            None,
            None,
            Vec::new(),
            None,
            None,
            None,
        );
        if !exported {
            return;
        }

        self.add_export(ApiExport {
            name,
            kind: "type_alias".to_string(),
            generics,
            signature: format!("export type {} = {}", alias.id.name, type_text),
            params: vec![],
            return_type: None,
            description: None,
            line_range,
            partial_analysis: false,
        });
    }

    fn handle_interface(
        &mut self,
        interface: &oxc_ast::ast::TSInterfaceDeclaration<'a>,
        exported: bool,
    ) {
        let name = interface.id.name.to_string();
        let line_range = self.line_range(interface.span.start, interface.span.end);
        let generics =
            crate::symbols::typescript::extract_generics_option(&interface.type_parameters);

        let mut children = Vec::new();
        let mut members = Vec::new();

        for member in &interface.body.body {
            use oxc_ast::ast::TSSignature;
            if let TSSignature::TSPropertySignature(prop) = member {
                let prop_name = crate::symbols::typescript::prop_key_to_string(&prop.key);
                let prop_range = self.line_range(prop.span.start, prop.span.end);
                let prop_ty = prop.type_annotation.as_ref().map(|ann| {
                    self.source[ann.span.start as usize..ann.span.end as usize].to_string()
                });
                children.push(Symbol {
                    id: format!("{}:{}", self.symbol_id(&name), prop_name),
                    name: prop_name.clone(),
                    kind: "property".to_string(),
                    line_range: prop_range,
                    children: vec![],
                    partial_analysis: false,
                    partial_reason: None,
                    visibility: Some(Visibility::Public),
                    value: None,
                    signature: None,
                    doc_comment: None,
                    attributes: Vec::new(),
                    is_async: None,
                    return_type: prop_ty.clone(),
                    params: None,
                });
                members.push(ApiExport {
                    name: prop_name.clone(),
                    kind: "property".to_string(),
                    generics: vec![],
                    signature: format!(
                        "{}{}: {}",
                        prop_name,
                        if prop.optional { "?" } else { "" },
                        prop_ty.as_deref().unwrap_or("unknown"),
                    ),
                    params: vec![],
                    return_type: prop_ty,
                    description: None,
                    line_range: prop_range,
                    partial_analysis: false,
                });
            }
        }

        self.add_symbol(
            name.clone(),
            "interface",
            line_range,
            children,
            Some(if exported {
                Visibility::Public
            } else {
                Visibility::Private
            }),
            None,
            None,
            None,
            Vec::new(),
            None,
            None,
            None,
        );
        if !exported {
            return;
        }

        self.add_export(ApiExport {
            name,
            kind: "interface".to_string(),
            generics,
            signature: format!("export interface {} {{ ... }}", interface.id.name),
            params: vec![],
            return_type: None,
            description: None,
            line_range,
            partial_analysis: false,
        });
        for m in members {
            self.add_export(m);
        }
    }

    fn handle_enum(
        &mut self,
        enum_decl: &oxc_ast::ast::TSEnumDeclaration<'a>,
        exported: bool,
    ) {
        let name = enum_decl.id.name.to_string();
        let line_range = self.line_range(enum_decl.span.start, enum_decl.span.end);

        self.add_symbol(
            name.clone(),
            "enum",
            line_range,
            vec![],
            Some(if exported {
                Visibility::Public
            } else {
                Visibility::Private
            }),
            None,
            None,
            None,
            Vec::new(),
            None,
            None,
            None,
        );
        if !exported {
            return;
        }

        self.add_export(ApiExport {
            name,
            kind: "enum".to_string(),
            generics: vec![],
            signature: format!("export enum {} {{ ... }}", enum_decl.id.name),
            params: vec![],
            return_type: None,
            description: None,
            line_range,
            partial_analysis: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_exported_function() {
        let source = "export function greet(name: string): string { return \"hello\"; }";
        let result = parse(source, Some(std::path::Path::new("test.ts"))).unwrap();
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "greet");
        assert_eq!(result.symbols[0].kind, "function");
        assert_eq!(result.exports.len(), 1);
        assert_eq!(result.exports[0].name, "greet");
    }

    #[test]
    fn test_parse_with_imports() {
        let source =
            "import { Component } from 'react';\nimport { helper } from './utils';\nexport function foo() {}";
        let result = parse(source, Some(std::path::Path::new("test.tsx"))).unwrap();
        assert!(result.imports.relative_imports.contains(&"./utils".to_string()));
        assert!(result.imports.external_imports.contains(&"react".to_string()));
        assert_eq!(result.symbols.len(), 1);
    }

    #[test]
    fn test_parse_no_imports() {
        let source = "const x: number = 1;\nconsole.log(x);";
        let result = parse(source, Some(std::path::Path::new("test.ts"))).unwrap();
        assert!(result.imports.relative_imports.is_empty());
        assert!(result.imports.external_imports.is_empty());
    }
}
