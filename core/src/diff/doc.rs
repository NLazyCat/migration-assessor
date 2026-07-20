use crate::diff::DocChange;
use crate::symbols::Symbol;

pub fn diff(old: &Symbol, new: &Symbol) -> Option<DocChange> {
    let old_deprecated = old.attributes.iter().any(|a| a == "#[deprecated]" || a == "@deprecated");
    let new_deprecated = new.attributes.iter().any(|a| a == "#[deprecated]" || a == "@deprecated");

    let old_has_todo = old.doc_comment.as_ref().map_or(false, |d| d.contains("TODO") || d.contains("FIXME") || d.contains("HACK"));
    let new_has_todo = new.doc_comment.as_ref().map_or(false, |d| d.contains("TODO") || d.contains("FIXME") || d.contains("HACK"));

    let old_has_safety = old.doc_comment.as_ref().map_or(false, |d| d.contains("SAFETY") || d.contains("unsafe"));
    let new_has_safety = new.doc_comment.as_ref().map_or(false, |d| d.contains("SAFETY") || d.contains("unsafe"));

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
