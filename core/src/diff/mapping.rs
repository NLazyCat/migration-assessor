use crate::symbols::{Symbol, SymbolIndex};
use std::collections::HashMap;

pub struct SymbolMapping<'a> {
    pub renamed: HashMap<String, String>,
    pub added: Vec<&'a Symbol>,
    pub removed: Vec<&'a Symbol>,
    pub stable: Vec<(&'a Symbol, &'a Symbol)>,
    pub confidence: HashMap<String, f64>,
}

pub fn build_symbol_mapping<'a>(old: &'a SymbolIndex, new: &'a SymbolIndex) -> SymbolMapping<'a> {
    let old_by_name: HashMap<&str, &Symbol> = old.symbols.iter().map(|s| (s.name.as_str(), s)).collect();
    let new_by_name: HashMap<&str, &Symbol> = new.symbols.iter().map(|s| (s.name.as_str(), s)).collect();

    let mut renamed = HashMap::new();
    let mut confidence = HashMap::new();

    let removed_names: Vec<&str> = old.symbols.iter()
        .filter(|s| !new_by_name.contains_key(s.name.as_str()))
        .map(|s| s.name.as_str())
        .collect();

    let added_names: Vec<&str> = new.symbols.iter()
        .filter(|s| !old_by_name.contains_key(s.name.as_str()))
        .map(|s| s.name.as_str())
        .collect();

    for &old_name in &removed_names {
        let old_sym = old_by_name[old_name];
        for &new_name in &added_names {
            let new_sym = new_by_name[new_name];
            if old_sym.kind != new_sym.kind {
                continue;
            }

            let sim = structural_similarity(old_sym, new_sym);
            if sim >= 0.75 {
                renamed.insert(old_sym.id.clone(), new_sym.id.clone());
                confidence.insert(old_sym.id.clone(), sim);
            }
        }
    }

    let added: Vec<&Symbol> = new.symbols.iter()
        .filter(|s| !old_by_name.contains_key(s.name.as_str()))
        .filter(|s| !renamed.values().any(|id| id == &s.id))
        .collect();

    let removed: Vec<&Symbol> = old.symbols.iter()
        .filter(|s| !new_by_name.contains_key(s.name.as_str()))
        .filter(|s| !renamed.contains_key(&s.id))
        .collect();

    let stable: Vec<(&Symbol, &Symbol)> = old.symbols.iter()
        .filter(|s| new_by_name.contains_key(s.name.as_str()))
        .map(|s| (s, new_by_name[s.name.as_str()]))
        .collect();

    SymbolMapping {
        renamed,
        added,
        removed,
        stable,
        confidence,
    }
}

fn structural_similarity(old: &Symbol, new: &Symbol) -> f64 {
    let old_kinds: Vec<&str> = old.children.iter().map(|c| c.kind.as_str()).collect();
    let new_kinds: Vec<&str> = new.children.iter().map(|c| c.kind.as_str()).collect();

    let lcs_len = lcs(&old_kinds, &new_kinds);
    let lcs_sim = if old_kinds.len() + new_kinds.len() == 0 {
        1.0
    } else {
        2.0 * lcs_len as f64 / (old_kinds.len() + new_kinds.len()) as f64
    };

    let old_lines = old.line_range[1] - old.line_range[0];
    let new_lines = new.line_range[1] - new.line_range[0];
    let line_sim = if old_lines.max(new_lines) == 0 {
        1.0
    } else {
        (old_lines.min(new_lines) as f64) / (old_lines.max(new_lines) as f64)
    };

    let child_sim = if old.children.is_empty() && new.children.is_empty() {
        1.0
    } else {
        (old.children.len().min(new.children.len()) as f64) / (old.children.len().max(new.children.len()) as f64)
    };

    lcs_sim * 0.5 + line_sim * 0.3 + child_sim * 0.2
}

fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut dp = vec![vec![0; b.len() + 1]; a.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            if a[i - 1] == b[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = dp[i - 1][j].max(dp[i][j - 1]);
            }
        }
    }
    dp[a.len()][b.len()]
}
