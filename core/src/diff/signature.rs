use crate::diff::{ChangeDetail, SymbolChange};
use crate::symbols::Symbol;

pub fn diff(old: &Symbol, new: &Symbol) -> Option<Vec<SymbolChange>> {
    let mut changes = Vec::new();

    if let (Some(old_params), Some(new_params)) = (&old.params, &new.params) {
        let old_names: std::collections::HashSet<String> = old_params.iter().map(|p| p.name.clone()).collect();
        let new_names: std::collections::HashSet<String> = new_params.iter().map(|p| p.name.clone()).collect();

        // Build a map: position -> (old_name, new_name) for same-position renames
        // where the type is unchanged (pure renames are compatible)
        let zipped: Vec<(Option<&str>, Option<&str>)> = (0..old_params.len().max(new_params.len()))
            .map(|i| {
                let old_n = old_params.get(i).map(|p| p.name.as_str());
                let new_n = new_params.get(i).map(|p| p.name.as_str());
                (old_n, new_n)
            })
            .collect();
        let mut pure_renames: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (i, (old_n, new_n)) in zipped.iter().enumerate() {
            if let (Some(on), Some(nn)) = (old_n, new_n) {
                if on != nn
                    && old_params.get(i).map(|p| &p.ty) == new_params.get(i).map(|p| &p.ty)
                {
                    pure_renames.insert(on.to_string());
                    // Emit a single compatible rename entry instead of removed+added
                    let sc = SymbolChange::new(
                        new.name.clone(),
                        new.kind.clone(),
                        "modified".to_string(),
                        "compatible".to_string(),
                        Some(old.line_range),
                        Some(new.line_range),
                        vec![ChangeDetail {
                            aspect: "signature".to_string(),
                            change_type: "renamed".to_string(),
                            description: format!("parameter '{}' renamed to '{}'", on, nn),
                            old_value: Some(on.to_string()),
                            new_value: Some(nn.to_string()),
                            migration_note: None,
                        }],
                    );
                    changes.push(sc);
                }
            }
        }

        for name in &old_names - &new_names {
            // Skip pure renames already handled above
            if pure_renames.contains(&name) {
                continue;
            }
            let sc = SymbolChange::new(
                new.name.clone(),
                new.kind.clone(),
                "modified".to_string(),
                "compatible".to_string(),
                Some(old.line_range),
                Some(new.line_range),
                vec![ChangeDetail {
                    aspect: "signature".to_string(),
                    change_type: "removed".to_string(),
                    description: format!("parameter '{}' removed", name),
                    old_value: Some(name.clone()),
                    new_value: None,
                    migration_note: None,
                }],
            );
            changes.push(sc);
        }

        for name in &new_names - &old_names {
            if pure_renames.contains(&name) {
                continue;
            }
            let sc = SymbolChange::new(
                new.name.clone(),
                new.kind.clone(),
                "modified".to_string(),
                "compatible".to_string(),
                Some(old.line_range),
                Some(new.line_range),
                vec![ChangeDetail {
                    aspect: "signature".to_string(),
                    change_type: "added".to_string(),
                    description: format!("parameter '{}' added", name),
                    old_value: None,
                    new_value: Some(name.clone()),
                    migration_note: None,
                }],
            );
            changes.push(sc);
        }

        for (old_p, new_p) in old_params.iter().zip(new_params.iter()) {
            if old_p.ty != new_p.ty {
                let sc = SymbolChange::new(
                    new.name.clone(),
                    new.kind.clone(),
                    "modified".to_string(),
                    "breaking".to_string(),
                    Some(old.line_range),
                    Some(new.line_range),
                    vec![ChangeDetail {
                        aspect: "signature".to_string(),
                        change_type: "changed".to_string(),
                        description: format!("parameter '{}' type changed from '{}' to '{}'", old_p.name, old_p.ty, new_p.ty),
                        old_value: Some(old_p.ty.clone()),
                        new_value: Some(new_p.ty.clone()),
                        migration_note: None,
                    }],
                );
                changes.push(sc);
            }

            if old_p.optional != new_p.optional {
                let sc = SymbolChange::new(
                    new.name.clone(),
                    new.kind.clone(),
                    "modified".to_string(),
                    "compatible".to_string(),
                    Some(old.line_range),
                    Some(new.line_range),
                    vec![ChangeDetail {
                        aspect: "signature".to_string(),
                        change_type: "changed".to_string(),
                        description: format!("parameter '{}' became {}", old_p.name, if new_p.optional { "optional" } else { "required" }),
                        old_value: Some(format!("{}optional", if old_p.optional { "" } else { "not " })),
                        new_value: Some(format!("{}optional", if new_p.optional { "" } else { "not " })),
                        migration_note: None,
                    }],
                );
                changes.push(sc);
            }
        }
    }

    if old.return_type != new.return_type {
        let sc = SymbolChange::new(
            new.name.clone(),
            new.kind.clone(),
            "modified".to_string(),
            "breaking".to_string(),
            Some(old.line_range),
            Some(new.line_range),
            vec![ChangeDetail {
                aspect: "signature".to_string(),
                change_type: "changed".to_string(),
                description: format!("return type changed from '{}' to '{}'",
                    old.return_type.as_deref().unwrap_or("void"),
                    new.return_type.as_deref().unwrap_or("void")),
                old_value: old.return_type.clone(),
                new_value: new.return_type.clone(),
                migration_note: None,
            }],
        );
        changes.push(sc);
    }

    if old.is_async != new.is_async {
        let sc = SymbolChange::new(
            new.name.clone(),
            new.kind.clone(),
            "modified".to_string(),
            "breaking".to_string(),
            Some(old.line_range),
            Some(new.line_range),
            vec![ChangeDetail {
                aspect: "signature".to_string(),
                change_type: "changed".to_string(),
                description: format!("function {} is_async", if new.is_async.unwrap_or(false) { "became" } else { "is no longer" }),
                old_value: Some(format!("{}is_async", if old.is_async.unwrap_or(false) { "" } else { "not " })),
                new_value: Some(format!("{}is_async", if new.is_async.unwrap_or(false) { "" } else { "not " })),
                migration_note: None,
            }],
        );
        changes.push(sc);
    }

    // Detect children added/removed within a parent symbol
    let old_child_names: std::collections::HashSet<(String, String)> = old.children.iter()
        .map(|c| (c.kind.clone(), c.name.clone()))
        .collect();
    let new_child_names: std::collections::HashSet<(String, String)> = new.children.iter()
        .map(|c| (c.kind.clone(), c.name.clone()))
        .collect();

    if !old_child_names.is_empty() || !new_child_names.is_empty() {
        // Children added (new but not old)
        for (kind, name) in &new_child_names - &old_child_names {
            changes.push(SymbolChange::new(
                new.name.clone(),
                new.kind.clone(),
                "modified".to_string(),
                "compatible".to_string(),
                Some(old.line_range),
                Some(new.line_range),
                vec![ChangeDetail {
                    aspect: kind.clone(),
                    change_type: "added".to_string(),
                    description: format!("{} '{}' added", kind, name),
                    old_value: None,
                    new_value: Some(name.clone()),
                    migration_note: None,
                }],
            ));
        }

        // Children removed (old but not new)
        for (kind, name) in &old_child_names - &new_child_names {
            changes.push(SymbolChange::new(
                new.name.clone(),
                new.kind.clone(),
                "modified".to_string(),
                "breaking".to_string(),
                Some(old.line_range),
                Some(new.line_range),
                vec![ChangeDetail {
                    aspect: kind.clone(),
                    change_type: "removed".to_string(),
                    description: format!("{} '{}' removed", kind, name),
                    old_value: Some(name.clone()),
                    new_value: None,
                    migration_note: None,
                }],
            ));
        }
    }

    if changes.is_empty() { None } else { Some(changes) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{SymbolParam, Visibility};

    fn make_sym(name: &str, params: Option<Vec<SymbolParam>>, return_type: Option<String>) -> Symbol {
        Symbol {
            id: name.to_string(),
            name: name.to_string(),
            kind: "function".to_string(),
            line_range: [1, 10],
            children: vec![],
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(Visibility::Public),
            value: None,
            signature: None,
            doc_comment: None,
            attributes: vec![],
            is_async: None,
            return_type,
            params,
        }
    }

    #[test]
    fn test_identical_symbols() {
        let s = make_sym("foo", None, None);
        assert!(diff(&s, &s).is_none());
    }

    #[test]
    fn test_params_changed() {
        let old = make_sym("foo", Some(vec![SymbolParam { name: "x".into(), ty: "u32".into(), optional: false, default_value: None }]), None);
        let new = make_sym("foo", Some(vec![SymbolParam { name: "y".into(), ty: "String".into(), optional: false, default_value: None }]), None);
        let result = diff(&old, &new);
        assert!(result.is_some());
    }

    #[test]
    fn test_return_type_changed() {
        let old = make_sym("foo", None, Some("u32".into()));
        let new = make_sym("foo", None, Some("String".into()));
        let result = diff(&old, &new);
        assert!(result.is_some());
    }
}
