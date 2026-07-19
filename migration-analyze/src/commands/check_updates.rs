use clap::Args;
use migration_core::compatibility::CompatibilityMatrix;
use migration_core::deps::ResolvedDependency;
use migration_core::output_paths;
use migration_core::project::SourceLanguage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::process::Command;

use crate::commands::context::ProjectContext;
use crate::commands::resolve_project_path;

#[derive(Args)]
pub struct CheckUpdatesArgs {
    /// Project root directory (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: String,

    /// Fetch from remote before checking updates
    #[arg(long)]
    pub fetch: bool,

    /// Output format: text or json
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateReport {
    generated_at: String,
    analyzed_commit: Option<String>,
    head_commit: Option<String>,
    has_changes: bool,
    changed_files: Vec<FileChangeSummary>,
    dep_changes: Vec<migration_core::compatibility::DepChangeInfo>,
    summary: UpdateSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateSummary {
    total_changed_files: usize,
    source_files_changed: usize,
    dep_files_changed: usize,
    deps_added: usize,
    deps_removed: usize,
    deps_modified: usize,
    deps_needing_review: usize,
    diff_stat: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileChangeSummary {
    file: String,
    status: String,
    additions: usize,
    deletions: usize,
    is_dep_file: bool,
}

pub fn run(args: &CheckUpdatesArgs) -> anyhow::Result<()> {
    let project_root = resolve_project_path(&args.path);
    let ctx = ProjectContext::load(&project_root)?;

    if !ctx.report_dir.exists() {
        anyhow::bail!(
            "Report dir not found at {}. Run 'migration-analyze analyze' first.",
            ctx.report_dir.display()
        );
    }

    // ── 1. Read manifest to get analyzed commit ──────────────────────────
    let manifest: serde_json::Value = ctx.load_json(output_paths::MANIFEST)?;
    let analyzed_commit = manifest["sourceRepo"]["analyzedCommit"]
        .as_str()
        .map(|s| s.to_string());

    let analyzed_commit = analyzed_commit.ok_or_else(|| {
        anyhow::anyhow!(
            "No analyzedCommit found in manifest. Re-run 'migration-analyze analyze'."
        )
    })?;

    // ── 2. Locate the source repo ────────────────────────────────────────
    let config = &ctx.config;
    let source_path = config
        .project
        .source
        .as_deref()
        .map(|s| resolve_source_path(s, &project_root));

    let source_dir = match source_path {
        Some(ref p) if is_git_repo(p) => p.clone(),
        _ => {
            anyhow::bail!(
                "Source repository not found at configured path. \
                 Set [project].source in migration.toml."
            );
        }
    };

    // ── 3. Optionally fetch remote ───────────────────────────────────────
    if args.fetch {
        println!("  Fetching latest from remote...");
        let fetch_result = Command::new("git")
            .args(["fetch", "origin"])
            .current_dir(&source_dir)
            .output();
        match fetch_result {
            Ok(out) if out.status.success() => {
                println!("    Fetch complete.");
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                eprintln!("    Warning: git fetch failed: {}", stderr);
            }
            Err(e) => {
                eprintln!("    Warning: could not fetch: {}", e);
            }
        }
    }

    // ── 4. Get HEAD commit hash ──────────────────────────────────────────
    let head_commit = run_git_cmd(&source_dir, &["rev-parse", "--short", "HEAD"]);

    // ── 5. Check if analyzed commit matches HEAD ─────────────────────────
    if head_commit.as_deref() == Some(&analyzed_commit) {
        let commit_clone = analyzed_commit.clone();
        let head_clone = head_commit.clone();
        if args.format == "json" {
            let report = UpdateReport {
                generated_at: chrono::Utc::now().to_rfc3339(),
                analyzed_commit: Some(analyzed_commit),
                head_commit,
                has_changes: false,
                changed_files: vec![],
                dep_changes: vec![],
                summary: UpdateSummary {
                    total_changed_files: 0,
                    source_files_changed: 0,
                    dep_files_changed: 0,
                    deps_added: 0,
                    deps_removed: 0,
                    deps_modified: 0,
                    deps_needing_review: 0,
                    diff_stat: String::new(),
                },
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!();
            println!("  No changes since analyzed commit {}.", analyzed_commit);
            println!("  Migration is up-to-date.");
            println!();
        }

        // Write empty report to updates/
        let report_dir = &ctx.report_dir;
        let updates_dir = report_dir.join("updates");
        std::fs::create_dir_all(&updates_dir)?;
        std::fs::write(
            updates_dir.join("diff_overview.json"),
            serde_json::to_string_pretty(&json!({
                "analyzedCommit": commit_clone,
                "headCommit": head_clone,
                "hasChanges": false,
                "checkedAt": chrono::Utc::now().to_rfc3339(),
            }))?,
        )?;

        return Ok(());
    }

    // ── 6. Get diff stat ─────────────────────────────────────────────────
    let diff_stat = run_git_cmd(
        &source_dir,
        &["diff", "--shortstat", &analyzed_commit, "HEAD"],
    )
    .unwrap_or_default();

    // ── 7. Get file-level diff ───────────────────────────────────────────
    let changed_files_raw = run_git_cmd(
        &source_dir,
        &["diff", "--name-status", &analyzed_commit, "HEAD"],
    )
    .unwrap_or_default();

    // Parse file changes and classify
    let dep_file_patterns = [
        "package.json",
        "package-lock.json",
        "npm-shrinkwrap.json",
        "yarn.lock",
        "Cargo.toml",
        "Cargo.lock",
        "pnpm-lock.yaml",
        "bun.lock",
    ];

    let mut changed_files: Vec<FileChangeSummary> = Vec::new();

    for line in changed_files_raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() < 2 {
            continue;
        }
        let status = parts[0];
        let file = parts[1];

        let is_dep = dep_file_patterns
            .iter()
            .any(|p| file == *p || file.ends_with(&format!("/{}", p)));

        // Count additions/deletions per file from detailed diff
        let file_diff = run_git_cmd(
            &source_dir,
            &["diff", "--numstat", &analyzed_commit, "HEAD", "--", file],
        )
        .unwrap_or_default();

        let (additions, deletions) = parse_numstat(&file_diff);

        changed_files.push(FileChangeSummary {
            file: file.to_string(),
            status: normalize_status(status),
            additions,
            deletions,
            is_dep_file: is_dep,
        });
    }

    let source_files_changed = changed_files
        .iter()
        .filter(|f| !f.is_dep_file)
        .count();
    let dep_files_changed = changed_files.iter().filter(|f| f.is_dep_file).count();

    // ── 8. Re-resolve dependencies from latest source ────────────────────
    let (dep_changes, _old_deps, _new_deps) = if dep_files_changed > 0 {
        detect_dependency_changes(&source_dir, &project_root, &ctx)?
    } else {
        (vec![], vec![], vec![])
    };

    let deps_added = dep_changes.iter().filter(|d| d.change_type == "added").count();
    let deps_removed = dep_changes
        .iter()
        .filter(|d| d.change_type == "removed")
        .count();
    let deps_modified = dep_changes
        .iter()
        .filter(|d| d.change_type == "upgraded" || d.change_type == "downgraded")
        .count();
    let deps_needing_review = dep_changes.iter().filter(|d| d.needs_review).count();

    let summary = UpdateSummary {
        total_changed_files: changed_files.len(),
        source_files_changed,
        dep_files_changed,
        deps_added,
        deps_removed,
        deps_modified,
        deps_needing_review,
        diff_stat,
    };

    // ── 9. Build report ─────────────────────────────────────────────────
    let report = UpdateReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        analyzed_commit: Some(analyzed_commit.clone()),
        head_commit,
        has_changes: true,
        changed_files,
        dep_changes,
        summary,
    };

    // ── 10. Write reports to updates/ dir ────────────────────────────────
    let report_dir = ctx.report_dir.clone();
    let updates_dir = report_dir.join("updates");
    std::fs::create_dir_all(&updates_dir)?;

    std::fs::write(
        updates_dir.join("diff_overview.json"),
        serde_json::to_string_pretty(&json!({
            "analyzedCommit": analyzed_commit,
            "headCommit": report.head_commit,
            "hasChanges": true,
            "changedFiles": report.changed_files,
            "summary": report.summary,
            "checkedAt": chrono::Utc::now().to_rfc3339(),
        }))?,
    )?;

    std::fs::write(
        updates_dir.join("changed_files.json"),
        serde_json::to_string_pretty(&report.changed_files)?,
    )?;

    std::fs::write(
        updates_dir.join("dep_changes.json"),
        serde_json::to_string_pretty(&report.dep_changes)?,
    )?;

    // ── 11. Print output ────────────────────────────────────────────────
    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => {
            print_text_report(&report, &source_dir);
        }
    }

    Ok(())
}

/// ── Helpers ──────────────────────────────────────────────────────────────────

fn print_text_report(report: &UpdateReport, source_dir: &Path) {
    use console::style;

    println!();
    println!("{}", style("━━━ Update Check Report ━━━").bold().cyan());
    println!();

    println!(
        "  {} {}",
        style("Analyzed commit:").bold(),
        report
            .analyzed_commit
            .as_deref()
            .unwrap_or("unknown")
    );
    println!(
        "  {} {}",
        style("HEAD commit:").bold(),
        report.head_commit.as_deref().unwrap_or("unknown")
    );
    println!(
        "  {} {}",
        style("Source repo:").bold(),
        source_dir.display()
    );

    println!();
    println!(
        "  {} {}",
        style("Changes detected:").bold(),
        if report.has_changes {
            style("YES").red().bold()
        } else {
            style("NO").green()
        }
    );

    if !report.has_changes {
        println!();
        println!("  Migration is up-to-date with the source repository.");
        return;
    }

    println!(
        "  {} {}",
        style("Changed files:").bold(),
        report.summary.total_changed_files
    );
    println!(
        "    {} source files, {} dependency files",
        style(report.summary.source_files_changed).yellow(),
        style(report.summary.dep_files_changed).yellow()
    );

    // Show changed files table
    println!();
    println!("  {}", style("File changes:").bold());
    for fc in &report.changed_files {
        let status_style = match fc.status.as_str() {
            "A" => style("A").green(),
            "D" => style("D").red(),
            "M" => style("M").yellow(),
            _ => style("?").dim(),
        };
        let dep_tag = if fc.is_dep_file {
            style(" [dep]").dim()
        } else {
            style("").dim()
        };
        println!(
            "    {} {:40} +{:<4} -{:<4}{}",
            status_style,
            fc.file,
            fc.additions,
            fc.deletions,
            dep_tag
        );
    }

    // Show dep changes
    if !report.dep_changes.is_empty() {
        println!();
        println!("  {}", style("Dependency changes:").bold());
        for dc in &report.dep_changes {
            let change_style = match dc.change_type.as_str() {
                "added" => style("+ added").green(),
                "removed" => style("- removed").red(),
                "upgraded" => style("~ upgraded").yellow(),
                "downgraded" => style("~ downgraded").red(),
                _ => style("? unknown").dim(),
            };

            let version_info = match (&dc.old_version, &dc.new_version) {
                (Some(old), Some(new)) => format!("{} → {}", old, new),
                (Some(old), None) => old.clone(),
                (None, Some(new)) => new.clone(),
                _ => String::new(),
            };

            let compat_info = match (&dc.compatibility_now, &dc.equivalent) {
                (Some(c), Some(eq)) => format!(" (compat: {}, → {})", c, eq),
                (Some(c), None) => format!(" (compat: {})", c),
                _ => String::new(),
            };

            let review_tag = if dc.needs_review {
                style(" ⚠ needs review").red().to_string()
            } else {
                String::new()
            };

            println!(
                "    {} {:30} {}{}{}",
                change_style,
                dc.package,
                version_info,
                compat_info,
                review_tag
            );
        }

        if report.summary.deps_needing_review > 0 {
            println!();
            println!(
                "  {}",
                style(format!(
                    "  {} dependencies need manual review.",
                    report.summary.deps_needing_review
                ))
                .red()
                .bold()
            );
        }
    }

    println!();
    println!(
        "  {}",
        style(format!(
            "Details: {}",
            report
                .head_commit
                .as_deref()
                .unwrap_or("unknown")
        ))
        .dim()
    );
    println!();
}

fn parse_numstat(output: &str) -> (usize, usize) {
    let line = output.lines().next().unwrap_or("");
    let parts: Vec<&str> = line.splitn(3, '\t').collect();
    if parts.len() < 2 {
        return (0, 0);
    }
    let additions = parts[0].parse::<usize>().unwrap_or(0);
    let deletions = parts[1].parse::<usize>().unwrap_or(0);
    (additions, deletions)
}

fn normalize_status(s: &str) -> String {
    match s.chars().next() {
        Some('A') => "A".to_string(),
        Some('M') => "M".to_string(),
        Some('D') => "D".to_string(),
        Some('R') => "M".to_string(), // rename treated as modify
        Some('C') => "A".to_string(), // copy treated as add
        _ => s.chars().next().map(|c| c.to_string()).unwrap_or_default(),
    }
}

fn is_git_repo(path: &Path) -> bool {
    path.join(".git").exists() || path.join("HEAD").exists()
}

fn run_git_cmd(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn resolve_source_path(src: &str, project_root: &Path) -> std::path::PathBuf {
    let p = std::path::Path::new(src);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_root.join(p)
    }
}

/// Detect dependency changes by comparing old (stored) and new (re-resolved) deps.
fn detect_dependency_changes(
    source_dir: &Path,
    _project_root: &Path,
    ctx: &ProjectContext,
) -> anyhow::Result<(
    Vec<migration_core::compatibility::DepChangeInfo>,
    Vec<ResolvedDependency>,
    Vec<ResolvedDependency>,
)> {
    // Load old dependencies from stored report
    let old_deps: Vec<ResolvedDependency> = match ctx
        .load_json::<serde_json::Value>(output_paths::external::PACKAGES)
    {
        Ok(val) => {
            // The stored packages may be a JSON object with "packages" key or an array
            if let Some(arr) = val.as_array() {
                serde_json::from_value(serde_json::Value::Array(arr.clone()))
                    .unwrap_or_default()
            } else if let Some(packages) = val.get("packages").and_then(|v| v.as_array()) {
                serde_json::from_value(serde_json::Value::Array(packages.clone()))
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    };

    // Re-resolve dependencies from the latest source
    let new_deps = match ctx.config.project.source_lang.as_deref() {
        Some("typescript") | Some("ts") => {
            migration_core::deps::resolve_dependencies(source_dir, SourceLanguage::TypeScript)
                .unwrap_or_default()
        }
        Some("rust") | Some("rs") => {
            migration_core::deps::resolve_dependencies(source_dir, SourceLanguage::Rust)
                .unwrap_or_default()
        }
        _ => {
            eprintln!("  Warning: unknown source language, skipping dependency analysis.");
            Vec::new()
        }
    };

    // Build a compatibility matrix to detect changes
    let target_lang = ctx.config.project.target_lang.clone();
    let source_lang = ctx.config.project.source_lang.clone().unwrap_or_default();
    let compat = CompatibilityMatrix::new(source_lang, target_lang);
    let dep_changes = compat.detect_dep_changes(&old_deps, &new_deps);

    Ok((dep_changes, old_deps, new_deps))
}
