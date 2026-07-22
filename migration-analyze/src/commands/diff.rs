use clap::Args;
use migration_core::language::LanguageRegistry;
use migration_core::output_paths;
use migration_core::align;
use serde_json::Value;
use std::path::Path;

use crate::commands::context::ProjectContext;
use crate::commands::resolve_project_path;
use crate::progress::ProgressDisplay;

use self::git_utils::{fetch_latest_version};
use self::types::DiffReportOutput;

mod git_utils;
mod logic;
mod propagation;
mod types;

#[derive(Args)]
pub struct DiffArgs {
    #[arg(default_value = ".")]
    pub path: String,

    #[arg(long, required_unless_present = "auto", conflicts_with = "auto")]
    pub new_version: Option<String>,

    #[arg(long)]
    pub auto: bool,
}

pub fn run(args: &DiffArgs) -> anyhow::Result<()> {
    let progress = ProgressDisplay::new();

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
    let source_path = config.project.source.clone();
    let from_version = config.project.source_version.clone();
    let new_version = if args.auto {
        let spinner = progress.add_spinner("Auto-detecting latest version...");
        let repo = source_repo.as_deref().or(source_path.as_deref());
        match repo {
            Some(r) if r.contains("://") || r.starts_with("git@") => {
                spinner.set_message("Fetching latest version from remote...");
                let latest = fetch_latest_version(r)?;
                spinner.finish_with_message(format!("Auto-detected: {}", latest));
                latest
            }
            Some(r) => {
                spinner.set_message("Searching local tags...");
                let candidate = if Path::new(r).is_absolute() {
                    std::path::PathBuf::from(r)
                } else {
                    project_root.join(r)
                };
                let latest = crate::commands::run_git_cmd(
                    &candidate,
                    &["tag", "--sort=-version:refname"],
                )
                .and_then(|tags| {
                    tags.lines().next().map(|t| t.to_string())
                })
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No tags found in local repo {}. Specify --new-version manually.",
                        candidate.display()
                    )
                })?;
                spinner.finish_with_message(format!("Auto-detected: {}", latest));
                latest
            }
            None => anyhow::bail!(
                "Either source_repo or source must be configured in migration.toml for --auto"
            ),
        }
    } else {
        args.new_version
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Either --new-version or --auto is required"))?
    };

    let diff_spinner = progress.add_spinner("Running AST-based diff analysis...");
    if let Some(r) = &source_repo {
        diff_spinner.set_message(format!("Source repo: {}", r));
    }
    if let Some(f) = &from_version {
        diff_spinner.set_message(format!("From: {}", f));
    }
    diff_spinner.set_message(format!("To: {}", new_version));

    let lang_registry = LanguageRegistry::get();
    let lang_name = logic::detect_project_language(lang_registry, &project_root, source_path.as_deref())
        .ok_or_else(|| anyhow::anyhow!("Cannot detect project language"))?;
    let language = lang_registry.get_language(&lang_name).unwrap();

    let mut diff_result = logic::run_ast_diff(
        &project_root,
        source_repo.as_deref(),
        from_version.as_deref().unwrap_or("HEAD"),
        &new_version,
        source_path.as_deref(),
        language,
    )?;
    diff_spinner.finish_with_message("AST diff analysis complete");

    // Resolve target-side symbols via alignment (real-time, no DB needed)
    let target_path = config.project.target.as_deref().map(Path::new);
    align::resolve_all(
        &mut diff_result.file_changes,
        target_path,
        "typescript",
        "rust",
    );

    if diff_result.file_changes.is_empty() {
        println!("No differences between versions.");
        return Ok(());
    }

    let file_count = diff_result.file_changes.len();
    let changed_bar = progress.add_bar(file_count as u64, "Changed files");
    for fc in &diff_result.file_changes {
        changed_bar.set_message(format!("{}  {}", fc.status, fc.file));
        changed_bar.inc(1);
    }
    changed_bar.finish_and_clear();
    println!("  Changed files: {}", file_count);

    let propagation_spinner = progress.add_spinner("Running propagation analysis...");
    let reverse_index = propagation::load_reverse_index(&ctx);
    let file_recs = propagation::load_file_recommendations(&report_dir);

    let all_file_changes = propagation::convert_to_output_format(&diff_result.file_changes, &file_recs);
    let all_files: Vec<String> = diff_result.file_changes.iter().map(|fc| fc.file.clone()).collect();

    // Deduplicate triggered_symbols to avoid redundant propagation entries
    let all_triggered_symbols_raw: Vec<String> = all_file_changes
        .iter()
        .flat_map(|fc| fc.changes.iter().map(|ch| format!("{}:{}", fc.file, ch.symbol)))
        .collect();
    let mut seen = std::collections::HashSet::new();
    let all_triggered_symbols: Vec<String> = all_triggered_symbols_raw
        .into_iter()
        .filter(|k| seen.insert(k.clone()))
        .collect();

    let propagation_result = propagation::propagate_changes(&all_triggered_symbols, &reverse_index);
    propagation_spinner.finish_with_message("Propagation analysis complete");

    let summary = diff_result.summary;

    let report = DiffReportOutput {
        generated_at: chrono::Utc::now().to_rfc3339(),
        source_repo,
        from_version,
        to_version: new_version,
        files: all_files,
        file_changes: all_file_changes,
        propagation: propagation_result,
        summary: types::SummaryInfo {
            total_files_changed: summary.total_files_changed,
            symbols_added: summary.symbols_added,
            symbols_removed: summary.symbols_removed,
            symbols_renamed: summary.symbols_renamed,
            symbols_modified: summary.symbols_modified,
            breaking_changes: summary.breaking_changes,
        },
    };

    let write_spinner = progress.add_spinner("Writing diff report...");
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
    let affected_summary = Value::Object({
        let mut m = serde_json::Map::new();
        m.insert("triggered_by".to_string(), Value::Array(report.propagation.triggered_by.iter().map(|s| Value::String(s.clone())).collect()));
        m.insert("affected_files".to_string(), Value::Array(report.propagation.affected_files.iter().map(|s| Value::String(s.clone())).collect()));
        m.insert("total_affected".to_string(), Value::Number(report.propagation.affected_files.len().into()));
        m
    });
    std::fs::write(
        &affected_path,
        serde_json::to_string_pretty(&affected_summary)?,
    )?;
    write_spinner.finish_with_message("Diff report written");

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
