pub mod rust;
pub mod typescript;

use std::path::Path;

#[derive(Debug, Clone)]
pub struct ModuleReferences {
    pub relative_imports: Vec<String>,
    pub external_imports: Vec<String>,
}

pub fn parse_file_references(path: &Path, source: &str) -> anyhow::Result<ModuleReferences> {
    let ext = path.extension().and_then(|e| e.to_str());
    match ext {
        Some("ts") | Some("tsx") => typescript::parse_references(source, Some(path)),
        Some("rs") => rust::parse_references(source),
        _ => Ok(ModuleReferences {
            relative_imports: vec![],
            external_imports: vec![],
        }),
    }
}
