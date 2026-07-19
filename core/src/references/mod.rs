pub mod rust;
pub mod typescript;

pub use typescript::PathAliasResolver;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

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
        _ => Ok(Vec::new()),
    }
}
