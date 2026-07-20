use crate::diff::{DiffReport, DiffSummary, FileDiffResult, PropagationResult};
use crate::git;
use crate::language::{Language, LanguageRegistry};
use std::path::Path;

pub struct DiffEngine;

impl DiffEngine {
    pub fn diff_files(
        old_source: &str,
        new_source: &str,
        file_path: &str,
        language: &dyn Language,
    ) -> anyhow::Result<FileDiffResult> {
        let old_parsed = language.parse(old_source, file_path)?;
        let new_parsed = language.parse(new_source, file_path)?;

        language.diff_analyzer().diff_files(&old_parsed, &new_parsed)
    }

    pub fn diff_project(
        project_root: &Path,
        from_version: &str,
        to_version: &str,
    ) -> anyhow::Result<DiffReport> {
        let lang_name = LanguageRegistry::get()
            .detect_language(project_root)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect project language"))?;

        let language = LanguageRegistry::get()
            .get_language(&lang_name)
            .ok_or_else(|| anyhow::anyhow!("Language {} not supported", lang_name))?;

        let changed_files = git::get_changed_files(project_root, from_version, to_version)?;

        let mut file_changes = Vec::new();
        for file in &changed_files {
            match (git::get_file_at_version(project_root, from_version, file), git::get_file_at_version(project_root, to_version, file)) {
                (Ok(old_source), Ok(new_source)) => {
                    let diff = Self::diff_files(&old_source, &new_source, file, language)?;
                    file_changes.push(diff);
                }
                (Err(_), Ok(_new_source)) => {
                    file_changes.push(FileDiffResult {
                        file: file.clone(),
                        status: "added".to_string(),
                        symbol_changes: Vec::new(),
                        import_changes: Vec::new(),
                        doc_changes: Vec::new(),
                    });
                }
                (Ok(_), Err(_)) => {
                    file_changes.push(FileDiffResult {
                        file: file.clone(),
                        status: "removed".to_string(),
                        symbol_changes: Vec::new(),
                        import_changes: Vec::new(),
                        doc_changes: Vec::new(),
                    });
                }
                _ => {}
            }
        }

        let summary = compute_summary(&file_changes);

        Ok(DiffReport {
            generated_at: chrono::Local::now().to_rfc3339(),
            from_version: Some(from_version.to_string()),
            to_version: to_version.to_string(),
            summary,
            file_changes,
            dependency_changes: Vec::new(),
            propagation: PropagationResult {
                affected_symbols: Vec::new(),
            },
        })
    }

    /// Diff HEAD vs working tree (uncommitted changes) for a project.
    pub fn diff_uncommitted(project_root: &Path) -> anyhow::Result<DiffReport> {
        let lang_name = LanguageRegistry::get()
            .detect_language(project_root)
            .ok_or_else(|| anyhow::anyhow!("Cannot detect project language"))?;

        let language = LanguageRegistry::get()
            .get_language(&lang_name)
            .ok_or_else(|| anyhow::anyhow!("Language {} not supported", lang_name))?;

        let changed_files = git::get_uncommitted_files(project_root)?;

        let mut file_changes = Vec::new();
        for file in &changed_files {
            let old_source = git::get_file_at_version(project_root, "HEAD", file).ok();
            let new_source = std::fs::read_to_string(project_root.join(file)).ok();

            match (old_source, new_source) {
                (Some(old), Some(new)) => {
                    let diff = Self::diff_files(&old, &new, file, language)?;
                    file_changes.push(diff);
                }
                (None, Some(_)) => {
                    file_changes.push(FileDiffResult {
                        file: file.clone(),
                        status: "added".to_string(),
                        symbol_changes: Vec::new(),
                        import_changes: Vec::new(),
                        doc_changes: Vec::new(),
                    });
                }
                _ => {}
            }
        }

        let summary = compute_summary(&file_changes);

        Ok(DiffReport {
            generated_at: chrono::Local::now().to_rfc3339(),
            from_version: Some("HEAD".to_string()),
            to_version: "working-tree".to_string(),
            summary,
            file_changes,
            dependency_changes: Vec::new(),
            propagation: PropagationResult {
                affected_symbols: Vec::new(),
            },
        })
    }
}

fn compute_summary(file_changes: &[FileDiffResult]) -> DiffSummary {
    let mut summary = DiffSummary {
        total_files_changed: file_changes.len(),
        symbols_added: 0,
        symbols_removed: 0,
        symbols_renamed: 0,
        symbols_modified: 0,
        breaking_changes: 0,
        new_dependencies: 0,
        removed_dependencies: 0,
    };

    for fc in file_changes {
        for sc in &fc.symbol_changes {
            match sc.change_type.as_str() {
                "added" => summary.symbols_added += 1,
                "removed" => summary.symbols_removed += 1,
                "renamed" => summary.symbols_renamed += 1,
                "modified" => summary.symbols_modified += 1,
                _ => {}
            }
            if sc.severity == "breaking" {
                summary.breaking_changes += 1;
            }
        }
        summary.new_dependencies += fc.import_changes.iter().filter(|ic| ic.change_type == "added").count();
        summary.removed_dependencies += fc.import_changes.iter().filter(|ic| ic.change_type == "removed").count();
    }

    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::SymbolChange;
    use crate::diff::ImportChange;

    fn mock_language() -> &'static dyn Language {
        LanguageRegistry::get().get_language("typescript").unwrap()
    }

    #[test]
    fn test_diff_files_no_changes() {
        let source = "const x = 1;\nexport function foo() { return x; }";
        let result = DiffEngine::diff_files(source, source, "test.ts", mock_language()).unwrap();
        assert_eq!(result.file, "test.ts");
        assert_eq!(result.status, "modified");
    }

    #[test]
    fn test_diff_files_with_added_symbol() {
        let old = "const x = 1;";
        let new = "const x = 1;\nexport function added() { return 42; }";
        let result = DiffEngine::diff_files(old, new, "test.ts", mock_language()).unwrap();
        let added: Vec<&SymbolChange> = result.symbol_changes.iter().filter(|s| s.change_type == "added").collect();
        assert!(!added.is_empty(), "should detect added symbols");
    }

    #[test]
    fn test_diff_files_with_removed_symbol() {
        let old = "const x = 1;\nexport function removed() { return 42; }";
        let new = "const x = 1;";
        let result = DiffEngine::diff_files(old, new, "test.ts", mock_language()).unwrap();
        let removed: Vec<&SymbolChange> = result.symbol_changes.iter().filter(|s| s.change_type == "removed").collect();
        assert!(!removed.is_empty(), "should detect removed symbols");
    }

    #[test]
    fn test_compute_summary_empty() {
        let summary = compute_summary(&[]);
        assert_eq!(summary.total_files_changed, 0);
        assert_eq!(summary.symbols_added, 0);
    }

    #[test]
    fn test_compute_summary_counts() {
        let changes = vec![FileDiffResult {
            file: "test.ts".into(),
            status: "modified".into(),
            symbol_changes: vec![
                SymbolChange::new(
                    "foo".into(),
                    "function".into(),
                    "added".into(),
                    "low".into(),
                    None,
                    None,
                    vec![],
                ),
                SymbolChange::new(
                    "bar".into(),
                    "function".into(),
                    "removed".into(),
                    "breaking".into(),
                    None,
                    None,
                    vec![],
                ),
            ],
            import_changes: vec![
                ImportChange {
                    change_type: "added".into(),
                    package: "lodash".into(),
                    old_path: None,
                    new_path: Some("lodash".into()),
                    is_external: true,
                    compatibility: None,
                },
            ],
            doc_changes: vec![],
        }];
        let summary = compute_summary(&changes);
        assert_eq!(summary.total_files_changed, 1);
        assert_eq!(summary.symbols_added, 1);
        assert_eq!(summary.symbols_removed, 1);
        assert_eq!(summary.breaking_changes, 1);
        assert_eq!(summary.new_dependencies, 1);
    }
}
