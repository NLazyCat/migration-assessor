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
