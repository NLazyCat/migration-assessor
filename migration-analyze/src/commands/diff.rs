use clap::Args;
use migration_core::language::LanguageRegistry;
use migration_core::output_paths;
use serde_json::Value;

use crate::commands::context::ProjectContext;
use crate::commands::resolve_project_path;

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
    let lang_name = logic::detect_project_language(lang_registry, &project_root, source_path.as_deref())
        .ok_or_else(|| anyhow::anyhow!("Cannot detect project language"))?;
    let language = lang_registry.get_language(&lang_name).unwrap();

    let diff_result = logic::run_ast_diff(
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

    let reverse_index = propagation::load_reverse_index(&ctx);
    let file_recs = propagation::load_file_recommendations(&report_dir);

    let all_file_changes = propagation::convert_to_output_format(&diff_result.file_changes, &file_recs);
    let all_files: Vec<String> = diff_result.file_changes.iter().map(|fc| fc.file.clone()).collect();

    let all_triggered_symbols: Vec<String> = all_file_changes
        .iter()
        .flat_map(|fc| fc.changes.iter().map(|ch| format!("{}:{}", fc.file, ch.symbol)))
        .collect();

    let propagation_result = propagation::propagate_changes(&all_triggered_symbols, &reverse_index);

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
