use clap::Args;
use migration_core::diff::{DiffReport, FileDiffResult};
use migration_core::diff::engine::DiffEngine;
use migration_core::language::LanguageRegistry;
use migration_core::output_paths;
use migration_core::recommendation::{DependencyRecommendation, RecommendationReport};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::commands::context::ProjectContext;
use crate::commands::resolve_project_path;

#[derive(Args)]
pub struct DiffArgs {
    #[arg(default_value = ".")]
    pub path: String,

    #[arg(long, required_unless_present = "auto", conflicts_with = "auto")]
    pub new_version: Option<String>,

    #[arg(long)]
    pub auto: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DiffReportOutput {
    generated_at: String,
    source_repo: Option<String>,
    from_version: Option<String>,
    to_version: String,
    files: Vec<String>,
    file_changes: Vec<FileChangeGroup>,
    propagation: PropagationResult,
    summary: SummaryInfo,
}

#[derive(Debug, Clone, Serialize)]
struct SummaryInfo {
    total_files_changed: usize,
    symbols_added: usize,
    symbols_removed: usize,
    symbols_renamed: usize,
    symbols_modified: usize,
    breaking_changes: usize,
}

#[derive(Debug, Clone, Serialize)]
struct FileChangeGroup {
    file: String,
    source_attached: bool,
    changes: Vec<SymbolChangeDetail>,
    import_changes: Vec<ImportChangeDetail>,
    doc_changes: Vec<DocChangeDetail>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    recommendations: Vec<DependencyRecommendation>,
}

#[derive(Debug, Clone, Serialize)]
struct SymbolChangeDetail {
    symbol: String,
    kind: String,
    change_type: String,
    severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rename_confidence: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    details: Vec<ChangeDetailInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_line_range: Option<[usize; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_line_range: Option<[usize; 2]>,
}

#[derive(Debug, Clone, Serialize)]
struct ChangeDetailInfo {
    aspect: String,
    change_type: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    migration_note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ImportChangeDetail {
    change_type: String,
    package: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_path: Option<String>,
    is_external: bool,
}

#[derive(Debug, Clone, Serialize)]
struct DocChangeDetail {
    change_type: String,
    symbol: String,
    is_deprecated: bool,
    has_todo: bool,
    has_safety_note: bool,
}

#[derive(Debug, Clone, Serialize)]
struct PropagationResult {
    triggered_by: Vec<String>,
    affected_files: Vec<String>,
    chain: Vec<PropagationLink>,
}

#[derive(Debug, Clone, Serialize)]
struct PropagationLink {
    from: String,
    to: String,
    via: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReverseRef {
    symbol: String,
    #[allow(dead_code)]
    location: ReverseLocation,
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReverseLocation {
    #[allow(dead_code)]
    file: String,
    #[allow(dead_code)]
    line: usize,
    #[allow(dead_code)]
    column: usize,
}

type ReverseIndex = HashMap<String, Vec<ReverseRef>>;

pub fn run(args: &DiffArgs) -> anyhow::Result<()> {
    let project_root = resolve_project_path(&args.path);
    let config_path = project_root.join("migration.toml");

    if !config_path.exists() {
        anyhow::bail!(
            "No migration.toml found in {}.\n\
             Run 'migration-analyze analyze' to analyze the project first.",
            project_root.display()
        );
    }

    let ctx = ProjectContext::load(&project_root)?;
    let config = &ctx.config;

    let migration_dir = ctx.migration_folder.clone();
    let report_dir = ctx.report_dir.clone();

    if !report_dir.exists() {
        anyhow::bail!(
            "Report folder not found at {}. Run 'migration-analyze analyze' first.",
            report_dir.display()
        );
    }

    let source_repo = config.project.source_repo.clone();
    let from_version = config.project.source_version.clone();
    let new_version = if args.auto {
        let repo = source_repo
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--auto requires source_repo in migration.toml"))?;
        let latest = fetch_latest_version(repo)?;
        println!("  Auto-detected latest version: {}", latest);
        latest
    } else {
        args.new_version
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Either --new-version or --auto is required"))?
    };
    let source_path = config.project.source.clone();

    println!("Running AST-based diff analysis...");
    if let Some(r) = &source_repo {
        println!("  Source repo: {}", r);
    }
    if let Some(f) = &from_version {
        println!("  From: {}", f);
    }
    println!("  To:   {}", new_version);

    let lang_registry = LanguageRegistry::get();
    let lang_name = lang_registry
        .detect_language(&project_root)
        .ok_or_else(|| anyhow::anyhow!("Cannot detect project language"))?;
    let language = lang_registry.get_language(&lang_name).unwrap();

    let diff_result = run_ast_diff(
        &project_root,
        source_repo.as_deref(),
        from_version.as_deref().unwrap_or("HEAD"),
        &new_version,
        source_path.as_deref(),
        language,
    )?;

    if diff_result.file_changes.is_empty() {
        println!("No differences between versions.");
        return Ok(());
    }

    println!("\nChanged files ({}):", diff_result.file_changes.len());
    for fc in &diff_result.file_changes {
        println!("  {}  {}", fc.status, fc.file);
    }

    let reverse_index = load_reverse_index(&ctx);
    let file_recs = load_file_recommendations(&report_dir);

    let all_file_changes = convert_to_output_format(&diff_result.file_changes, &file_recs);
    let all_files: Vec<String> = diff_result.file_changes.iter().map(|fc| fc.file.clone()).collect();

    let all_triggered_symbols: Vec<String> = all_file_changes
        .iter()
        .flat_map(|fc| fc.changes.iter().map(|ch| format!("{}:{}", fc.file, ch.symbol)))
        .collect();

    let propagation = propagate_changes(&all_triggered_symbols, &reverse_index);

    let summary = SummaryInfo {
        total_files_changed: diff_result.summary.total_files_changed,
        symbols_added: diff_result.summary.symbols_added,
        symbols_removed: diff_result.summary.symbols_removed,
        symbols_renamed: diff_result.summary.symbols_renamed,
        symbols_modified: diff_result.summary.symbols_modified,
        breaking_changes: diff_result.summary.breaking_changes,
    };

    let report = DiffReportOutput {
        generated_at: chrono::Utc::now().to_rfc3339(),
        source_repo,
        from_version,
        to_version: new_version,
        files: all_files,
        file_changes: all_file_changes,
        propagation,
        summary,
    };

    let diff_dir = migration_dir.join("diffs");
    std::fs::create_dir_all(&diff_dir)?;

    let timestamp = chrono::Utc::now().format("%Y-%m-%d");
    let dated_name = format!("diff-{}.json", timestamp);
    let report_path = diff_dir.join(&dated_name);
    let report_json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&report_path, &report_json)?;

    let latest_path = diff_dir.join(output_paths::diffs::LATEST.trim_start_matches("diffs/"));
    std::fs::write(&latest_path, report_json)?;

    let affected_path = diff_dir.join("affected-files.json");
    let affected_summary = serde_json::json!({
        "triggered_by": report.propagation.triggered_by,
        "affected_files": report.propagation.affected_files,
        "total_affected": report.propagation.affected_files.len(),
    });
    std::fs::write(
        &affected_path,
        serde_json::to_string_pretty(&affected_summary)?,
    )?;

    println!(
        "  Affected files: {}",
        report.propagation.affected_files.len()
    );
    println!(
        "  Breaking changes: {}",
        report.summary.breaking_changes
    );

    Ok(())
}

fn run_ast_diff(
    project_root: &Path,
    source_repo: Option<&str>,
    from_version: &str,
    to_version: &str,
    source_path: Option<&str>,
    language: &dyn migration_core::language::Language,
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

    let summary = compute_summary(&file_changes);

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
    language: &dyn migration_core::language::Language,
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
    language: &dyn migration_core::language::Language,
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

    let summary = compute_summary(&file_changes);

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

fn convert_to_output_format(
    file_changes: &[FileDiffResult],
    file_recs: &HashMap<String, Vec<DependencyRecommendation>>,
) -> Vec<FileChangeGroup> {
    let mut result = Vec::new();
    for fc in file_changes {
        let mut changes = Vec::new();
        for sc in &fc.symbol_changes {
            let mut details = Vec::new();
            for d in &sc.details {
                details.push(ChangeDetailInfo {
                    aspect: d.aspect.clone(),
                    change_type: d.change_type.clone(),
                    description: d.description.clone(),
                    old_value: d.old_value.clone(),
                    new_value: d.new_value.clone(),
                    migration_note: d.migration_note.clone(),
                });
            }
            changes.push(SymbolChangeDetail {
                symbol: sc.symbol.clone(),
                kind: sc.kind.clone(),
                change_type: sc.change_type.clone(),
                severity: sc.severity.clone(),
                old_name: sc.old_name.clone(),
                rename_confidence: sc.rename_confidence,
                details,
                old_line_range: sc.old_line_range,
                new_line_range: sc.new_line_range,
            });
        }

        let mut import_changes = Vec::new();
        for ic in &fc.import_changes {
            import_changes.push(ImportChangeDetail {
                change_type: ic.change_type.clone(),
                package: ic.package.clone(),
                old_path: ic.old_path.clone(),
                new_path: ic.new_path.clone(),
                is_external: ic.is_external,
            });
        }

        let mut doc_changes = Vec::new();
        for dc in &fc.doc_changes {
            doc_changes.push(DocChangeDetail {
                change_type: dc.change_type.clone(),
                symbol: dc.symbol.clone(),
                is_deprecated: dc.is_deprecated,
                has_todo: dc.has_todo,
                has_safety_note: dc.has_safety_note,
            });
        }

        let mut recommendations = Vec::new();
        if let Some(recs) = file_recs.get(&fc.file) {
            recommendations = recs.clone();
        }

        result.push(FileChangeGroup {
            file: fc.file.clone(),
            source_attached: true,
            changes,
            import_changes,
            doc_changes,
            recommendations,
        });
    }
    result
}

fn load_reverse_index(ctx: &ProjectContext) -> ReverseIndex {
    ctx.load_reverse_index().unwrap_or_default()
}

fn propagate_changes(
    triggered_symbols: &[String],
    reverse_index: &ReverseIndex,
) -> PropagationResult {
    let mut visited: HashSet<String> = HashSet::new();
    let mut chain: Vec<PropagationLink> = Vec::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut affected_files: HashSet<String> = HashSet::new();

    for sym in triggered_symbols {
        visited.insert(sym.clone());
        queue.push_back(sym.clone());
    }

    while let Some(current) = queue.pop_front() {
        if let Some(refs) = reverse_index.get(&current) {
            for r in refs {
                let dependent_symbol = &r.symbol;
                if !visited.contains(dependent_symbol) {
                    visited.insert(dependent_symbol.clone());
                    queue.push_back(dependent_symbol.clone());

                    if let Some(file) = dependent_symbol.rsplit_once(':').map(|x| x.0) {
                        affected_files.insert(file.to_string());
                    }

                    chain.push(PropagationLink {
                        from: current.clone(),
                        to: dependent_symbol.clone(),
                        via: r.kind.clone(),
                    });
                }
            }
        }
    }

    let mut sorted_files: Vec<String> = affected_files.into_iter().collect();
    sorted_files.sort();

    PropagationResult {
        triggered_by: triggered_symbols.to_vec(),
        affected_files: sorted_files,
        chain,
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

fn compute_summary(file_changes: &[FileDiffResult]) -> migration_core::diff::DiffSummary {
    let mut summary = migration_core::diff::DiffSummary {
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

fn is_analyzable_file(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "rs")
}

fn create_temp_dir() -> anyhow::Result<PathBuf> {
    let base = std::env::temp_dir().join("_mig_diff");
    let mut i = 0u64;
    loop {
        let dir = base.join(i.to_string());
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
            return Ok(dir);
        }
        i += 1;
    }
}

fn fetch_latest_version(repo: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["ls-remote", "--tags", repo])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git ls-remote failed for {}: {}", repo, stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    let mut tags: Vec<String> = Vec::new();
    for line in &lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let ref_str = parts[1];

        if let Some(tag) = ref_str.strip_prefix("refs/tags/") {
            if tag.ends_with("^{}") {
                continue;
            }
            tags.push(tag.to_string());
        }
    }

    tags.sort_by(|a, b| {
        let a_ver = a.trim_start_matches('v');
        let b_ver = b.trim_start_matches('v');
        let a_parts: Vec<&str> = a_ver.split('.').collect();
        let b_parts: Vec<&str> = b_ver.split('.').collect();

        for (ap, bp) in a_parts.iter().zip(b_parts.iter()) {
            match (ap.parse::<u64>(), bp.parse::<u64>()) {
                (Ok(an), Ok(bn)) if an != bn => return an.cmp(&bn),
                _ => {}
            }
        }
        a_parts.len().cmp(&b_parts.len()).then_with(|| a.cmp(b))
    });

    let latest = tags
        .last()
        .cloned()
        .or_else(|| {
            let head_output = std::process::Command::new("git")
                .args(["ls-remote", repo, "HEAD"])
                .output()
                .ok()?;
            if !head_output.status.success() {
                return None;
            }
            let stdout = String::from_utf8_lossy(&head_output.stdout);
            stdout.split_whitespace().next().map(|s| s.to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("No tags or refs found in remote {}", repo))?;

    Ok(latest)
}

fn load_file_recommendations(report_dir: &Path) -> HashMap<String, Vec<DependencyRecommendation>> {
    let rec_path = report_dir.join(output_paths::external::RECOMMENDATIONS);
    if !rec_path.exists() {
        return HashMap::new();
    }
    let content = match std::fs::read_to_string(&rec_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let report: RecommendationReport = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };

    let mut map: HashMap<String, Vec<DependencyRecommendation>> = HashMap::new();
    for dep in &report.dependencies {
        for module in &dep.affected_modules {
            map.entry(module.clone()).or_default().push(dep.clone());
        }
    }
    map
}