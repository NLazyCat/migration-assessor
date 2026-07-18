use super::ModuleReferences;
use std::path::Path;

use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::SourceType;

pub fn parse_references(
    source: &str,
    file_path: Option<&Path>,
) -> anyhow::Result<ModuleReferences> {
    let source_type = detect_source_type(file_path);

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

fn detect_source_type(file_path: Option<&Path>) -> SourceType {
    match file_path.and_then(|p| p.extension().and_then(|e| e.to_str())) {
        Some("tsx") => SourceType::tsx(),
        Some("ts") => SourceType::ts(),
        Some("mts") | Some("cts") => SourceType::default()
            .with_typescript(true)
            .with_module(true),
        _ => SourceType::ts(),
    }
}
