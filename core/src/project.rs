use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLanguage {
    TypeScript,
    Rust,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub source_language: SourceLanguage,
    pub target_language: String,
}

impl Project {
    pub fn detect(
        root: &Path,
        target_language: String,
        source_lang_hint: Option<String>,
    ) -> anyhow::Result<Self> {
        let has_package_json = root.join("package.json").exists();
        let has_cargo_toml = root.join("Cargo.toml").exists();

        let source_language = match source_lang_hint.as_deref() {
            Some("typescript") | Some("ts") => SourceLanguage::TypeScript,
            Some("rust") | Some("rs") => SourceLanguage::Rust,
            _ => match (has_package_json, has_cargo_toml) {
                (true, false) => SourceLanguage::TypeScript,
                (false, true) => SourceLanguage::Rust,
                (true, true) => anyhow::bail!(
                    "Both package.json and Cargo.toml found; please specify --source-lang"
                ),
                (false, false) => {
                    anyhow::bail!("No package.json or Cargo.toml found in {}", root.display())
                }
            },
        };

        Ok(Project {
            root: root.to_path_buf(),
            source_language,
            target_language,
        })
    }

    pub fn source_language_str(&self) -> &'static str {
        match self.source_language {
            SourceLanguage::TypeScript => "typescript",
            SourceLanguage::Rust => "rust",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn ts_project() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        fs::write(path.join("package.json"), "{}").unwrap();
        (dir, path)
    }

    fn rust_project() -> (TempDir, PathBuf) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();
        fs::write(path.join("Cargo.toml"), "[package]\nname = \"test\"\n").unwrap();
        (dir, path)
    }

    #[test]
    fn test_detect_typescript_from_package_json() {
        let (_d, root) = ts_project();
        let proj = Project::detect(&root, "rust".into(), None).unwrap();
        assert_eq!(proj.source_language, SourceLanguage::TypeScript);
        assert_eq!(proj.target_language, "rust");
    }

    #[test]
    fn test_detect_rust_from_cargo_toml() {
        let (_d, root) = rust_project();
        let proj = Project::detect(&root, "typescript".into(), None).unwrap();
        assert_eq!(proj.source_language, SourceLanguage::Rust);
    }

    #[test]
    fn test_detect_hint_typescript() {
        let (_d, root) = rust_project();
        let proj = Project::detect(&root, "rust".into(), Some("typescript".into())).unwrap();
        assert_eq!(proj.source_language, SourceLanguage::TypeScript);
    }

    #[test]
    fn test_detect_hint_rust() {
        let (_d, root) = ts_project();
        let proj = Project::detect(&root, "ts".into(), Some("rust".into())).unwrap();
        assert_eq!(proj.source_language, SourceLanguage::Rust);
    }

    #[test]
    fn test_detect_both_configs() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        let err = Project::detect(dir.path(), "rust".into(), None).unwrap_err();
        assert!(err.to_string().contains("Both package.json"));
    }

    #[test]
    fn test_detect_neither_config() {
        let dir = TempDir::new().unwrap();
        let err = Project::detect(dir.path(), "rust".into(), None).unwrap_err();
        assert!(err.to_string().contains("No package.json"));
    }

    #[test]
    fn test_source_language_str() {
        let (_d, root) = ts_project();
        let proj = Project::detect(&root, "rust".into(), None).unwrap();
        assert_eq!(proj.source_language_str(), "typescript");

        let (_d2, root2) = rust_project();
        let proj2 = Project::detect(&root2, "ts".into(), None).unwrap();
        assert_eq!(proj2.source_language_str(), "rust");
    }

    #[test]
    fn test_detect_hint_short_ts() {
        let (_d, root) = rust_project();
        let proj = Project::detect(&root, "rust".into(), Some("ts".into())).unwrap();
        assert_eq!(proj.source_language, SourceLanguage::TypeScript);
    }

    #[test]
    fn test_detect_hint_short_rs() {
        let (_d, root) = ts_project();
        let proj = Project::detect(&root, "ts".into(), Some("rs".into())).unwrap();
        assert_eq!(proj.source_language, SourceLanguage::Rust);
    }
}
