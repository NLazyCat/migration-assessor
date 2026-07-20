use crate::diff::{DiffReport, DiffSummary, FileDiffResult, PropagationResult};
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

        let changed_files = get_changed_files(project_root, from_version, to_version)?;

        let mut file_changes = Vec::new();
        for file in &changed_files {
            match (get_file_at_version(project_root, from_version, file), get_file_at_version(project_root, to_version, file)) {
                (Ok(old_source), Ok(new_source)) => {
                    let diff = Self::diff_files(&old_source, &new_source, file, language)?;
                    file_changes.push(diff);
                }
                (Err(_), Ok(new_source)) => {
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
}

fn get_changed_files(project_root: &Path, from_version: &str, to_version: &str) -> anyhow::Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", from_version, to_version])
        .current_dir(project_root)
        .output()?;

    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(files)
}

fn get_file_at_version(project_root: &Path, version: &str, file_path: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{}:{}", version, file_path)])
        .current_dir(project_root)
        .output()?;

    if !output.status.success() {
        return Err(anyhow::anyhow!("Failed to get file {} at version {}", file_path, version));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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
