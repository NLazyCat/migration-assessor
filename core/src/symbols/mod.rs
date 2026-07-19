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
