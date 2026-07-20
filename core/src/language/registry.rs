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
