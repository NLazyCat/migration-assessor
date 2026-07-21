pub mod typescript;
pub mod rust;

use crate::parser::ModuleReferences;
use crate::project::SourceLanguage;
use crate::symbols::{ApiContract, ApiExport, Symbol};
use std::path::Path;

/// Complete result from a single AST walk of one source file.
/// All consumers read this; no file is ever parsed more than once.
#[derive(Debug, Clone)]
pub struct AstOutput {
    /// Relative file path (e.g. "src/utils/format.ts")
    pub file_path: String,
    /// Full source code (AI needs this to understand logic)
    pub source: String,
    /// Source language
    pub language: String,
    /// Extracted symbols (functions, classes, interfaces, etc.)
    pub symbols: Vec<Symbol>,
    /// Exported API surface
    pub exports: Vec<ApiExport>,
    /// API contract for this module
    pub api_contract: ApiContract,
    /// Import statements (relative + external)
    pub imports: ModuleReferences,
    /// Parser diagnostics (warnings/errors)
    pub diagnostics: Vec<crate::language::Diagnostic>,
}

/// Parse one source file and extract everything in a single traversal.
/// Dispatches to the correct language backend internally.
pub fn parse(
    source: &str,
    file_path: &Path,
    language: SourceLanguage,
) -> anyhow::Result<AstOutput> {
    match language {
        SourceLanguage::TypeScript | SourceLanguage::JavaScript => {
            typescript::parse(source, Some(file_path))
        }
        SourceLanguage::Rust => rust::parse(source, file_path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_typescript() {
        let source = "import { helper } from './utils';\nexport function greet(name: string): string { return \"hello\"; }";
        let result = parse(source, Path::new("test.ts"), SourceLanguage::TypeScript).unwrap();
        assert_eq!(result.file_path, "test.ts");
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "greet");
        assert!(result.imports.relative_imports.contains(&"./utils".to_string()));
    }

    #[test]
    fn test_parse_rust() {
        let source = "use std::collections::HashMap;\npub fn greet(name: &str) -> String { format!(\"hello {}\", name) }";
        let result = parse(source, Path::new("lib.rs"), SourceLanguage::Rust).unwrap();
        assert_eq!(result.file_path, "lib.rs");
        assert_eq!(result.symbols.len(), 1);
        assert_eq!(result.symbols[0].name, "greet");
        assert!(result.imports.external_imports.iter().any(|i| i.contains("HashMap")));
    }

    #[test]
    fn test_parse_empty_source() {
        let result = parse("", Path::new("empty.ts"), SourceLanguage::TypeScript).unwrap();
        assert!(result.symbols.is_empty());
        assert!(result.imports.relative_imports.is_empty());
    }
}
