pub mod rust;
pub mod typescript;

use crate::project::SourceLanguage;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolIndex {
    pub module: String,
    pub symbols: Vec<Symbol>,
}

impl SymbolIndex {
    pub fn all_symbols(&self) -> Vec<&Symbol> {
        let mut result = Vec::new();
        for sym in &self.symbols {
            result.push(sym);
            result.extend(sym.all_symbols());
        }
        result
    }
}

impl Symbol {
    pub fn all_symbols(&self) -> Vec<&Symbol> {
        let mut result = Vec::new();
        for child in &self.children {
            result.push(child);
            result.extend(child.all_symbols());
        }
        result
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Visibility {
    Public,
    Private,
    Protected,
    Default,
    Crate,
    Super,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolParam {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub optional: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub line_range: [usize; 2],
    pub children: Vec<Symbol>,
    pub partial_analysis: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visibility: Option<Visibility>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc_comment: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub attributes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_async: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<SymbolParam>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiContract {
    pub module: String,
    pub exports: Vec<ApiExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiExport {
    pub name: String,
    pub kind: String,
    pub generics: Vec<String>,
    pub signature: String,
    pub params: Vec<Param>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub line_range: [usize; 2],
    pub partial_analysis: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub optional: bool,
}

pub struct SymbolExtractor;

impl SymbolExtractor {
    pub fn extract_all(
        root: &Path,
        files: &[PathBuf],
        source_language: SourceLanguage,
    ) -> anyhow::Result<Vec<(SymbolIndex, ApiContract)>> {
        let results: Vec<(SymbolIndex, ApiContract)> = files
            .par_iter()
            .filter_map(|file| {
                let source = match fs::read_to_string(file) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Warning: failed to read {}: {}", file.display(), e);
                        return None;
                    }
                };

                let relative = file.strip_prefix(root).unwrap_or(file);
                let module = relative.to_string_lossy().replace('\\', "/");

                match source_language {
                    SourceLanguage::Rust => match rust::extract(&module, &source) {
                        Ok(r) => Some(r),
                        Err(e) => {
                            eprintln!(
                                "Warning: failed to extract symbols from {}: {}",
                                file.display(),
                                e
                            );
                            None
                        }
                    },
                    SourceLanguage::TypeScript => {
                        match typescript::extract(&module, &source, Some(file)) {
                            Ok(r) => Some(r),
                            Err(e) => {
                                eprintln!(
                                    "Warning: failed to extract symbols from {}: {}",
                                    file.display(),
                                    e
                                );
                                None
                            }
                        }
                    }
                }
            })
            .collect();

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::SourceLanguage;

    fn make_symbol(name: &str, children: Vec<Symbol>) -> Symbol {
        Symbol {
            id: format!("mod:{}", name),
            name: name.to_string(),
            kind: "function".into(),
            line_range: [1, 5],
            children,
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
        }
    }

    #[test]
    fn test_symbol_index_all_symbols_no_children() {
        let si = SymbolIndex {
            module: "test".into(),
            symbols: vec![make_symbol("foo", vec![]), make_symbol("bar", vec![])],
        };
        let all = si.all_symbols();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].name, "foo");
        assert_eq!(all[1].name, "bar");
    }

    #[test]
    fn test_symbol_index_all_symbols_with_children() {
        let child = make_symbol("inner", vec![]);
        let parent = make_symbol("outer", vec![child]);
        let si = SymbolIndex {
            module: "test".into(),
            symbols: vec![parent],
        };
        let all = si.all_symbols();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_symbol_all_symbols_nested() {
        let grandchild = make_symbol("gc", vec![]);
        let child = make_symbol("child", vec![grandchild]);
        let parent = make_symbol("parent", vec![child]);
        let results = parent.all_symbols();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].name, "child");
        assert_eq!(results[1].name, "gc");
    }

    #[test]
    fn test_symbol_all_symbols_no_children() {
        let sym = make_symbol("solo", vec![]);
        let results = sym.all_symbols();
        assert!(results.is_empty());
    }

    #[test]
    fn test_extract_all_empty_files() {
        let dir = tempfile::TempDir::new().unwrap();
        let result = SymbolExtractor::extract_all(dir.path(), &[], SourceLanguage::TypeScript).unwrap();
        assert!(result.is_empty());

        let result = SymbolExtractor::extract_all(dir.path(), &[], SourceLanguage::Rust).unwrap();
        assert!(result.is_empty());
    }
}
