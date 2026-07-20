use std::path::{Path, PathBuf};

use migration_core::diff::engine::DiffEngine;
use migration_core::diff::{DiffReport, FileDiffResult};
use migration_core::language::{Language, LanguageRegistry};

use super::git_utils::{
    create_temp_dir, get_changed_files, get_file_at_version, is_analyzable_file,
};

pub(crate) fn run_ast_diff(
    project_root: &Path,
    source_repo: Option<&str>,
    from_version: &str,
    to_version: &str,
    source_path: Option<&str>,
    language: &dyn Language,
) -> anyhow::Result<DiffReport> {
    let candidate = match source_path {
        Some(src) => {
            let p = Path::new(src);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                project_root.join(p)
            }
        }
        None => {
            if let Some(repo) = source_repo {
                return fetch_and_diff_remote(repo, from_version, to_version, language);
            } else {
                anyhow::bail!(
                    "Either source_repo or source must be configured in migration.toml"
                );
            }
        }
    };

    if !candidate.join(".git").exists() {
        if let Some(repo) = source_repo {
            return fetch_and_diff_remote(repo, from_version, to_version, language);
        } else {
            anyhow::bail!("Not a git repository: {}", candidate.display());
        }
    }

    let changed_files = get_changed_files(&candidate, from_version, to_version)?;

    let mut file_changes = Vec::new();
    for file in &changed_files {
        if !is_analyzable_file(file) {
            continue;
        }

        match (
            get_file_at_version(&candidate, from_version, file),
            get_file_at_version(&candidate, to_version, file),
        ) {
            (Ok(old_source), Ok(new_source)) => {
                let diff = DiffEngine::diff_files(&old_source, &new_source, file, language)?;
                file_changes.push(diff);
            }
            (Err(_), Ok(new_source)) => {
                let new_parsed = language.parse(&new_source, file)?;
                let (index, _) = language.extract_symbols(&new_parsed)?;
                let mut symbol_changes = Vec::new();
                for sym in index.all_symbols() {
                    symbol_changes.push(migration_core::diff::SymbolChange {
                        symbol: sym.name.clone(),
                        kind: sym.kind.clone(),
                        change_type: "added".to_string(),
                        severity: "low".to_string(),
                        old_name: None,
                        rename_confidence: None,
                        details: Vec::new(),
                        old_line_range: None,
                        new_line_range: Some(sym.line_range),
                    });
                }
                file_changes.push(FileDiffResult {
                    file: file.clone(),
                    status: "added".to_string(),
                    symbol_changes,
                    import_changes: Vec::new(),
                    doc_changes: Vec::new(),
                });
            }
            (Ok(old_source), Err(_)) => {
                let old_parsed = language.parse(&old_source, file)?;
                let (index, _) = language.extract_symbols(&old_parsed)?;
                let mut symbol_changes = Vec::new();
                for sym in index.all_symbols() {
                    symbol_changes.push(migration_core::diff::SymbolChange {
                        symbol: sym.name.clone(),
                        kind: sym.kind.clone(),
                        change_type: "removed".to_string(),
                        severity: "high".to_string(),
                        old_name: None,
                        rename_confidence: None,
                        details: Vec::new(),
                        old_line_range: Some(sym.line_range),
                        new_line_range: None,
                    });
                }
                file_changes.push(FileDiffResult {
                    file: file.clone(),
                    status: "removed".to_string(),
                    symbol_changes,
                    import_changes: Vec::new(),
                    doc_changes: Vec::new(),
                });
            }
            _ => {}
        }
    }

    let summary = super::propagation::compute_summary(&file_changes);

    Ok(DiffReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        from_version: Some(from_version.to_string()),
        to_version: to_version.to_string(),
        summary,
        file_changes,
        dependency_changes: Vec::new(),
        propagation: migration_core::diff::PropagationResult {
            affected_symbols: Vec::new(),
        },
    })
}

fn fetch_and_diff_remote(
    repo: &str,
    from_version: &str,
    to_version: &str,
    language: &dyn Language,
) -> anyhow::Result<DiffReport> {
    println!("  Fetching remote repo for AST-based diff...");
    let tmp_dir = create_temp_dir()?;
    let result = fetch_repo_and_diff(&tmp_dir, repo, from_version, to_version, language);
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

fn fetch_repo_and_diff(
    tmp_dir: &Path,
    repo: &str,
    from_version: &str,
    to_version: &str,
    language: &dyn Language,
) -> anyhow::Result<DiffReport> {
    let init = std::process::Command::new("git")
        .args(["init"])
        .current_dir(tmp_dir)
        .output()?;
    if !init.status.success() {
        anyhow::bail!("git init failed in temp dir");
    }

    let add_remote = std::process::Command::new("git")
        .args(["remote", "add", "origin", repo])
        .current_dir(tmp_dir)
        .output()?;
    if !add_remote.status.success() {
        let stderr = String::from_utf8_lossy(&add_remote.stderr);
        anyhow::bail!("git remote add failed: {}", stderr);
    }

    let fetch = std::process::Command::new("git")
        .args(["fetch", "origin", "--depth", "50"])
        .current_dir(tmp_dir)
        .output()?;
    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        anyhow::bail!("git fetch failed: {}", stderr);
    }

    let changed_files = get_changed_files(tmp_dir, from_version, to_version)?;

    let mut file_changes = Vec::new();
    for file in &changed_files {
        if !is_analyzable_file(file) {
            continue;
        }

        match (
            get_file_at_version(tmp_dir, from_version, file),
            get_file_at_version(tmp_dir, to_version, file),
        ) {
            (Ok(old_source), Ok(new_source)) => {
                let diff = DiffEngine::diff_files(&old_source, &new_source, file, language)?;
                file_changes.push(diff);
            }
            (Err(_), Ok(new_source)) => {
                let new_parsed = language.parse(&new_source, file)?;
                let (index, _) = language.extract_symbols(&new_parsed)?;
                let mut symbol_changes = Vec::new();
                for sym in index.all_symbols() {
                    symbol_changes.push(migration_core::diff::SymbolChange {
                        symbol: sym.name.clone(),
                        kind: sym.kind.clone(),
                        change_type: "added".to_string(),
                        severity: "low".to_string(),
                        old_name: None,
                        rename_confidence: None,
                        details: Vec::new(),
                        old_line_range: None,
                        new_line_range: Some(sym.line_range),
                    });
                }
                file_changes.push(FileDiffResult {
                    file: file.clone(),
                    status: "added".to_string(),
                    symbol_changes,
                    import_changes: Vec::new(),
                    doc_changes: Vec::new(),
                });
            }
            (Ok(old_source), Err(_)) => {
                let old_parsed = language.parse(&old_source, file)?;
                let (index, _) = language.extract_symbols(&old_parsed)?;
                let mut symbol_changes = Vec::new();
                for sym in index.all_symbols() {
                    symbol_changes.push(migration_core::diff::SymbolChange {
                        symbol: sym.name.clone(),
                        kind: sym.kind.clone(),
                        change_type: "removed".to_string(),
                        severity: "high".to_string(),
                        old_name: None,
                        rename_confidence: None,
                        details: Vec::new(),
                        old_line_range: Some(sym.line_range),
                        new_line_range: None,
                    });
                }
                file_changes.push(FileDiffResult {
                    file: file.clone(),
                    status: "removed".to_string(),
                    symbol_changes,
                    import_changes: Vec::new(),
                    doc_changes: Vec::new(),
                });
            }
            _ => {}
        }
    }

    let summary = super::propagation::compute_summary(&file_changes);

    Ok(DiffReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        from_version: Some(from_version.to_string()),
        to_version: to_version.to_string(),
        summary,
        file_changes,
        dependency_changes: Vec::new(),
        propagation: migration_core::diff::PropagationResult {
            affected_symbols: Vec::new(),
        },
    })
}

pub(crate) fn detect_project_language(
    lang_registry: &'static LanguageRegistry,
    project_root: &Path,
    source_path: Option<&str>,
) -> Option<String> {
    if let Some(src) = source_path {
        let candidate = if Path::new(src).is_absolute() {
            PathBuf::from(src)
        } else {
            project_root.join(src)
        };
        if candidate.exists()
            && let Some(lang) = lang_registry.detect_language(&candidate)
        {
            return Some(lang);
        }
    }
    lang_registry.detect_language(project_root)
}
