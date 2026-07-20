use clap::Args;
use migration_core::diff::engine::DiffEngine;
use migration_core::verify::verify_migration;
use std::fs;
use std::path::Path;

use crate::commands::context::ProjectContext;
use crate::commands::resolve_project_path;

#[derive(Args)]
pub struct VerifyArgs {
    #[arg(default_value = ".")]
    pub path: String,

    /// New source version to verify migration against (tag or commit hash)
    #[arg(long, required = true)]
    pub new_version: String,

    /// Coverage threshold (0.0-1.0) for passing verification
    #[arg(long, default_value = "0.9")]
    pub threshold: f64,
}

pub fn run(args: &VerifyArgs) -> anyhow::Result<()> {
    let project_root = resolve_project_path(&args.path);
    let config_path = project_root.join("migration.toml");

    if !config_path.exists() {
        anyhow::bail!(
            "No migration.toml found in {}.\n\
             Run 'migration-analyze analyze' first.",
            project_root.display()
        );
    }

    let ctx = ProjectContext::load(&project_root)?;
    let config = &ctx.config;

    let source_version = config
        .project
        .source_version
        .as_deref()
        .unwrap_or("HEAD");
    let target_path = config.project.target.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "No target project configured in migration.toml.\n\
             Add 'target = \"../your-rust-project\"' under [project]."
        )
    })?;

    let source_lang = config
        .project
        .source_lang
        .as_deref()
        .unwrap_or("typescript");
    let target_lang = config.project.target_lang.as_str();

    let target_root = if Path::new(target_path).is_absolute() {
        Path::new(target_path).to_path_buf()
    } else {
        project_root.join(target_path)
    };

    if !target_root.exists() {
        anyhow::bail!(
            "Target project not found at {}.\n\
             Check the 'target' path in migration.toml.",
            target_root.display()
        );
    }

    println!("Verifying migration alignment...");
    println!("  Source version: {} → {}", source_version, args.new_version);
    println!("  Target project: {}", target_root.display());
    println!("  Threshold: {:.0}%", args.threshold * 100.0);

    // Run source diff to get changed files between versions
    let source_diff_report = DiffEngine::diff_project(
        &project_root,
        source_version,
        &args.new_version,
    )?;

    if source_diff_report.file_changes.is_empty() {
        println!("No changes detected in source between versions.");
        return Ok(());
    }

    // Verify against target uncommitted changes
    let result = verify_migration(
        &source_diff_report.file_changes,
        &target_root,
        source_lang,
        target_lang,
        args.threshold,
    )?;

    println!("\n  Source files changed: {}", result.source_files);
    println!("  Target files changed: {}", result.target_files);
    println!("  Coverage: {}", result.summary_line());

    if !result.unmatched.is_empty() {
        println!("\n  Unmatched symbols ({}):", result.unmatched.len());
        for u in &result.unmatched {
            println!("    {}::{} ({})", u.file, u.symbol, u.change_type);
        }
    }

    if result.passed {
        let config_path = project_root.join("migration.toml");
        let content = fs::read_to_string(&config_path)?;
        let old_version = config.project.source_version.as_deref().unwrap_or("");
        let updated = if content.contains("source_version") {
            content.replace(
                &format!("source_version = {:?}", old_version),
                &format!("source_version = {:?}", args.new_version),
            )
        } else {
            format!("{}\nsource_version = {:?}\n", content.trim_end(), args.new_version)
        };
        fs::write(&config_path, updated)?;
        println!("\n  ✓ Verification passed — source_version updated to {}", args.new_version);
    } else {
        println!(
            "\n  ✗ Verification failed — coverage below {:.0}% threshold.",
            args.threshold * 100.0
        );
    }

    Ok(())
}
