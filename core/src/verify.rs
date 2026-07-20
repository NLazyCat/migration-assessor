use crate::align::naming::NamingRegistry;
use crate::diff::engine::DiffEngine;
use crate::diff::FileDiffResult;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnmatchedSymbol {
    pub file: String,
    pub symbol: String,
    pub change_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyResult {
    pub coverage: f64,
    pub matched: usize,
    pub total: usize,
    pub threshold: f64,
    pub passed: bool,
    pub unmatched: Vec<UnmatchedSymbol>,
    pub source_files: usize,
    pub target_files: usize,
}

impl VerifyResult {
    pub fn summary_line(&self) -> String {
        format!(
            "{:.1}% coverage ({}/{} symbols) — {}",
            self.coverage * 100.0,
            self.matched,
            self.total,
            if self.passed { "PASS" } else { "FAIL" }
        )
    }
}

/// Compare source diff (X→Y) against target project's uncommitted changes.
/// Uses naming registry to translate symbol names across languages.
pub fn verify_migration(
    source_diff: &[FileDiffResult],
    target_root: &Path,
    source_lang: &str,
    target_lang: &str,
    threshold: f64,
) -> anyhow::Result<VerifyResult> {
    let target_diff = DiffEngine::diff_uncommitted(target_root)?;

    let naming = NamingRegistry::new(source_lang, target_lang);

    let mut matched = 0;
    let mut total = 0;
    let mut unmatched = Vec::new();

    for source_fc in source_diff {
        let target_fc = find_matching_target_file(&target_diff.file_changes, &source_fc.file, source_lang, target_lang);

        for source_sc in &source_fc.symbol_changes {
            total += 1;

            let expected_name = naming.translate_name(&source_sc.symbol);
            let found = target_fc.and_then(|tf| {
                tf.symbol_changes.iter().find(|tsc| {
                    let name_matches = tsc.symbol == expected_name
                        || tsc.symbol.to_lowercase() == expected_name.to_lowercase()
                        || tsc.symbol == source_sc.symbol;
                    let change_matches = tsc.change_type == source_sc.change_type;
                    name_matches && change_matches
                })
            });

            if found.is_some() {
                matched += 1;
            } else {
                unmatched.push(UnmatchedSymbol {
                    file: source_fc.file.clone(),
                    symbol: source_sc.symbol.clone(),
                    change_type: source_sc.change_type.clone(),
                });
            }
        }
    }

    let coverage = if total > 0 {
        matched as f64 / total as f64
    } else {
        1.0
    };

    Ok(VerifyResult {
        coverage,
        matched,
        total,
        threshold,
        passed: coverage >= threshold,
        unmatched,
        source_files: source_diff.len(),
        target_files: target_diff.file_changes.len(),
    })
}

/// Find the matching target file for a given source file.
/// Translates `.` to `_` (user.service → user_service) and removes
/// language-specific suffixes.
fn find_matching_target_file<'a>(
    target_changes: &'a [FileDiffResult],
    source_file: &str,
    _source_lang: &str,
    _target_lang: &str,
) -> Option<&'a FileDiffResult> {
    let source_stem = Path::new(source_file)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    // Convert dot-notation to snake_case (user.service → user_service)
    let source_normalized = source_stem.replace('.', "_");

    target_changes.iter().find(|tf| {
        let target_stem = Path::new(&tf.file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let target_normalized = target_stem.replace('.', "_");
        target_normalized == source_normalized || target_stem == source_stem
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{FileDiffResult, SymbolChange};

    fn make_symbol_change(name: &str, change_type: &str) -> SymbolChange {
        SymbolChange::new(
            name.to_string(),
            "function".to_string(),
            change_type.to_string(),
            "compatible".to_string(),
            None,
            None,
            vec![],
        )
    }

    fn make_file_diff(file: &str, symbols: Vec<SymbolChange>) -> FileDiffResult {
        FileDiffResult {
            file: file.to_string(),
            status: "modified".to_string(),
            symbol_changes: symbols,
            import_changes: vec![],
            doc_changes: vec![],
        }
    }

    #[test]
    fn test_verify_requires_real_git_repo() {
        let source_diff = vec![make_file_diff(
            "user.service.ts",
            vec![make_symbol_change("search_users", "modified")],
        )];

        let result = verify_migration(
            &source_diff,
            Path::new(""),
            "typescript",
            "rust",
            0.9,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_find_matching_target_file() {
        let target_changes = vec![
            make_file_diff("user_service.rs", vec![]),
            make_file_diff("order_service.rs", vec![]),
        ];

        let result = find_matching_target_file(&target_changes, "user.service.ts", "typescript", "rust");
        assert!(result.is_some());
        assert_eq!(result.unwrap().file, "user_service.rs");
    }

    #[test]
    fn test_find_matching_target_file_no_match() {
        let target_changes = vec![
            make_file_diff("user_service.rs", vec![]),
        ];

        let result = find_matching_target_file(&target_changes, "product.service.ts", "typescript", "rust");
        assert!(result.is_none());
    }

    #[test]
    fn test_verify_result_summary_line() {
        let result = VerifyResult {
            coverage: 0.92,
            matched: 23,
            total: 25,
            threshold: 0.9,
            passed: true,
            unmatched: vec![],
            source_files: 5,
            target_files: 5,
        };
        assert!(result.summary_line().contains("92.0%"));
        assert!(result.summary_line().contains("PASS"));
    }

    #[test]
    fn test_verify_result_fail() {
        let result = VerifyResult {
            coverage: 0.75,
            matched: 15,
            total: 20,
            threshold: 0.9,
            passed: false,
            unmatched: vec![],
            source_files: 5,
            target_files: 4,
        };
        assert!(result.summary_line().contains("75.0%"));
        assert!(result.summary_line().contains("FAIL"));
    }
}
