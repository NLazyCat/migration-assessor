use crate::diff::{ChangeDetail, SymbolChange};
use crate::symbols::Symbol;

pub fn diff_value(old: &Symbol, new: &Symbol) -> Option<SymbolChange> {
    if old.value != new.value && (old.kind == "const" || old.kind == "variable" || old.kind == "static") {
        Some(SymbolChange {
            symbol: new.name.clone(),
            kind: new.kind.clone(),
            change_type: "modified".to_string(),
            severity: "compatible".to_string(),
            old_name: None,
            rename_confidence: None,
            details: vec![ChangeDetail {
                aspect: "value".to_string(),
                change_type: "changed".to_string(),
                description: format!("value changed from '{}' to '{}'", 
                    old.value.as_deref().unwrap_or(""), 
                    new.value.as_deref().unwrap_or("")),
                old_value: old.value.clone(),
                new_value: new.value.clone(),
                migration_note: None,
            }],
            old_line_range: Some(old.line_range),
            new_line_range: Some(new.line_range),
        })
    } else {
        None
    }
}

pub fn diff_body(_old: &Symbol, _new: &Symbol) -> Option<Vec<SymbolChange>> {
    None
}
