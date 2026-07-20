use crate::diff::{ChangeDetail, SymbolChange};
use crate::symbols::{Symbol, SymbolParam};

pub fn diff(old: &Symbol, new: &Symbol) -> Option<Vec<SymbolChange>> {
    let mut changes = Vec::new();

    if let (Some(old_params), Some(new_params)) = (&old.params, &new.params) {
        let old_names: std::collections::HashSet<String> = old_params.iter().map(|p| p.name.clone()).collect();
        let new_names: std::collections::HashSet<String> = new_params.iter().map(|p| p.name.clone()).collect();

        for name in &old_names - &new_names {
            let mut sc = SymbolChange {
                symbol: new.name.clone(),
                kind: new.kind.clone(),
                change_type: "modified".to_string(),
                severity: "breaking".to_string(),
                old_name: None,
                rename_confidence: None,
                details: Vec::new(),
                old_line_range: Some(old.line_range),
                new_line_range: Some(new.line_range),
            };
            sc.details.push(ChangeDetail {
                aspect: "signature".to_string(),
                change_type: "removed".to_string(),
                description: format!("parameter '{}' removed", name),
                old_value: Some(name.clone()),
                new_value: None,
                migration_note: None,
            });
            changes.push(sc);
        }

        for name in &new_names - &old_names {
            let mut sc = SymbolChange {
                symbol: new.name.clone(),
                kind: new.kind.clone(),
                change_type: "modified".to_string(),
                severity: "breaking".to_string(),
                old_name: None,
                rename_confidence: None,
                details: Vec::new(),
                old_line_range: Some(old.line_range),
                new_line_range: Some(new.line_range),
            };
            sc.details.push(ChangeDetail {
                aspect: "signature".to_string(),
                change_type: "added".to_string(),
                description: format!("parameter '{}' added", name),
                old_value: None,
                new_value: Some(name.clone()),
                migration_note: None,
            });
            changes.push(sc);
        }

        for (old_p, new_p) in old_params.iter().zip(new_params.iter()) {
            if old_p.ty != new_p.ty {
                let mut sc = SymbolChange {
                    symbol: new.name.clone(),
                    kind: new.kind.clone(),
                    change_type: "modified".to_string(),
                    severity: "breaking".to_string(),
                    old_name: None,
                    rename_confidence: None,
                    details: Vec::new(),
                    old_line_range: Some(old.line_range),
                    new_line_range: Some(new.line_range),
                };
                sc.details.push(ChangeDetail {
                    aspect: "signature".to_string(),
                    change_type: "changed".to_string(),
                    description: format!("parameter '{}' type changed from '{}' to '{}'", old_p.name, old_p.ty, new_p.ty),
                    old_value: Some(old_p.ty.clone()),
                    new_value: Some(new_p.ty.clone()),
                    migration_note: None,
                });
                changes.push(sc);
            }

            if old_p.optional != new_p.optional {
                let mut sc = SymbolChange {
                    symbol: new.name.clone(),
                    kind: new.kind.clone(),
                    change_type: "modified".to_string(),
                    severity: "compatible".to_string(),
                    old_name: None,
                    rename_confidence: None,
                    details: Vec::new(),
                    old_line_range: Some(old.line_range),
                    new_line_range: Some(new.line_range),
                };
                sc.details.push(ChangeDetail {
                    aspect: "signature".to_string(),
                    change_type: "changed".to_string(),
                    description: format!("parameter '{}' became {}", old_p.name, if new_p.optional { "optional" } else { "required" }),
                    old_value: Some(format!("{}optional", if old_p.optional { "" } else { "not " })),
                    new_value: Some(format!("{}optional", if new_p.optional { "" } else { "not " })),
                    migration_note: None,
                });
                changes.push(sc);
            }
        }
    }

    if old.return_type != new.return_type {
        let mut sc = SymbolChange {
            symbol: new.name.clone(),
            kind: new.kind.clone(),
            change_type: "modified".to_string(),
            severity: "breaking".to_string(),
            old_name: None,
            rename_confidence: None,
            details: Vec::new(),
            old_line_range: Some(old.line_range),
            new_line_range: Some(new.line_range),
        };
        sc.details.push(ChangeDetail {
            aspect: "signature".to_string(),
            change_type: "changed".to_string(),
            description: format!("return type changed from '{}' to '{}'", 
                old.return_type.as_deref().unwrap_or("void"), 
                new.return_type.as_deref().unwrap_or("void")),
            old_value: old.return_type.clone(),
            new_value: new.return_type.clone(),
            migration_note: None,
        });
        changes.push(sc);
    }

    if old.is_async != new.is_async {
        let mut sc = SymbolChange {
            symbol: new.name.clone(),
            kind: new.kind.clone(),
            change_type: "modified".to_string(),
            severity: "breaking".to_string(),
            old_name: None,
            rename_confidence: None,
            details: Vec::new(),
            old_line_range: Some(old.line_range),
            new_line_range: Some(new.line_range),
        };
        sc.details.push(ChangeDetail {
            aspect: "signature".to_string(),
            change_type: "changed".to_string(),
            description: format!("function {} is_async", if new.is_async.unwrap_or(false) { "became" } else { "is no longer" }),
            old_value: Some(format!("{}is_async", if old.is_async.unwrap_or(false) { "" } else { "not " })),
            new_value: Some(format!("{}is_async", if new.is_async.unwrap_or(false) { "" } else { "not " })),
            migration_note: None,
        });
        changes.push(sc);
    }

    if changes.is_empty() { None } else { Some(changes) }
}
