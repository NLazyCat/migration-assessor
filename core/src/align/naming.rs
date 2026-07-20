#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaseTransform {
    None,
    CamelToSnake,
    SnakeToCamel,
}

pub struct NamingRegistry {
    pub strip_prefixes: Vec<String>,
    pub case: CaseTransform,
    pub type_map: Vec<(String, String)>,
}

impl NamingRegistry {
    pub fn new(source_lang: &str, target_lang: &str) -> Self {
        let dir_key = format!("{}_to_{}", source_lang, target_lang);
        let data = match dir_key.as_str() {
            "typescript_to_rust" | "javascript_to_rust" => {
                include_str!(concat!(env!("OUT_DIR"), "/ts_to_rust.toml"))
            }
            "rust_to_typescript" | "rust_to_javascript" => {
                include_str!(concat!(env!("OUT_DIR"), "/rust_to_ts.toml"))
            }
            _ => return Self::default(),
        };

        let table: toml::Table = match data.parse() {
            Ok(t) => t,
            Err(_) => return Self::default(),
        };

        let mut strip_prefixes = Vec::new();
        let mut case = CaseTransform::None;

        if let Some(convention) = table.get("convention").and_then(|v| v.as_table()) {
            if let Some(prefixes) = convention
                .get("strip_prefixes")
                .and_then(|v| v.as_array())
            {
                for p in prefixes {
                    if let Some(s) = p.as_str() {
                        strip_prefixes.push(s.to_string());
                    }
                }
            }
            if let Some(case_str) = convention.get("case").and_then(|v| v.as_str()) {
                case = match case_str {
                    "CamelToSnake" => CaseTransform::CamelToSnake,
                    "SnakeToCamel" => CaseTransform::SnakeToCamel,
                    _ => CaseTransform::None,
                };
            }
        }

        let mut type_map = Vec::new();
        if let Some(tm) = table.get("type_mapping").and_then(|v| v.as_table()) {
            for (from, to) in tm {
                if let Some(to_str) = to.as_str() {
                    type_map.push((from.clone(), to_str.to_string()));
                }
            }
        }

        Self {
            strip_prefixes,
            case,
            type_map,
        }
    }

    pub fn translate_name(&self, name: &str) -> String {
        let stripped = self.strip_prefix(name);
        self.apply_case(&stripped)
    }

    pub fn candidates(&self, name: &str) -> Vec<String> {
        let mut result = Vec::new();
        // Original
        result.push(name.to_string());
        // Stripped (if different)
        let stripped = self.strip_prefix(name);
        if stripped != name {
            result.push(stripped.clone());
        }
        // Transformed case
        let transformed = self.apply_case(name);
        if transformed != name && !result.contains(&transformed) {
            result.push(transformed.clone());
        }
        // Stripped + transformed
        let stripped_transformed = self.apply_case(&stripped);
        if stripped_transformed != transformed && !result.contains(&stripped_transformed) {
            result.push(stripped_transformed);
        }
        result
    }

    pub fn translate_type(&self, ty: &str) -> String {
        for (from, to) in &self.type_map {
            if let Some(inner) = Self::match_template(from, ty) {
                if inner.is_empty() {
                    return to.clone();
                }
                // Recursively translate inner type
                let translated_inner = self.translate_type(&inner);
                return to.replace("{T}", &translated_inner)
                    .replace("{K}", &translated_inner)
                    .replace("{V}", &translated_inner);
            }
        }
        ty.to_string()
    }

    fn strip_prefix(&self, name: &str) -> String {
        for prefix in &self.strip_prefixes {
            if let Some(rest) = name.strip_prefix(prefix) {
                if !rest.is_empty() && rest.chars().next().map_or(false, |c| c.is_uppercase()) {
                    return rest.to_string();
                }
            }
        }
        name.to_string()
    }

    fn apply_case(&self, name: &str) -> String {
        match self.case {
            CaseTransform::CamelToSnake => {
                let mut result = String::new();
                for (i, c) in name.chars().enumerate() {
                    if c.is_uppercase() {
                        if i > 0 {
                            result.push('_');
                        }
                        result.push(c.to_ascii_lowercase());
                    } else {
                        result.push(c);
                    }
                }
                result
            }
            CaseTransform::SnakeToCamel => {
                let mut result = String::new();
                let mut upper_next = false;
                for c in name.chars() {
                    if c == '_' {
                        upper_next = true;
                    } else if upper_next {
                        result.push(c.to_ascii_uppercase());
                        upper_next = false;
                    } else {
                        result.push(c);
                    }
                }
                result
            }
            CaseTransform::None => name.to_string(),
        }
    }

    /// Match a template pattern like "Promise<{T}>" against "Promise<User>"
    /// Returns the captured inner type(s), or None if no match.
    fn match_template(pattern: &str, ty: &str) -> Option<String> {
        if !pattern.contains("{T}") && !pattern.contains("{K}") && !pattern.contains("{V}") {
            return if pattern == ty { Some(String::new()) } else { None };
        }

        // Find the template parameter marker
        let template_start = pattern.find('{')?;
        let template_end = pattern.rfind('}')? + 1;

        let prefix = &pattern[..template_start];
        let suffix = &pattern[template_end..];

        if !ty.starts_with(prefix) || !ty.ends_with(suffix) {
            return None;
        }

        let inner = &ty[prefix.len()..ty.len() - suffix.len()];
        if inner.is_empty() || inner.contains('<') || inner.contains('>') {
            return None;
        }

        Some(inner.to_string())
    }

    fn default() -> Self {
        Self {
            strip_prefixes: vec![],
            case: CaseTransform::None,
            type_map: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_name_strips_i_prefix() {
        let reg = NamingRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_name("IUser"), "user");
    }

    #[test]
    fn test_translate_name_camel_to_snake() {
        let reg = NamingRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_name("displayName"), "display_name");
    }

    #[test]
    fn test_translate_name_no_change() {
        let reg = NamingRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_name("login"), "login");
    }

    #[test]
    fn test_candidates() {
        let reg = NamingRegistry::new("typescript", "rust");
        let candidates = reg.candidates("IUser");
        assert!(candidates.contains(&"User".to_string()));
        assert!(candidates.contains(&"user".to_string()));
    }

    #[test]
    fn test_translate_type_promise() {
        let reg = NamingRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_type("Promise<User>"), "Result<User>");
    }

    #[test]
    fn test_translate_type_array() {
        let reg = NamingRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_type("Array<string>"), "Vec<String>");
    }

    #[test]
    fn test_translate_type_no_match() {
        let reg = NamingRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_type("CustomType"), "CustomType");
    }

    #[test]
    fn test_translate_type_string() {
        let reg = NamingRegistry::new("typescript", "rust");
        assert_eq!(reg.translate_type("string"), "String");
    }

    #[test]
    fn test_default_registry() {
        let reg = NamingRegistry::new("unknown", "unknown");
        assert_eq!(reg.translate_name("IUser"), "IUser");
        assert_eq!(reg.translate_type("string"), "string");
    }

    #[test]
    fn test_snake_to_camel() {
        let reg = NamingRegistry::new("rust", "typescript");
        assert_eq!(reg.translate_name("display_name"), "displayName");
    }
}
