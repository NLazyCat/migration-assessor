use super::ModuleReferences;
use crate::util;
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::{ParseOptions, Parser};

pub fn parse_references(
    source: &str,
    file_path: Option<&Path>,
) -> anyhow::Result<ModuleReferences> {
    let source_type = util::detect_source_type(file_path);

    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type)
        .with_options(ParseOptions {
            parse_regular_expression: true,
            ..ParseOptions::default()
        })
        .parse();

    let mut relative = Vec::new();
    let mut external = Vec::new();

    for stmt in &ret.program.body {
        match stmt {
            Statement::ImportDeclaration(import) => {
                let src = import.source.value.to_string();
                classify_import(&src, &mut relative, &mut external);
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(source) = &export.source {
                    let src = source.value.to_string();
                    classify_import(&src, &mut relative, &mut external);
                }
            }
            Statement::ExportAllDeclaration(export) => {
                let src = export.source.value.to_string();
                classify_import(&src, &mut relative, &mut external);
            }
            _ => {}
        }
    }

    relative.sort();
    relative.dedup();
    external.sort();
    external.dedup();

    Ok(ModuleReferences {
        relative_imports: relative,
        external_imports: external,
    })
}

fn classify_import(src: &str, relative: &mut Vec<String>, external: &mut Vec<String>) {
    if src.starts_with('.') || src.starts_with('/') {
        relative.push(src.to_string());
    } else {
        external.push(src.to_string());
    }
}

/// Parse import bindings from a JavaScript source file.
pub fn parse_import_bindings(
    source: &str,
    file_path: Option<&Path>,
) -> anyhow::Result<Vec<crate::references::ImportBinding>> {
    use crate::references::ImportBinding;
    use oxc_ast::ast::ImportDeclarationSpecifier;

    let source_type = util::detect_source_type(file_path);
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, source, source_type)
        .with_options(ParseOptions {
            parse_regular_expression: true,
            ..ParseOptions::default()
        })
        .parse();

    let mut bindings = Vec::new();

    for stmt in &ret.program.body {
        if let Statement::ImportDeclaration(import) = stmt {
            let source_module = import.source.value.to_string();

            if let Some(specifiers) = &import.specifiers {
                for specifier in specifiers {
                    let (local_name, exported_name) = match specifier {
                        ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => {
                            (s.local.name.to_string(), "default".to_string())
                        }
                        ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => {
                            (s.local.name.to_string(), "*".to_string())
                        }
                        ImportDeclarationSpecifier::ImportSpecifier(s) => {
                            let exported = match &s.imported {
                                oxc_ast::ast::ModuleExportName::IdentifierName(n) => n.name.to_string(),
                                oxc_ast::ast::ModuleExportName::IdentifierReference(n) => n.name.to_string(),
                                oxc_ast::ast::ModuleExportName::StringLiteral(n) => n.value.to_string(),
                            };
                            (s.local.name.to_string(), exported)
                        }
                    };

                    bindings.push(ImportBinding {
                        local_name,
                        source_module: source_module.clone(),
                        exported_name,
                    });
                }
            }
        }
    }

    Ok(bindings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_js_parse_references_relative() {
        let source = r#"import { foo } from './helper';"#;
        let refs = parse_references(source, Some(Path::new("test.js"))).unwrap();
        assert_eq!(refs.relative_imports, vec!["./helper"]);
        assert!(refs.external_imports.is_empty());
    }

    #[test]
    fn test_js_parse_references_external() {
        let source = r#"import fs from 'fs';"#;
        let refs = parse_references(source, Some(Path::new("test.js"))).unwrap();
        assert!(refs.relative_imports.is_empty());
        assert_eq!(refs.external_imports, vec!["fs"]);
    }

    #[test]
    fn test_js_parse_references_no_imports() {
        let source = "const x = 1;";
        let refs = parse_references(source, Some(Path::new("test.js"))).unwrap();
        assert!(refs.relative_imports.is_empty());
        assert!(refs.external_imports.is_empty());
    }

    #[test]
    fn test_js_parse_import_bindings_default() {
        let source = r#"import foo from './bar';"#;
        let bindings = parse_import_bindings(source, Some(Path::new("test.js"))).unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].local_name, "foo");
        assert_eq!(bindings[0].exported_name, "default");
        assert_eq!(bindings[0].source_module, "./bar");
    }

    #[test]
    fn test_js_parse_import_bindings_named() {
        let source = r#"import { a, b } from './mod';"#;
        let bindings = parse_import_bindings(source, Some(Path::new("test.js"))).unwrap();
        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].local_name, "a");
        assert_eq!(bindings[0].exported_name, "a");
        assert_eq!(bindings[1].local_name, "b");
    }
}
