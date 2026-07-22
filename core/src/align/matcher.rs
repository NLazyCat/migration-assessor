use super::api_map::ApiMapRegistry;
use super::naming::NamingRegistry;
use super::signature;
use crate::diff::ChangeDetail;
use crate::symbols::SymbolIndex;

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub target_file: Option<String>,
    pub target_symbol: Option<String>,
    pub target_child: Option<String>,
    pub target_line_range: Option<[usize; 2]>,
    pub confidence: f64,
}

/// Three-level matching for a single symbol change.
///
/// Level 1 (name + file): Try same-file match with translated name.
/// Level 2 (signature): If ambiguous, compare parameter/return types.
pub fn match_symbol(
    source_file: &str,
    source_symbol: &str,
    _source_kind: &str,
    target_project: &[(SymbolIndex, crate::symbols::ApiContract)],
    naming: &NamingRegistry,
    _api_map: &ApiMapRegistry,
) -> MatchResult {
    let indices: Vec<&SymbolIndex> = target_project.iter().map(|(idx, _)| idx).collect();

    // Default file-level target (extension swap)
    let default_file = default_target_file(source_file);

    // Narrow to candidate files
    let candidates = find_candidate_files(source_file, &indices);

    if candidates.is_empty() {
        return MatchResult {
            target_file: default_file,
            target_symbol: None,
            target_child: None,
            target_line_range: None,
            confidence: 0.0,
        };
    }

    // Level 1: Try name match in candidate files
    let name_candidates = naming.candidates(source_symbol);
    for file_idx in &candidates {
        for s in file_idx.all_symbols() {
            if name_candidates.iter().any(|c| c == &s.name) {
                return MatchResult {
                    target_file: Some(file_idx.module.clone()),
                    target_symbol: Some(s.name.clone()),
                    target_child: None,
                    target_line_range: Some(s.line_range),
                    confidence: 0.85,
                };
            }
        }
    }

    // Level 2: Signature match
    let mut scored: Vec<(String, String, f64)> = Vec::new();
    for file_idx in &candidates {
        for s in file_idx.all_symbols() {
            let src_params: Vec<(String, String)> = Vec::new();
            let tgt_params: Vec<(String, String)> = s
                .params
                .as_ref()
                .map(|p| p.iter().map(|p| (p.name.clone(), p.ty.clone())).collect())
                .unwrap_or_default();

            let score = signature::compare_signatures(
                source_symbol,
                &src_params,
                None,
                &s.name,
                &tgt_params,
                s.return_type.as_deref(),
                naming,
            );
            if score > 0.5 {
                scored.push((file_idx.module.clone(), s.name.clone(), score));
            }
        }
    }

    // Pick best
    scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    if let Some((file, name, conf)) = scored.into_iter().next() {
        return MatchResult {
            target_file: Some(file),
            target_symbol: Some(name),
            target_child: None,
            target_line_range: None,
            confidence: conf,
        };
    }

    // No match found — return default file
    MatchResult {
        target_file: default_file,
        target_symbol: None,
        target_child: None,
        target_line_range: None,
        confidence: 0.0,
    }
}

fn find_candidate_files<'a>(
    source_file: &str,
    target_symbols: &[&'a SymbolIndex],
) -> Vec<&'a SymbolIndex> {
    let default = default_target_file(source_file);

    // Prefer same-name file after extension swap
    let same_name: Vec<&SymbolIndex> = target_symbols
        .iter()
        .filter(|idx| {
            if let Some(ref d) = default {
                &idx.module == d
            } else {
                false
            }
        })
        .copied()
        .collect();
    if !same_name.is_empty() {
        return same_name;
    }

    // Fallback: all target files
    target_symbols.to_vec()
}

/// Default extension swap for file-level target
const EXTENSION_MAP: &[(&str, &str)] = &[
    (".ts", ".rs"),
    (".tsx", ".rs"),
    (".js", ".rs"),
    (".vue", ".rs"),
];

fn default_target_file(source_file: &str) -> Option<String> {
    let source = source_file.replace('\\', "/");
    for (from_ext, to_ext) in EXTENSION_MAP {
        if let Some(stem) = source.strip_suffix(from_ext) {
            return Some(format!("{}{}", stem, to_ext));
        }
    }
    None
}

/// After parent-level match, resolve child/field-level target context.
///
/// For each relevant detail (member added/removed), translates the child name
/// via naming conventions and locates it in the matched target parent symbol's children.
pub fn resolve_child_context(
    source_symbol: &str,
    details: &[ChangeDetail],
    matched_file: Option<&str>,
    matched_parent: Option<&str>,
    target_project: &[(SymbolIndex, crate::symbols::ApiContract)],
    naming: &NamingRegistry,
) -> (Option<String>, Option<[usize; 2]>) {
    let Some(tf) = matched_file else { return (None, None) };
    let Some(_parent) = matched_parent else { return (None, None) };

    // Get the first child-level detail (member, method, or property)
    let detail = match details.iter().find(|d| d.aspect == "member" || d.aspect == "method" || d.aspect == "property") {
        Some(d) => d,
        None => return (None, None),
    };

    let child_source = detail.new_value.as_deref()
        .or(detail.old_value.as_deref())
        .unwrap_or("");
    if child_source.is_empty() {
        return (None, None);
    }

    // Find the matched target parent symbol
    let target_idx = target_project.iter().find(|(idx, _)| idx.module == *tf);
    let Some((idx, _)) = target_idx else { return (None, None) };

    let parent_candidates = naming.candidates(source_symbol);
    let all_syms = idx.all_symbols();
    let target_parent = all_syms.iter().find(|s| parent_candidates.contains(&s.name));
    let Some(parent_sym) = target_parent else { return (None, None) };

    // Translate child name
    let translated = naming.translate_name(child_source);

    // Look for existing child in target parent
    let matched = parent_sym.children.iter().find(|c| c.name == translated);
    match matched {
        Some(child) => (Some(child.name.clone()), Some(child.line_range)),
        None => (Some(translated), parent_sym.line_range.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::{ApiContract, Symbol};

    fn make_target_project() -> Vec<(SymbolIndex, ApiContract)> {
        vec![(
            SymbolIndex {
                module: "src/user.rs".into(),
                symbols: vec![Symbol {
                    id: "src/user.rs::User".into(),
                    name: "User".into(),
                    kind: "struct".into(),
                    line_range: [1, 10],
                    children: vec![],
                    partial_analysis: false,
                    partial_reason: None,
                    visibility: None,
                    value: None,
                    signature: None,
                    doc_comment: None,
                    attributes: vec![],
                    is_async: None,
                    return_type: None,
                    params: None,
                }],
            },
            ApiContract {
                module: "src/user.rs".into(),
                exports: vec![],
            },
        )]
    }

    #[test]
    fn test_match_symbol_by_name() {
        let naming = NamingRegistry::new("typescript", "rust");
        let api_map = ApiMapRegistry::new("typescript", "rust");
        let targets = make_target_project();

        let result = match_symbol(
            "src/models/user.ts",
            "IUser",
            "interface",
            &targets,
            &naming,
            &api_map,
        );
        assert_eq!(result.target_file.as_deref(), Some("src/user.rs"));
        assert_eq!(result.target_symbol.as_deref(), Some("User"));
        assert!((result.confidence - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_match_symbol_fallback_default() {
        let naming = NamingRegistry::new("typescript", "rust");
        let api_map = ApiMapRegistry::new("typescript", "rust");
        let targets = vec![];

        let result = match_symbol(
            "src/models/user.ts",
            "NonExistent",
            "function",
            &targets,
            &naming,
            &api_map,
        );
        assert_eq!(result.target_file.as_deref(), Some("src/models/user.rs"));
        assert!(result.target_symbol.is_none());
        assert!((result.confidence - 0.0).abs() < 0.01);
    }
}
