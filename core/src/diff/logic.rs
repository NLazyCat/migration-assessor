use crate::diff::{ChangeDetail, SymbolChange};
use crate::symbols::Symbol;

pub fn diff_value(old: &Symbol, new: &Symbol) -> Option<SymbolChange> {
    if old.value != new.value && (old.kind == "const" || old.kind == "variable" || old.kind == "static") {
        Some(SymbolChange::new(
            new.name.clone(),
            new.kind.clone(),
            "modified".to_string(),
            "compatible".to_string(),
            Some(old.line_range),
            Some(new.line_range),
            vec![ChangeDetail {
                aspect: "value".to_string(),
                change_type: "changed".to_string(),
                description: format!("value changed from '{}' to '{}'", 
                    old.value.as_deref().unwrap_or(""), 
                    new.value.as_deref().unwrap_or("")),
                old_value: old.value.clone(),
                new_value: new.value.clone(),
                migration_note: None,
            }],
        ))
    } else {
        None
    }
}

pub fn diff_body(_old: &Symbol, _new: &Symbol) -> Option<Vec<SymbolChange>> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::Visibility;

    fn make_sym(name: &str, kind: &str, value: Option<String>) -> Symbol {
        Symbol {
            id: name.to_string(),
            name: name.to_string(),
            kind: kind.to_string(),
            line_range: [1, 10],
            children: vec![],
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(Visibility::Public),
            value,
            signature: None,
            doc_comment: None,
            attributes: vec![],
            is_async: None,
            return_type: None,
            params: None,
        }
    }

    #[test]
    fn test_same_value() {
        let s = make_sym("X", "const", Some("42".into()));
        assert!(diff_value(&s, &s).is_none());
    }

    #[test]
    fn test_changed_value() {
        let old = make_sym("X", "const", Some("42".into()));
        let new = make_sym("X", "const", Some("43".into()));
        let result = diff_value(&old, &new);
        assert!(result.is_some());
        assert_eq!(result.unwrap().change_type, "modified");
    }

    #[test]
    fn test_non_const_kind_ignored() {
        let old = make_sym("f", "function", Some("old".into()));
        let new = make_sym("f", "function", Some("new".into()));
        assert!(diff_value(&old, &new).is_none());
    }
}
