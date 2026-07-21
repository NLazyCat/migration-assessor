use crate::ast::AstOutput;
use crate::symbols::Visibility;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Per-file migration specification — the single contract an AI reads
/// to migrate one source file to the target language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationSpec {
    // ── File identity ─────────────────────────────────────────
    /// Relative source path (e.g. "src/utils/format.ts")
    pub file: String,
    /// Target path (e.g. "src/utils/format.rs")
    pub target_path: String,
    /// Topological layer (0 = no deps, migrate first)
    pub layer: usize,
    /// Migration effort estimate
    pub migration_effort: String,
    /// Whether this module has tests
    pub has_tests: bool,

    // ── Full source code (AI needs this to understand logic) ──
    pub source: String,

    // ── Exported API surface ──────────────────────────────────
    pub exports: Vec<SpecExport>,

    // ── Symbol-level migration guidance ───────────────────────
    pub symbols: Vec<SpecSymbol>,

    // ── Import map ────────────────────────────────────────────
    pub imports: SpecImports,

    // ── Reverse dependencies (who depends on this file) ───────
    pub referenced_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecExport {
    pub name: String,
    pub kind: String,
    pub signature: String,
    pub return_type: Option<String>,
    pub line_range: [usize; 2],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecSymbol {
    pub name: String,
    pub kind: String,
    pub visibility: String,
    pub line_range: [usize; 2],
    pub signature: Option<String>,
    pub target_name: String,       // translated to target naming convention
    pub target_signature: Option<String>, // translated signature
    pub params: Vec<SpecParam>,
    pub return_type: Option<String>,
    pub is_async: Option<bool>,
    pub migration_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecParam {
    pub name: String,
    pub ty: String,
    pub optional: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecImports {
    /// Relative/local imports resolved to source file paths
    pub relative: Vec<SpecImport>,
    /// External package imports
    pub external: Vec<SpecImport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecImport {
    pub from: String,
    pub symbols: Vec<String>,
    pub target_import: Option<String>,
    pub migration_note: Option<String>,
}

/// Build a migration spec from an `AstOutput`.
///
/// `referenced_by` is the list of files that import from this module.
/// `layer` is the topological layer (computed during graph building).
/// `migration_effort` and `has_tests` come from the scoring phase.
pub fn build_spec(
    ast_output: &AstOutput,
    referenced_by: Vec<String>,
    layer: usize,
    migration_effort: &str,
    has_tests: bool,
) -> MigrationSpec {
    let target_path = derive_target_path(&ast_output.file_path);

    let exports: Vec<SpecExport> = ast_output
        .exports
        .iter()
        .map(|e| SpecExport {
            name: e.name.clone(),
            kind: e.kind.clone(),
            signature: e.signature.clone(),
            return_type: e.return_type.clone(),
            line_range: e.line_range,
        })
        .collect();

    let symbols: Vec<SpecSymbol> = ast_output
        .symbols
        .iter()
        .filter_map(|s| {
            // Only include exported symbols in the spec (private symbols are internal)
            if matches!(s.visibility, Some(Visibility::Public)) {
                Some(SpecSymbol {
                    name: s.name.clone(),
                    kind: s.kind.clone(),
                    visibility: format!("{:?}", s.visibility),
                    line_range: s.line_range,
                    signature: s.signature.clone(),
                    target_name: to_snake_case(&s.name),
                    target_signature: s.signature.clone(),
                    params: s
                        .params
                        .as_ref()
                        .map(|ps| {
                            ps.iter()
                                .map(|p| SpecParam {
                                    name: p.name.clone(),
                                    ty: p.ty.clone(),
                                    optional: p.optional,
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                    return_type: s.return_type.clone(),
                    is_async: s.is_async,
                    migration_note: None,
                })
            } else {
                None
            }
        })
        .collect();

    // Build import map from ModuleReferences
    let mut relative_imports: Vec<SpecImport> = Vec::new();
    let mut external_imports: Vec<SpecImport> = Vec::new();

    for imp in &ast_output.imports.relative_imports {
        relative_imports.push(SpecImport {
            from: imp.clone(),
            symbols: vec![],
            target_import: None,
            migration_note: Some("Resolve relative path during migration".to_string()),
        });
    }
    for imp in &ast_output.imports.external_imports {
        external_imports.push(SpecImport {
            from: imp.clone(),
            symbols: vec![],
            target_import: None,
            migration_note: Some("Check compatibility matrix for equivalent crate".to_string()),
        });
    }

    MigrationSpec {
        file: ast_output.file_path.clone(),
        target_path,
        layer,
        migration_effort: migration_effort.to_string(),
        has_tests,
        source: ast_output.source.clone(),
        exports,
        symbols,
        imports: SpecImports {
            relative: relative_imports,
            external: external_imports,
        },
        referenced_by,
    }
}

/// Derive the target file path from the source path.
/// Simple convention: replace `.ts`/`.tsx`/`.js`/`.jsx` with `.rs`.
fn derive_target_path(source_path: &str) -> String {
    let path = Path::new(source_path);
    if let Some(stem) = path.file_stem() {
        if let Some(parent) = path.parent() {
            return parent
                .join(format!("{}.rs", stem.to_string_lossy()))
                .to_string_lossy()
                .replace('\\', "/");
        }
        format!("{}.rs", stem.to_string_lossy())
    } else {
        format!("{}.rs", source_path)
    }
}

/// Convert a name to snake_case (e.g., "formatPrice" → "format_price").
///
/// Handles:
///   - Standard camelCase:     "getUserById"  → "get_user_by_id"
///   - Acronyms in camelCase:  "XMLParser"    → "xml_parser"
///   - SCREAMING_SNAKE_CASE:   "APP_NAME"     → "app_name"
///   - Leading acronym:        "DBConnection" → "db_connection"
fn to_snake_case(name: &str) -> String {
    let mut result = String::new();
    let chars: Vec<char> = name.chars().collect();
    let n = chars.len();
    let mut i = 0;

    while i < n {
        let c = chars[i];

        if c == '_' {
            result.push('_');
            i += 1;
            continue;
        }

        if c.is_uppercase() {
            // Find the full uppercase run
            let run_start = i;
            while i < n && chars[i].is_uppercase() {
                i += 1;
            }
            let run_end = i; // exclusive
            let run_len = run_end - run_start;

            // Prepend underscore unless this is the very start of the string
            if run_start > 0 && chars[run_start - 1] != '_' {
                result.push('_');
            }

            if run_len == 1 {
                // Single uppercase letter — ordinary camelCase word boundary
                result.push(c.to_ascii_lowercase());
            } else {
                // Multi-uppercase run: could be:
                //   a) Acronym before a lowercase word: "XMLParser"
                //   b) SCREAMING_SNAKE segment:         "APP_NAME"
                //   c) Acronym at end:                  "parseXML"

                let next_is_lower = run_end < n && chars[run_end].is_lowercase();

                if next_is_lower {
                    // Case (a): "XMLParser" — push all but the last uppercase as the acronym
                    for j in run_start..(run_end - 1) {
                        result.push(chars[j].to_ascii_lowercase());
                    }
                    // The last uppercase is the start of the next word
                    if !result.is_empty() && !result.ends_with('_') {
                        result.push('_');
                    }
                    result.push(chars[run_end - 1].to_ascii_lowercase());
                } else {
                    // Cases (b) and (c): full uppercase run — push all as one word
                    for j in run_start..run_end {
                        result.push(chars[j].to_ascii_lowercase());
                    }
                }
            }
        } else {
            result.push(c);
            i += 1;
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::AstOutput;
    use crate::parser::ModuleReferences;
    use crate::symbols::{ApiContract, ApiExport, Symbol, SymbolParam, Visibility};

    fn make_test_ast() -> AstOutput {
        AstOutput {
            file_path: "src/utils/format.ts".to_string(),
            source: "export function formatPrice(price: number): string {\n  return price.toFixed(2);\n}".to_string(),
            language: "typescript".to_string(),
            symbols: vec![Symbol {
                id: "src/utils/format.ts:formatPrice".to_string(),
                name: "formatPrice".to_string(),
                kind: "function".to_string(),
                line_range: [1, 3],
                children: vec![],
                partial_analysis: false,
                partial_reason: None,
                visibility: Some(Visibility::Public),
                value: None,
                signature: Some("export function formatPrice(price: number) -> string".to_string()),
                doc_comment: None,
                attributes: vec![],
                is_async: Some(false),
                return_type: Some("string".to_string()),
                params: Some(vec![SymbolParam {
                    name: "price".to_string(),
                    ty: "number".to_string(),
                    optional: false,
                    default_value: None,
                }]),
            }],
            exports: vec![ApiExport {
                name: "formatPrice".to_string(),
                kind: "function".to_string(),
                generics: vec![],
                signature: "export function formatPrice(price: number) -> string".to_string(),
                params: vec![],
                return_type: Some("string".to_string()),
                description: None,
                line_range: [1, 3],
                partial_analysis: false,
            }],
            api_contract: ApiContract {
                module: "src/utils/format.ts".to_string(),
                exports: vec![],
            },
            imports: ModuleReferences {
                relative_imports: vec!["./types".to_string()],
                external_imports: vec!["lodash".to_string()],
            },
            diagnostics: vec![],
        }
    }

    #[test]
    fn test_build_spec_basic() {
        let ast = make_test_ast();
        let spec = build_spec(&ast, vec![], 0, "trivial", false);

        assert_eq!(spec.file, "src/utils/format.ts");
        assert_eq!(spec.target_path, "src/utils/format.rs");
        assert_eq!(spec.layer, 0);
        assert_eq!(spec.migration_effort, "trivial");
        assert_eq!(spec.symbols.len(), 1);
        assert_eq!(spec.symbols[0].target_name, "format_price");
        assert_eq!(spec.imports.relative.len(), 1);
        assert_eq!(spec.imports.external.len(), 1);
    }

    #[test]
    fn test_derive_target_path() {
        assert_eq!(derive_target_path("src/utils/format.ts"), "src/utils/format.rs");
        assert_eq!(derive_target_path("src/utils/format.tsx"), "src/utils/format.rs");
        assert_eq!(derive_target_path("models/user.ts"), "models/user.rs");
        assert_eq!(derive_target_path("lib.rs"), "lib.rs");
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("formatPrice"), "format_price");
        assert_eq!(to_snake_case("getUserById"), "get_user_by_id");
        assert_eq!(to_snake_case("simple"), "simple");
        assert_eq!(to_snake_case("XMLParser"), "xml_parser");
        assert_eq!(to_snake_case("APP_NAME"), "app_name");
        assert_eq!(to_snake_case("API_PREFIX"), "api_prefix");
        assert_eq!(to_snake_case("HTTP_STATUS"), "http_status");
        assert_eq!(to_snake_case("DBConnection"), "db_connection");
        assert_eq!(to_snake_case("parseXML"), "parse_xml");
        assert_eq!(to_snake_case("parseXMLDocument"), "parse_xml_document");
    }

    #[test]
    fn test_spec_serializes_to_json() {
        let ast = make_test_ast();
        let spec = build_spec(&ast, vec!["src/services/cart.ts".to_string()], 0, "trivial", false);
        let json = serde_json::to_string_pretty(&spec).unwrap();
        assert!(json.contains("formatPrice"));
        assert!(json.contains("format_price"));
        assert!(json.contains("./types"));
        assert!(json.contains("lodash"));
        assert!(json.contains("src/services/cart.ts"));
    }
}
