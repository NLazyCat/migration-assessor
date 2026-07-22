use super::{ApiContract, ApiExport, Param, Symbol, SymbolIndex, SymbolParam, Visibility};
use crate::util;
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::{
    BindingPattern, Class, ClassElement, Declaration, ExportDefaultDeclarationKind,
    Function, MethodDefinitionKind, PropertyKey, Statement,
};
use oxc_parser::{Parser, ParserReturn};

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

fn bind_name(pattern: &BindingPattern) -> String {
    match pattern {
        BindingPattern::BindingIdentifier(id) => id.name.to_string(),
        BindingPattern::AssignmentPattern(assign) => bind_name(&assign.left),
        _ => "_".to_string(),
    }
}

fn line_range_of_span(source: &str, span: oxc_span::Span) -> [usize; 2] {
    let start = source[..span.start as usize].matches('\n').count() + 1;
    let end = source[..span.end as usize].matches('\n').count() + 1;
    [start, end]
}

impl<'a> SymbolExtractor<'a> {
    fn symbol_id(&self, name: &str) -> String {
        format!("{}:{}", self.module, name)
    }

    fn handle_statement(&mut self, stmt: &Statement<'a>) {
        match stmt {
            Statement::ExportDefaultDeclaration(decl) => {
                let name = match &decl.declaration {
                    ExportDefaultDeclarationKind::FunctionExpression(f) => {
                        f.id.as_ref().map(|id| id.name.to_string())
                    }
                    ExportDefaultDeclarationKind::ClassExpression(c) => {
                        c.id.as_ref().map(|id| id.name.to_string())
                    }
                    _ => None,
                };
                let name = name.unwrap_or_else(|| "default".to_string());

                let (kind, sig, params, return_type, line_range) = match &decl.declaration {
                    ExportDefaultDeclarationKind::FunctionExpression(f) => {
                        Self::extract_function_info(self.source, f)
                    }
                    ExportDefaultDeclarationKind::ClassExpression(c) => {
                        ("class".into(), String::new(), Vec::new(), None, line_range_of_span(self.source, c.span))
                    }
                    _ => ("expression".to_string(), String::new(), Vec::<Param>::new(), None, [1, 1]),
                };

                self.exports.push(ApiExport {
                    name: name.clone(),
                    kind,
                    generics: vec![],
                    signature: sig,
                    params,
                    return_type,
                    description: None,
                    line_range,
                    partial_analysis: false,
                });

                if let ExportDefaultDeclarationKind::ClassExpression(c) = &decl.declaration {
                    self.handle_class(c);
                }
            }
            Statement::ExportNamedDeclaration(decl) => {
                if let Some(declaration) = &decl.declaration {
                    match declaration {
                        Declaration::VariableDeclaration(var) => {
                            for var_decl in &var.declarations {
                                let name = bind_name(&var_decl.id);
                                if name != "_" {
                                    let sig = format!("const {}: unknown", name);
                                    self.symbols.push(Symbol {
                                        id: self.symbol_id(&name),
                                        name: name.clone(),
                                        kind: "variable".to_string(),
                                        line_range: line_range_of_span(self.source, var_decl.span),
                                        children: vec![],
                                        partial_analysis: false,
                                        partial_reason: None,
                                        visibility: Some(Visibility::Public),
                                        value: None,
                                        signature: Some(sig.clone()),
                                        doc_comment: None,
                                        attributes: vec![],
                                        is_async: None,
                                        return_type: None,
                                        params: None,
                                    });
                                    self.exports.push(ApiExport {
                                        name,
                                        kind: "variable".to_string(),
                                        generics: vec![],
                                        signature: sig,
                                        params: vec![],
                                        return_type: None,
                                        description: None,
                                        line_range: line_range_of_span(self.source, var_decl.span),
                                        partial_analysis: false,
                                    });
                                }
                            }
                        }
                        Declaration::FunctionDeclaration(f) => {
                            if let Some(id) = &f.id {
                                let (kind, sig, params, return_type, line_range) =
                                    Self::extract_function_info(self.source, f);
                                let name = id.name.to_string();
                                let params_sym: Vec<SymbolParam> = params.iter().map(|p| SymbolParam {
                                    name: p.name.clone(),
                                    ty: p.ty.clone(),
                                    optional: p.optional,
                                    default_value: None,
                                }).collect();
                                self.symbols.push(Symbol {
                                    id: self.symbol_id(&name),
                                    name: name.clone(),
                                    kind: kind.clone(),
                                    line_range,
                                    children: vec![],
                                    partial_analysis: false,
                                    partial_reason: None,
                                    visibility: Some(Visibility::Public),
                                    value: None,
                                    signature: Some(sig.clone()),
                                    doc_comment: None,
                                    attributes: vec![],
                                    is_async: Some(f.r#async),
                                    return_type: return_type.clone(),
                                    params: Some(params_sym),
                                });
                                self.exports.push(ApiExport {
                                    name,
                                    kind,
                                    generics: vec![],
                                    signature: sig,
                                    params,
                                    return_type,
                                    description: None,
                                    line_range,
                                    partial_analysis: false,
                                });
                            }
                        }
                        Declaration::ClassDeclaration(class) => {
                            if let Some(id) = &class.id {
                                let name = id.name.to_string();
                                self.symbols.push(Symbol {
                                    id: self.symbol_id(&name),
                                    name: name.clone(),
                                    kind: "class".to_string(),
                                    line_range: line_range_of_span(self.source, class.span),
                                    children: vec![],
                                    partial_analysis: false,
                                    partial_reason: None,
                                    visibility: Some(Visibility::Public),
                                    value: None,
                                    signature: None,
                                    doc_comment: None,
                                    attributes: vec![],
                                    is_async: None,
                                    return_type: None,
                                    params: None,
                                });
                                self.exports.push(ApiExport {
                                    name,
                                    kind: "class".to_string(),
                                    generics: vec![],
                                    signature: String::new(),
                                    params: vec![],
                                    return_type: None,
                                    description: None,
                                    line_range: line_range_of_span(self.source, class.span),
                                    partial_analysis: false,
                                });
                                self.handle_class(class);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Statement::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    let (kind, sig, params, return_type, line_range) =
                        Self::extract_function_info(self.source, f);
                    let params_sym: Vec<SymbolParam> = params.iter().map(|p| SymbolParam {
                        name: p.name.clone(),
                        ty: p.ty.clone(),
                        optional: p.optional,
                        default_value: None,
                    }).collect();
                    self.symbols.push(Symbol {
                        id: self.symbol_id(&id.name),
                        name: id.name.to_string(),
                        kind,
                        line_range,
                        children: vec![],
                        partial_analysis: false,
                        partial_reason: None,
                        visibility: None,
                        value: None,
                        signature: Some(sig),
                        doc_comment: None,
                        attributes: vec![],
                        is_async: Some(f.r#async),
                        return_type,
                        params: Some(params_sym),
                    });
                }
            }
            Statement::VariableDeclaration(var) => {
                for var_decl in &var.declarations {
                    let name = bind_name(&var_decl.id);
                    if name != "_" {
                        self.symbols.push(Symbol {
                            id: self.symbol_id(&name),
                            name: name.to_string(),
                            kind: "variable".to_string(),
                            line_range: line_range_of_span(self.source, var_decl.span),
                            children: vec![],
                            partial_analysis: false,
                            partial_reason: None,
                            visibility: None,
                            value: None,
                            signature: Some(format!("const {}: unknown", name)),
                            doc_comment: None,
                            attributes: vec![],
                            is_async: None,
                            return_type: None,
                            params: None,
                        });
                    }
                }
            }
            Statement::ClassDeclaration(class) => {
                if let Some(id) = &class.id {
                    self.symbols.push(Symbol {
                        id: self.symbol_id(&id.name),
                        name: id.name.to_string(),
                        kind: "class".to_string(),
                        line_range: line_range_of_span(self.source, class.span),
                        children: vec![],
                        partial_analysis: false,
                        partial_reason: None,
                        visibility: None,
                        value: None,
                        signature: None,
                        doc_comment: None,
                        attributes: vec![],
                        is_async: None,
                        return_type: None,
                        params: None,
                    });
                    self.handle_class(class);
                }
            }
            Statement::ExportAllDeclaration(export_all) => {
                let name = format!("* from {}", export_all.source.value);
                self.exports.push(ApiExport {
                    name,
                    kind: "re-export".to_string(),
                    generics: vec![],
                    signature: String::new(),
                    params: vec![],
                    return_type: None,
                    description: None,
                    line_range: line_range_of_span(self.source, export_all.span),
                    partial_analysis: false,
                });
            }
            _ => {}
        }
    }

    fn handle_class(&mut self, class: &Class<'a>) {
        for element in &class.body.body {
            if let ClassElement::MethodDefinition(method) = element {
                let name = property_key_name(&method.key);
                let static_flag = if method.r#static { "static " } else { "" };
                let kind = match method.kind {
                    MethodDefinitionKind::Method | MethodDefinitionKind::Get | MethodDefinitionKind::Set => "method",
                    MethodDefinitionKind::Constructor => "constructor",
                };

                let sig = format!("{}{} {}()", static_flag, kind, name);
                let params: Vec<SymbolParam> = method
                    .value
                    .params
                    .items
                    .iter()
                    .map(|p| SymbolParam {
                        name: bind_name(&p.pattern),
                        ty: "any".to_string(),
                        optional: p.optional,
                        default_value: None,
                    })
                    .collect();

                self.symbols.push(Symbol {
                    id: self.symbol_id(&name),
                    name: name.clone(),
                    kind: kind.to_string(),
                    line_range: line_range_of_span(self.source, method.span),
                    children: vec![],
                    partial_analysis: false,
                    partial_reason: None,
                    visibility: Some(Visibility::Public),
                    value: None,
                    signature: Some(sig),
                    doc_comment: None,
                    attributes: vec![],
                    is_async: Some(method.value.r#async),
                    return_type: None,
                    params: Some(params),
                });
            }
        }
    }

    fn extract_function_info(
        source: &str,
        f: &Function<'a>,
    ) -> (String, String, Vec<Param>, Option<String>, [usize; 2]) {
        let kind = if f.r#async { "async_function" } else { "function" };
        let name = f.id.as_ref().map(|id| id.name.to_string()).unwrap_or_default();

        let params: Vec<Param> = f
            .params
            .items
            .iter()
            .map(|p| Param {
                name: bind_name(&p.pattern),
                ty: "any".to_string(),
                optional: p.optional,
            })
            .collect();

        let params_sig: Vec<String> = params
            .iter()
            .map(|p| format!("{}: {}", p.name, p.ty))
            .collect();
        let sig = format!(
            "{}({})",
            name,
            params_sig.join(", "),
        );

        let line_range = line_range_of_span(source, f.span);

        (kind.to_string(), sig, params, None, line_range)
    }
}

fn property_key_name(key: &PropertyKey) -> String {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.to_string(),
        PropertyKey::PrivateIdentifier(id) => id.name.to_string(),
        PropertyKey::Identifier(id) => id.name.to_string(),
        PropertyKey::StringLiteral(s) => s.value.to_string(),
        PropertyKey::NumericLiteral(n) => n.value.to_string(),
        _ => "computed".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_extract_function() {
        let source = "function add(a, b) { return a + b; }";
        let (index, _) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        assert!(!index.symbols.is_empty());
        assert_eq!(index.symbols[0].name, "add");
        assert_eq!(index.symbols[0].kind, "function");
    }

    #[test]
    fn test_extract_arrow_function() {
        let source = "const add = (a, b) => a + b;";
        let (index, _) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        let add = index.symbols.iter().find(|s| s.name == "add");
        assert!(add.is_some());
        assert_eq!(add.unwrap().kind, "variable");
    }

    #[test]
    fn test_extract_exported_function() {
        let source = "export function greet(name) { return `Hello ${name}`; }";
        let (_, contract) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        assert_eq!(contract.exports.len(), 1);
        assert_eq!(contract.exports[0].name, "greet");
    }

    #[test]
    fn test_extract_class() {
        let source = "class MyClass { constructor() {} method() {} }";
        let (index, _) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        let cls = index.symbols.iter().find(|s| s.name == "MyClass");
        assert!(cls.is_some());
        assert_eq!(cls.unwrap().kind, "class");
    }

    #[test]
    fn test_extract_export_default() {
        let source = "export default function() {}";
        let (_, contract) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        assert!(!contract.exports.is_empty());
        assert_eq!(contract.exports[0].name, "default");
    }

    #[test]
    fn test_extract_empty_source() {
        let (index, contract) = extract("test.js", "", Some(Path::new("test.js"))).unwrap();
        assert!(index.symbols.is_empty());
        assert!(contract.exports.is_empty());
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let source = "const a = 1;\nlet b = 2;\nfunction c() {}";
        let (index, _) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        assert_eq!(index.symbols.len(), 3);
    }

    #[test]
    fn test_extract_async_function() {
        let source = "async function fetchData() { return await fetch('/data'); }";
        let (index, _) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        let func = index.symbols.iter().find(|s| s.name == "fetchData").unwrap();
        assert_eq!(func.is_async, Some(true));
        assert_eq!(func.kind, "async_function");
    }

    #[test]
    fn test_extract_exported_class() {
        let source = "export class Animal { speak() {} }";
        let (_, contract) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        assert!(!contract.exports.is_empty());
        assert_eq!(contract.exports[0].kind, "class");
    }

    #[test]
    fn test_extract_const_variable() {
        let source = "const PI = 3.14159;";
        let (index, _) = extract("test.js", source, Some(Path::new("test.js"))).unwrap();
        let pi = index.symbols.iter().find(|s| s.name == "PI").unwrap();
        assert_eq!(pi.kind, "variable");
    }
}
