use crate::diff::DocChange;
use crate::symbols::Symbol;

pub fn diff(old: &Symbol, new: &Symbol) -> Option<DocChange> {
    let old_deprecated = old.attributes.iter().any(|a| a == "#[deprecated]" || a == "@deprecated");
    let new_deprecated = new.attributes.iter().any(|a| a == "#[deprecated]" || a == "@deprecated");

    let old_has_todo = old.doc_comment.as_ref().is_some_and(|d| d.contains("TODO") || d.contains("FIXME") || d.contains("HACK"));
    let new_has_todo = new.doc_comment.as_ref().is_some_and(|d| d.contains("TODO") || d.contains("FIXME") || d.contains("HACK"));

    let old_has_safety = old.doc_comment.as_ref().is_some_and(|d| d.contains("SAFETY") || d.contains("unsafe"));
    let new_has_safety = new.doc_comment.as_ref().is_some_and(|d| d.contains("SAFETY") || d.contains("unsafe"));

    if old.doc_comment != new.doc_comment || old_deprecated != new_deprecated || old_has_todo != new_has_todo || old_has_safety != new_has_safety {
        let change_type = if old.doc_comment.is_none() && new.doc_comment.is_some() {
            "added".to_string()
        } else if old.doc_comment.is_some() && new.doc_comment.is_none() {
            "removed".to_string()
        } else {
            "changed".to_string()
        };

        Some(DocChange {
            change_type,
            symbol: new.name.clone(),
            is_deprecated: new_deprecated,
            has_todo: new_has_todo,
            has_safety_note: new_has_safety,
            old_doc: old.doc_comment.clone(),
            new_doc: new.doc_comment.clone(),
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::Visibility;

    fn make_sym(name: &str, doc: Option<&str>, deprecated: bool) -> Symbol {
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
            doc_comment: doc.map(|s| s.to_string()),
            attributes: if deprecated { vec!["#[deprecated]".to_string()] } else { vec![] },
            is_async: None,
            return_type: None,
            params: None,
        }
    }

    #[test]
    fn test_same_doc() {
        let s = make_sym("foo", Some("hello"), false);
        assert!(diff(&s, &s).is_none());
    }

    #[test]
    fn test_doc_changed() {
        let old = make_sym("foo", Some("old"), false);
        let new = make_sym("foo", Some("new"), false);
        let result = diff(&old, &new);
        assert!(result.is_some());
        assert_eq!(result.unwrap().change_type, "changed");
    }

    #[test]
    fn test_doc_added() {
        let old = make_sym("foo", None, false);
        let new = make_sym("foo", Some("new"), false);
        let result = diff(&old, &new);
        assert!(result.is_some());
        assert_eq!(result.unwrap().change_type, "added");
    }

    #[test]
    fn test_doc_removed() {
        let old = make_sym("foo", Some("old"), false);
        let new = make_sym("foo", None, false);
        let result = diff(&old, &new);
        assert!(result.is_some());
        assert_eq!(result.unwrap().change_type, "removed");
    }

    #[test]
    fn test_deprecated_added() {
        let old = make_sym("foo", None, false);
        let new = make_sym("foo", None, true);
        let result = diff(&old, &new);
        assert!(result.is_some());
        assert!(result.unwrap().is_deprecated);
    }
}
