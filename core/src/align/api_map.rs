use std::collections::HashMap;

pub struct ApiMapRegistry {
    mapping: HashMap<String, String>,
}

impl ApiMapRegistry {
    pub fn new(source_lang: &str, target_lang: &str) -> Self {
        let dir_key = format!("{}_to_{}", source_lang, target_lang);
        let data = match dir_key.as_str() {
            "typescript_to_rust" | "javascript_to_rust" => {
                include_str!(concat!(env!("OUT_DIR"), "/ts_to_rust.toml"))
            }
            "rust_to_typescript" | "rust_to_javascript" => {
                include_str!(concat!(env!("OUT_DIR"), "/rust_to_ts.toml"))
            }
            _ => return Self { mapping: HashMap::new() },
        };

        let table: toml::Table = match data.parse() {
            Ok(t) => t,
            Err(_) => return Self { mapping: HashMap::new() },
        };

        let mut mapping = HashMap::new();
        if let Some(am) = table.get("api_mapping").and_then(|v| v.as_table()) {
            for (from, to) in am {
                if let Some(to_str) = to.as_str() {
                    mapping.insert(from.clone(), to_str.to_string());
                }
            }
        }

        Self { mapping }
    }

    pub fn translate_call(&self, call: &str) -> Option<&str> {
        self.mapping.get(call).map(|s| s.as_str())
    }

    pub fn translate_call_chain(&self, calls: &[String]) -> Vec<String> {
        calls
            .iter()
            .map(|c| self.translate_call(c).unwrap_or(c).to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_db_call() {
        let reg = ApiMapRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_call("db.findOne"), Some("db.find_one"));
    }

    #[test]
    fn test_translate_fetch() {
        let reg = ApiMapRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_call("fetch"), Some("reqwest::get"));
    }

    #[test]
    fn test_translate_unknown_call() {
        let reg = ApiMapRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_call("foo.bar"), None);
    }

    #[test]
    fn test_translate_call_chain() {
        let reg = ApiMapRegistry::new("typescript", "rust");
        let calls = vec!["db.findOne".to_string(), "db.save".to_string()];
        let result = reg.translate_call_chain(&calls);
        assert_eq!(result[0], "db.find_one");
        assert_eq!(result[1], "db.insert");
    }

    #[test]
    fn test_default_registry() {
        let reg = ApiMapRegistry::new("unknown", "unknown");
        assert!(reg.translate_call("db.findOne").is_none());
    }
}
