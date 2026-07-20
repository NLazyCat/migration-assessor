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
                // Re-exports: export { x } from "y"
                if let Some(source) = &export.source {
                    let src = source.value.to_string();
                    classify_import(&src, &mut relative, &mut external);
                }
            }
            Statement::ExportAllDeclaration(export) => {
                // Re-exports: export * from "y"
                let src = export.source.value.to_string();
                classify_import(&src, &mut relative, &mut external);
            }
            // Dynamic import() is not captured here - we don't walk expressions
            _ => {}
        }
    }

    // Deduplicate
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_ts_parse_references() {
        let source = r#"
import { Component } from 'react';
import { helper } from './utils';
export type { Props } from './types';
export * from 'lodash';
"#;
        let refs = parse_references(source, Some(Path::new("test.tsx"))).unwrap();
        assert!(refs.external_imports.contains(&"react".to_string()));
        assert!(refs.external_imports.contains(&"lodash".to_string()));
        assert!(refs.relative_imports.contains(&"./utils".to_string()));
        assert!(refs.relative_imports.contains(&"./types".to_string()));
    }

    #[test]
    fn test_ts_parse_references_no_imports() {
        let source = "const x: number = 1;\nconsole.log(x);";
        let refs = parse_references(source, Some(Path::new("test.ts"))).unwrap();
        assert!(refs.relative_imports.is_empty());
        assert!(refs.external_imports.is_empty());
    }

    #[test]
    fn test_classify_import_relative() {
        let mut rel = Vec::new();
        let mut ext = Vec::new();
        classify_import("./foo", &mut rel, &mut ext);
        assert_eq!(rel, vec!["./foo"]);
        assert!(ext.is_empty());
    }

    #[test]
    fn test_classify_import_external() {
        let mut rel = Vec::new();
        let mut ext = Vec::new();
        classify_import("lodash", &mut rel, &mut ext);
        assert!(rel.is_empty());
        assert_eq!(ext, vec!["lodash"]);
    }
}

