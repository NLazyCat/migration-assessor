pub mod api_map;
pub mod matcher;
pub mod naming;
pub mod signature;

use crate::diff::FileDiffResult;
use std::path::Path;

/// Main entry: enrich all FileDiffResult symbol changes with target locations.
///
/// Reads the target project (if configured and exists), extracts its symbols,
/// then runs three-level matching for every source change.
pub fn resolve_all(
    file_changes: &mut [FileDiffResult],
    target_root: Option<&Path>,
    source_lang: &str,
    target_lang: &str,
) {
    let Some(root) = target_root else { return };
    if !root.exists() {
        return;
    }

    // Parse target project symbols
    let target_symbols = match crate::symbols::SymbolExtractor::extract_all_from_dir(root, target_lang)
    {
        Ok(syms) => syms,
        Err(_) => return,
    };

    let naming_registry = naming::NamingRegistry::new(source_lang, target_lang);
    let api_registry = api_map::ApiMapRegistry::new(source_lang, target_lang);

    for fc in file_changes.iter_mut() {
        for sc in fc.symbol_changes.iter_mut() {
            if sc.target_file.is_some() {
                continue;
            }
            let result = matcher::match_symbol(
                &fc.file,
                &sc.symbol,
                &sc.kind,
                &target_symbols,
                &naming_registry,
                &api_registry,
            );
            let target_file = result.target_file.clone();
            let target_symbol = result.target_symbol.clone();
            sc.target_file = result.target_file;
            sc.target_symbol = result.target_symbol;
            let (child, line_range) = matcher::resolve_child_context(
                &sc.symbol,
                &sc.details,
                target_file.as_deref(),
                target_symbol.as_deref(),
                &target_symbols,
                &naming_registry,
            );
            sc.target_child = child;
            sc.target_line_range = line_range;
        }
    }
}
