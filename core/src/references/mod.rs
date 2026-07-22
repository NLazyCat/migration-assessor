pub mod javascript;
pub mod rust;
pub mod typescript;

pub use typescript::PathAliasResolver;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::fs;

/// Write the reverse index as per-file JSON shards under `output_dir/references/reverse/`.
///
/// Each shard file uses symbol-only top-level keys; the reader
/// (`ProjectContext::load_reverse_index`) reconstructs the full `file:symbol` keys.
pub fn write_reverse_shards(reverse: &ReverseIndex, output_dir: &Path) -> anyhow::Result<()> {
    let reverse_dir = output_dir.join("references").join("reverse");

    // Group entries by file from the full key "file:symbol"
    let mut by_file: HashMap<String, serde_json::Map<String, serde_json::Value>> = HashMap::new();
    for (full_key, refs) in reverse {
        if let Some((file, symbol)) = full_key.rsplit_once(':') {
            let entry = by_file.entry(file.to_string()).or_default();
            entry.insert(symbol.to_string(), serde_json::to_value(refs)?);
        } else {
            // Keys without a ':' go into a "misc" shard
            let entry = by_file.entry("_misc".to_string()).or_default();
            entry.insert(full_key.clone(), serde_json::to_value(refs)?);
        }
    }

    // Write per-file shards
    for (file, entries) in &by_file {
        let shard_path = reverse_dir.join(format!("{}.json", file));
        if let Some(parent) = shard_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let value = serde_json::Value::Object(entries.clone());
        fs::write(&shard_path, serde_json::to_string_pretty(&value)?)?;
    }

    Ok(())
}

/// A single cross-file symbol reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolReference {
    pub symbol: String,
    pub location: Location,
    pub kind: ReferenceKind,
}

/// Source location of a reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Location {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

/// The nature of a reference between two symbols.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceKind {
    /// Function/method call: `foo()`
    Call,
    /// Class instantiation: `new Foo()`
    Instantiation,
    /// Class inheritance: `class A extends B`
    Extends,
    /// Interface/type extension: `interface A extends B`
    ExtendsType,
    /// Interface implementation: `class A implements B`
    Implements,
    /// Type reference in annotations: `const x: SomeType`
    TypeReference,
    /// Property/method access on an import: `obj.method()`
    PropertyAccess,
    /// Generic reference use (fallback)
    Usage,
}

/// Forward reference index: for each source symbol, list all symbols it references.
pub type ForwardIndex = HashMap<String, Vec<SymbolReference>>;

/// Reverse reference index: for each target symbol, list all symbols that reference it.
pub type ReverseIndex = HashMap<String, Vec<SymbolReference>>;

/// Per-file import bindings: local_name -> (target_file, exported_name).
pub type FileBindings = HashMap<String, (String, String)>;

/// An import binding within a single file: `import { exported_name as local_name } from "source_module"`.
#[derive(Debug, Clone)]
pub struct ImportBinding {
    pub local_name: String,
    pub source_module: String,
    pub exported_name: String,
}

/// Parse import bindings from a TypeScript/Rust source file.
pub fn parse_import_bindings(
    source: &str,
    file_path: Option<&Path>,
) -> anyhow::Result<Vec<ImportBinding>> {
    let ext = file_path
        .and_then(|p| p.extension())
        .and_then(|e| e.to_str())
        .unwrap_or("ts");

    match ext {
        "ts" | "tsx" | "mts" | "cts" => typescript::parse_import_bindings(source, file_path),
        "rs" => rust::parse_import_bindings(source),
        "js" | "jsx" | "mjs" | "cjs" => crate::parser::javascript::parse_import_bindings(source, file_path),
        _ => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_ts_imports() {
        let source = r#"import { foo } from "./bar";"#;
        let result = parse_import_bindings(source, Some(Path::new("test.ts"))).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].local_name, "foo");
        assert_eq!(result[0].source_module, "./bar");
    }

    #[test]
    fn test_parse_tsx_imports() {
        let source = r#"import React from "./Component";"#;
        let result = parse_import_bindings(source, Some(Path::new("test.tsx"))).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].local_name, "React");
    }

    #[test]
    fn test_parse_rust_imports() {
        let source = "use crate::models::User;";
        let result = parse_import_bindings(source, Some(Path::new("lib.rs"))).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].local_name, "User");
    }

    #[test]
    fn test_parse_unknown_extension_returns_empty() {
        let source = "some random content";
        let result = parse_import_bindings(source, Some(Path::new("foo.txt"))).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_default_extension_ts() {
        let source = r#"import { a } from "./b";"#;
        let result = parse_import_bindings(source, None).unwrap();
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_import_binding_struct() {
        let binding = ImportBinding {
            local_name: "myLocal".into(),
            source_module: "./myModule".into(),
            exported_name: "myExport".into(),
        };
        assert_eq!(binding.local_name, "myLocal");
        assert_eq!(binding.source_module, "./myModule");
        assert_eq!(binding.exported_name, "myExport");
    }

    #[test]
    fn test_symbol_reference_struct() {
        let sr = SymbolReference {
            symbol: "file.ts:foo".into(),
            location: Location {
                file: "test.ts".into(),
                line: 10,
                column: 5,
            },
            kind: ReferenceKind::Call,
        };
        assert_eq!(sr.symbol, "file.ts:foo");
        assert_eq!(sr.location.line, 10);
    }

    #[test]
    fn test_reference_kind_variants() {
        assert_ne!(ReferenceKind::Call, ReferenceKind::Usage);
        assert_ne!(ReferenceKind::Instantiation, ReferenceKind::TypeReference);
        assert_eq!(ReferenceKind::Extends, ReferenceKind::Extends);
        assert_eq!(ReferenceKind::Implements, ReferenceKind::Implements);
        assert_eq!(ReferenceKind::PropertyAccess, ReferenceKind::PropertyAccess);
    }

    #[test]
    fn test_location_struct() {
        let loc = Location {
            file: "src/main.rs".into(),
            line: 42,
            column: 7,
        };
        assert_eq!(loc.file, "src/main.rs");
        assert_eq!(loc.line, 42);
        assert_eq!(loc.column, 7);
    }
}
