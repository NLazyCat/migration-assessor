use super::naming::NamingRegistry;

/// Compare parameter counts and types between source and target symbol.
/// Returns a similarity score 0.0–1.0.
pub fn compare_signatures(
    source_name: &str,
    source_params: &[(String, String)],
    source_return: Option<&str>,
    target_name: &str,
    target_params: &[(String, String)],
    target_return: Option<&str>,
    naming: &NamingRegistry,
) -> f64 {
    let mut score = 0.0;

    // 1. Name match after translation (0–0.4)
    let translated = naming.translate_name(source_name);
    if translated == target_name || source_name == target_name {
        score += 0.4;
    }

    // 2. Parameter count compatibility (0–0.3)
    let param_diff = (source_params.len() as i32 - target_params.len() as i32).abs();
    score += if param_diff == 0 {
        0.3
    } else if param_diff <= 2 {
        0.15
    } else {
        0.0
    };

    // 3. Parameter type compatibility (0–0.2)
    if !source_params.is_empty() && !target_params.is_empty() {
        let mut matched = 0usize;
        for (_, src_ty) in source_params {
            let translated_ty = naming.translate_type(src_ty);
            for (_, tgt_ty) in target_params {
                if translated_ty == *tgt_ty || *src_ty == *tgt_ty {
                    matched += 1;
                    break;
                }
            }
        }
        let max = source_params.len().max(target_params.len());
        score += (matched as f64 / max as f64) * 0.2;
    }

    // 4. Return type compatibility (0–0.1)
    match (source_return, target_return) {
        (Some(s), Some(t)) => {
            let translated_ret = naming.translate_type(s);
            if translated_ret == t || s == t {
                score += 0.1;
            }
        }
        (None, None) => score += 0.05,
        _ => {}
    }

    score.min(1.0)
}

/// Check if the source symbol's children (fields/members) align with target's children.
/// Returns 0.0–1.0 based on field name match rate after translation.
pub fn compare_children(
    source_children: &[(&str, Option<&str>)],
    target_children: &[(&str, Option<&str>)],
    naming: &NamingRegistry,
) -> f64 {
    if source_children.is_empty() || target_children.is_empty() {
        return 0.0;
    }

    let translated_source: Vec<String> = source_children
        .iter()
        .map(|(name, _)| naming.translate_name(name))
        .collect();
    let target_names: Vec<&str> = target_children.iter().map(|(n, _)| *n).collect();

    let matched = translated_source
        .iter()
        .filter(|sn| target_names.iter().any(|tn| tn == sn))
        .count();

    matched as f64 / source_children.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::align::naming::NamingRegistry;

    fn make_naming() -> NamingRegistry {
        NamingRegistry::new("typescript", "rust")
    }

    #[test]
    fn test_compare_signatures_exact_match() {
        let naming = make_naming();
        let score = compare_signatures(
            "login",
            &[("email".into(), "string".into()), ("pwd".into(), "string".into())],
            Some("void"),
            "login",
            &[("email".into(), "String".into()), ("pwd".into(), "String".into())],
            Some("()"),
            &naming,
        );
        assert!((score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_compare_signatures_different_names() {
        let naming = make_naming();
        let score = compare_signatures(
            "login",
            &[],
            None,
            "logout",
            &[],
            None,
            &naming,
        );
        assert!(score < 0.5);
    }

    #[test]
    fn test_compare_children_exact() {
        let naming = make_naming();
        let score = compare_children(
            &[("displayName", Some("string")), ("email", Some("string"))],
            &[("display_name", Some("String")), ("email", Some("String"))],
            &naming,
        );
        assert!((score - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_compare_children_partial() {
        let naming = make_naming();
        let score = compare_children(
            &[("displayName", Some("string")), ("email", Some("string")), ("role", Some("string"))],
            &[("display_name", Some("String")), ("email", Some("String"))],
            &naming,
        );
        let expected = 2.0 / 3.0;
        assert!((score - expected).abs() < 0.01);
    }
}
