use super::{ApiContract, ApiExport, Param, Symbol, SymbolIndex, SymbolParam, Visibility};
use crate::util;
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    ArrowFunctionExpression, BindingPattern, Class, ClassElement, Declaration,
    ExportDefaultDeclarationKind, ExportNamedDeclaration, Expression, Function,
    MethodDefinitionKind, PropertyKey, Statement,
};
use oxc_parser::{Parser, ParserReturn};

/// Extract symbols and API contracts from a TypeScript file.
pub fn extract(
    module: &str,
    source: &str,
    file_path: Option<&Path>,
) -> anyhow::Result<(SymbolIndex, ApiContract)> {
    let source_type = util::detect_source_type(file_path);

    let allocator = Allocator::default();
    let ParserReturn { program, panicked, .. } =
        Parser::new(&allocator, source, source_type).parse();

    if panicked {
        return Ok((SymbolIndex { module: module.to_string(), symbols: vec![] }, ApiContract { module: module.to_string(), exports: vec![] }));
    }

    let mut extractor = SymbolExtractor {
        module: module.to_string(),
        source,
        symbols: Vec::new(),
        exports: Vec::new(),
    };

    for stmt in &program.body {
        extractor.handle_statement(stmt);
    }

    Ok((
        SymbolIndex {
            module: module.to_string(),
            symbols: extractor.symbols,
        },
        ApiContract {
            module: module.to_string(),
            exports: extractor.exports,
        },
    ))
}

struct SymbolExtractor<'a> {
    module: String,
    source: &'a str,
    symbols: Vec<Symbol>,
    exports: Vec<ApiExport>,
}

impl<'a> SymbolExtractor<'a> {
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

    // ── Functions ─────────────────────────────────────────────────

    fn handle_function(&mut self, func: &Function<'a>, exported: bool) {
        if let Some(id) = &func.id {
            self.emit_function(id.name.to_string(), func, exported);
        }
    }

    fn emit_function(&mut self, name: String, func: &Function<'a>, exported: bool) {
        let line_range = self.line_range(func.span.start, func.span.end);
        let params = extract_params(self.source, func);
        let return_type = func
            .return_type
            .as_ref()
            .map(|ann| trim_type_annotation(self.source, ann.span));
        let generics = extract_generics_option(&func.type_parameters);
        let sig = format_function_signature(self.source, &name, func);
        
        let symbol_params: Vec<SymbolParam> = params.iter().map(|p| SymbolParam {
            name: p.name.clone(),
            ty: p.ty.clone(),
            optional: p.optional,
            default_value: None,
        }).collect();

        self.add_symbol(
            name.clone(),
            "function",
            line_range,
            vec![],
            Some(if exported { Visibility::Public } else { Visibility::Private }),
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

    // ── Classes ───────────────────────────────────────────────────

    fn handle_class(&mut self, class: &Class<'a>, exported: bool) {
        if let Some(id) = &class.id {
            self.emit_class(id.name.to_string(), class, exported);
        }
    }

    fn emit_class(&mut self, name: String, class: &Class<'a>, exported: bool) {
        let line_range = self.line_range(class.span.start, class.span.end);
        let generics = extract_generics_option(&class.type_parameters);

        let mut children = Vec::new();
        let mut methods = Vec::new();
        let mut constructor_params = Vec::new();

        for element in &class.body.body {
            match element {
                ClassElement::MethodDefinition(method) => {
                    let method_name = prop_key_to_string(&method.key);
                    let method_range = self.line_range(method.span.start, method.span.end);
                    let ps = extract_params(self.source, &method.value);
                    let rt = method
                        .value
                        .return_type
                        .as_ref()
                        .map(|ann| trim_type_annotation(self.source, ann.span));
                    let symbol_params: Vec<SymbolParam> = ps.iter().map(|p| SymbolParam {
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                        optional: p.optional,
                        default_value: None,
                    }).collect();
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
                        generics: extract_generics_option(&method.value.type_parameters),
                        signature: format_method_signature(
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
                    let prop_name = prop_key_to_string(&prop.key);
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
                constructor_params = extract_params(self.source, &method.value);
            }
        }

        self.add_symbol(
            name.clone(),
            "class",
            line_range,
            children,
            Some(if exported { Visibility::Public } else { Visibility::Private }),
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

    // ── Variables ─────────────────────────────────────────────────

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
                params = extract_from_arrow_params(self.source, arrow);
                return_type = arrow
                    .return_type
                    .as_ref()
                    .map(|ann| trim_type_annotation(self.source, ann.span));
                is_async = Some(arrow.r#async);
                symbol_params = Some(params.iter().map(|p| SymbolParam {
                    name: p.name.clone(),
                    ty: p.ty.clone(),
                    optional: p.optional,
                    default_value: None,
                }).collect());
            } else if let Expression::FunctionExpression(func) = init {
                params = extract_params(self.source, func);
                return_type = func
                    .return_type
                    .as_ref()
                    .map(|ann| trim_type_annotation(self.source, ann.span));
                is_async = Some(func.r#async);
                symbol_params = Some(params.iter().map(|p| SymbolParam {
                    name: p.name.clone(),
                    ty: p.ty.clone(),
                    optional: p.optional,
                    default_value: None,
                }).collect());
            }
        }

        self.add_symbol(
            name.clone(),
            "variable",
            line_range,
            vec![],
            Some(if exported { Visibility::Public } else { Visibility::Private }),
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

    // ── Type Aliases ──────────────────────────────────────────────

    fn handle_type_alias(
        &mut self,
        alias: &oxc_ast::ast::TSTypeAliasDeclaration<'a>,
        exported: bool,
    ) {
        let name = alias.id.name.to_string();
        let line_range = self.line_range(alias.span.start, alias.span.end);
        let generics = extract_generics_option(&alias.type_parameters);

        let type_text = extract_type_source(self.source, alias.span);

        self.add_symbol(
            name.clone(),
            "type_alias",
            line_range,
            vec![],
            Some(if exported { Visibility::Public } else { Visibility::Private }),
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
            signature: format!("export type {} = {}", alias.id.name, type_text,),
            params: vec![],
            return_type: None,
            description: None,
            line_range,
            partial_analysis: false,
        });
    }

    // ── Interfaces ────────────────────────────────────────────────

    fn handle_interface(
        &mut self,
        interface: &oxc_ast::ast::TSInterfaceDeclaration<'a>,
        exported: bool,
    ) {
        let name = interface.id.name.to_string();
        let line_range = self.line_range(interface.span.start, interface.span.end);
        let generics = extract_generics_option(&interface.type_parameters);

        let mut children = Vec::new();
        let mut members = Vec::new();

        for member in &interface.body.body {
            use oxc_ast::ast::TSSignature;
            if let TSSignature::TSPropertySignature(prop) = member {
                let prop_name = prop_key_to_string(&prop.key);
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
            Some(if exported { Visibility::Public } else { Visibility::Private }),
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

    // ── Enums ─────────────────────────────────────────────────────

    fn handle_enum(&mut self, enum_decl: &oxc_ast::ast::TSEnumDeclaration<'a>, exported: bool) {
        let name = enum_decl.id.name.to_string();
        let line_range = self.line_range(enum_decl.span.start, enum_decl.span.end);

        self.add_symbol(
            name.clone(),
            "enum",
            line_range,
            vec![],
            Some(if exported { Visibility::Public } else { Visibility::Private }),
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

// ── Helper functions ───────────────────────────────────────────────

fn bind_name(pattern: &BindingPattern) -> String {
    match pattern {
        BindingPattern::BindingIdentifier(id) => id.name.to_string(),
        BindingPattern::AssignmentPattern(assign) => bind_name(&assign.left),
        _ => "_".to_string(),
    }
}

fn extract_generics_option<'a>(
    type_params: &Option<oxc_allocator::Box<'a, oxc_ast::ast::TSTypeParameterDeclaration<'a>>>,
) -> Vec<String> {
    type_params
        .as_ref()
        .map(|tp| tp.params.iter().map(|p| p.name.to_string()).collect())
        .unwrap_or_default()
}

fn extract_params<'a>(source: &str, func: &Function<'a>) -> Vec<Param> {
    func.params
        .items
        .iter()
        .map(|p| {
            let name = bind_name(&p.pattern);
            let ty = p
                .type_annotation
                .as_ref()
                .map(|ann| trim_type_annotation(source, ann.span))
                .unwrap_or_else(|| "unknown".to_string());
            Param {
                name,
                ty,
                optional: p.optional,
            }
        })
        .collect()
}

fn extract_from_arrow_params(source: &str, arrow: &ArrowFunctionExpression) -> Vec<Param> {
    arrow
        .params
        .items
        .iter()
        .map(|p| {
            let name = bind_name(&p.pattern);
            let ty = p
                .type_annotation
                .as_ref()
                .map(|ann| trim_type_annotation(source, ann.span))
                .unwrap_or_else(|| "unknown".to_string());
            Param {
                name,
                ty,
                optional: p.optional,
            }
        })
        .collect()
}

/// Extract type text from a TSTypeAnnotation span, stripping the leading `: ` prefix.
fn trim_type_annotation(source: &str, span: oxc_span::Span) -> String {
    let text = &source[span.start as usize..span.end as usize];
    text.trim_start_matches(':').trim().to_string()
}

fn prop_key_to_string(key: &PropertyKey) -> String {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
        PropertyKey::PrivateIdentifier(id) => format!("#{}", id.name),
        _ => "[expr]".to_string(),
    }
}

fn format_function_signature(source: &str, name: &str, func: &Function) -> String {
    let ps = func
        .params
        .items
        .iter()
        .map(|p| {
            let n = bind_name(&p.pattern);
            let ty = p
                .type_annotation
                .as_ref()
                .map(|ann| trim_type_annotation(source, ann.span))
                .unwrap_or_else(|| "unknown".to_string());
            format!("{}: {}", n, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let rt = func
        .return_type
        .as_ref()
        .map(|ann| trim_type_annotation(source, ann.span))
        .unwrap_or_else(|| "void".to_string());

    format!("export function {}({}) -> {}", name, ps, rt)
}

fn format_method_signature(
    source: &str,
    class_name: &str,
    key: &PropertyKey,
    func: &Function,
) -> String {
    let method_name = prop_key_to_string(key);
    let ps = func
        .params
        .items
        .iter()
        .map(|p| {
            let n = bind_name(&p.pattern);
            let ty = p
                .type_annotation
                .as_ref()
                .map(|ann| trim_type_annotation(source, ann.span))
                .unwrap_or_else(|| "unknown".to_string());
            format!("{}: {}", n, ty)
        })
        .collect::<Vec<_>>()
        .join(", ");

    let rt = func
        .return_type
        .as_ref()
        .map(|ann| trim_type_annotation(source, ann.span))
        .unwrap_or_else(|| "void".to_string());

    format!("{}::{}({}) -> {}", class_name, method_name, ps, rt)
}

/// Extract the type expression text from a TSTypeAliasDeclaration.
fn extract_type_source(source: &str, alias_span: oxc_span::Span) -> String {
    let text = &source[alias_span.start as usize..alias_span.end as usize];
    // Find '=' and return everything after it (trimmed)
    if let Some(eq_pos) = text.find('=') {
        text[eq_pos + 1..].trim().to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_extract_exported_function() {
        let source = "export function greet(name: string): string { return \"hello\"; }";
        let (index, contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "greet");
        assert_eq!(index.symbols[0].kind, "function");
        assert_eq!(contract.exports.len(), 1);
        assert_eq!(contract.exports[0].name, "greet");
    }

    #[test]
    fn test_extract_private_function() {
        let source = "function hidden() { return 42; }";
        let (index, contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert!(contract.exports.is_empty());
    }

    #[test]
    fn test_extract_exported_class() {
        let source = "export class MyClass { constructor() {} greet(): void {} }";
        let (index, _contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "MyClass");
        assert_eq!(index.symbols[0].kind, "class");
    }

    #[test]
    fn test_extract_exported_interface() {
        let source = "export interface User { name: string; age: number; }";
        let (index, _contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "User");
        assert_eq!(index.symbols[0].kind, "interface");
    }

    #[test]
    fn test_extract_type_alias() {
        let source = "export type Callback = (x: number) => void;";
        let (index, _contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "Callback");
        assert_eq!(index.symbols[0].kind, "type_alias");
    }

    #[test]
    fn test_extract_enum() {
        let source = "export enum Color { Red, Green, Blue }";
        let (index, _contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "Color");
        assert_eq!(index.symbols[0].kind, "enum");
    }

    #[test]
    fn test_extract_exported_variable() {
        let source = "export const VERSION = \"1.0.0\";";
        let (index, _contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "VERSION");
        assert_eq!(index.symbols[0].kind, "variable");
    }

    #[test]
    fn test_extract_arrow_function() {
        let source = "export const add = (a: number, b: number): number => a + b;";
        let (index, contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "add");
        assert_eq!(index.symbols[0].kind, "variable");
        assert!(contract.exports[0].params.len() == 2);
    }

    #[test]
    fn test_extract_export_default_function() {
        let source = "export default function() { return true; }";
        let (index, contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 1);
        assert_eq!(index.symbols[0].name, "default");
        assert_eq!(contract.exports.len(), 1);
        assert_eq!(contract.exports[0].name, "default");
    }

    #[test]
    fn test_extract_empty_source() {
        let source = "";
        let (index, contract) = extract("empty.ts", source, Some(Path::new("empty.ts"))).unwrap();
        assert!(index.symbols.is_empty());
        assert!(contract.exports.is_empty());
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let source = "export function foo() {} export function bar() {}";
        let (index, contract) = extract("test.ts", source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(index.symbols.len(), 2);
        assert_eq!(contract.exports.len(), 2);
    }
}

