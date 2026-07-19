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

