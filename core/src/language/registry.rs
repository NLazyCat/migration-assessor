use std::collections::HashMap;
use std::sync::OnceLock;

use super::Language;
use super::typescript::TypeScriptLanguage;
use super::rust::RustLanguage;

pub struct LanguageRegistry {
    languages: HashMap<String, Box<dyn Language>>,
}

impl LanguageRegistry {
    pub fn get() -> &'static Self {
        static INSTANCE: OnceLock<LanguageRegistry> = OnceLock::new();
        INSTANCE.get_or_init(|| {
            let mut registry = LanguageRegistry {
                languages: HashMap::new(),
            };
            registry.register(Box::new(TypeScriptLanguage));
            registry.register(Box::new(RustLanguage));
            registry
        })
    }

    fn register(&mut self, lang: Box<dyn Language>) {
        self.languages.insert(lang.name().to_string(), lang);
    }

    pub fn get_language(&self, name: &str) -> Option<&dyn Language> {
        self.languages.get(name).map(|l| l.as_ref())
    }

    pub fn detect_language(&self, project_root: &std::path::Path) -> Option<String> {
        for (name, lang) in &self.languages {
            if lang.detect_project_type(project_root) {
                return Some(name.clone());
            }
        }
        None
    }

    pub fn list_languages(&self) -> Vec<String> {
        self.languages.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_registry_singleton() {
        let reg = LanguageRegistry::get();
        let reg2 = LanguageRegistry::get();
        assert!(std::ptr::eq(reg, reg2));
    }

    #[test]
    fn test_registered_languages() {
        let reg = LanguageRegistry::get();
        let langs = reg.list_languages();
        assert!(langs.contains(&"typescript".to_string()));
        assert!(langs.contains(&"rust".to_string()));
    }

    #[test]
    fn test_get_language() {
        let reg = LanguageRegistry::get();
        let ts = reg.get_language("typescript");
        assert!(ts.is_some());
        assert_eq!(ts.unwrap().name(), "typescript");

        let rust = reg.get_language("rust");
        assert!(rust.is_some());
        assert_eq!(rust.unwrap().name(), "rust");
    }

    #[test]
    fn test_get_language_unknown() {
        let reg = LanguageRegistry::get();
        assert!(reg.get_language("python").is_none());
    }

    #[test]
    fn test_detect_language_ts() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        let reg = LanguageRegistry::get();
        let detected = reg.detect_language(dir.path());
        assert_eq!(detected, Some("typescript".to_string()));
    }

    #[test]
    fn test_detect_language_rust() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\nname=\"test\"\n").unwrap();
        let reg = LanguageRegistry::get();
        let detected = reg.detect_language(dir.path());
        assert_eq!(detected, Some("rust".to_string()));
    }

    #[test]
    fn test_detect_language_neither() {
        let dir = tempfile::TempDir::new().unwrap();
        let reg = LanguageRegistry::get();
        let detected = reg.detect_language(dir.path());
        assert!(detected.is_none());
    }

    #[test]
    fn test_language_names() {
        let reg = LanguageRegistry::get();
        let ts = reg.get_language("typescript").unwrap();
        assert_eq!(ts.name(), "typescript");
        assert!(!ts.file_extensions().is_empty());

        let rust = reg.get_language("rust").unwrap();
        assert_eq!(rust.name(), "rust");
        assert!(!rust.file_extensions().is_empty());
    }
}
