pub mod javascript;
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
        Some("js") | Some("jsx") | Some("mjs") | Some("cjs") => {
            javascript::parse_references(source, Some(path))
        }
        _ => Ok(ModuleReferences {
            relative_imports: vec![],
            external_imports: vec![],
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_parse_file_references_ts() {
        let source = "import { foo } from './helper';\nimport fs from 'fs';\nexport { bar } from 'lodash';";
        let refs = parse_file_references(Path::new("test.ts"), source).unwrap();
        assert!(refs.relative_imports.contains(&"./helper".to_string()));
        assert!(refs.external_imports.contains(&"fs".to_string()));
        assert!(refs.external_imports.contains(&"lodash".to_string()));
    }

    #[test]
    fn test_parse_file_references_rs() {
        let source = "use std::collections::HashMap;\nuse crate::helper::foo;\nmod bar;";
        let refs = parse_file_references(Path::new("lib.rs"), source).unwrap();
        assert!(refs.external_imports.iter().any(|i| i.contains("HashMap")));
        assert!(refs.relative_imports.iter().any(|i| i.contains("crate::helper")));
        assert!(refs.relative_imports.iter().any(|i| i == "self::bar"));
    }

    #[test]
    fn test_parse_file_references_unknown_extension() {
        let source = "anything";
        let refs = parse_file_references(Path::new("foo.py"), source).unwrap();
        assert!(refs.relative_imports.is_empty());
        assert!(refs.external_imports.is_empty());
    }

    #[test]
    fn test_parse_file_references_no_imports() {
        let source = "const x = 1;\nlet y = 2;";
        let refs = parse_file_references(Path::new("test.ts"), source).unwrap();
        assert!(refs.relative_imports.is_empty());
        assert!(refs.external_imports.is_empty());
    }
}
