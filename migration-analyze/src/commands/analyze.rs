use clap::Args;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use migration_core::output_paths;
use migration_core::*;
use rayon::join;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::commands::resolve_project_path;

#[derive(Args)]
pub struct AnalyzeArgs {
    /// Project root directory (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: String,

    /// Override report directory (default: {repo}-migration/report/)
    #[arg(long)]
    pub output: Option<String>,

    /// Fail on first error instead of collecting all errors
    #[arg(long)]
    pub strict: bool,

    /// Override score weights: in_degree,complexity,compatibility,cycles,tests
    #[arg(long)]
    pub score_weights: Option<String>,

    /// Output format for array-shaped artifacts
    #[arg(long, default_value = "json")]
    pub format: String,
}

pub fn run(args: &AnalyzeArgs) -> anyhow::Result<()> {
    let project_root = resolve_project_path(&args.path);

    // Load config if exists
    let config_path = project_root.join("migration.toml");
    let mut config = if config_path.exists() {
        migration_core::config::Config::load(&config_path)?
    } else {
        eprintln!("[info] No migration.toml found, using defaults");
        migration_core::config::Config::default()
    };

    // CLI overrides
    if args.strict {
        config.project.strict = true;
    }
    if let Some(weights_str) = &args.score_weights {
        let parts: Vec<&str> = weights_str.split(',').collect();
        if parts.len() != 5 {
            anyhow::bail!(
                "--score-weights requires 5 comma-separated values: in_degree,complexity,compatibility,cycles,tests"
            );
        }
        config.scoring.weights.in_degree = parts[0].parse()?;
        config.scoring.weights.complexity = parts[1].parse()?;
        config.scoring.weights.compatibility = parts[2].parse()?;
        config.scoring.weights.cycles = parts[3].parse()?;
        config.scoring.weights.tests = parts[4].parse()?;
    }

    // Auto-detect source repo (respects config.project.source if set)
    let source_repo_dir = detect_source_repo(&project_root, &config)?;
    let source_repo_name = source_repo_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid source repo directory name"))?;

    println!();
    println!("{}", style("━━━ Migration Analysis ━━━").bold().cyan());
    println!();
    println!(
        "  {} {} ({})",
        style("Source repo:").bold(),
        source_repo_name,
        source_repo_dir.display()
    );

    let source_language = config.project.source_lang.clone().unwrap_or_default();
    let target_language = &config.project.target_lang;

    // Detect project
    let project = Project::detect(
        &source_repo_dir,
        target_language.clone(),
        Some(source_language.clone()),
    )?;

    // Discover files
    let pb = ProgressBar::new(6);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{prefix:.bold} {wide_bar:.cyan/blue} {pos}/{len} {msg:.dim}")?,
    );
    pb.set_prefix("Analyzing");

    let discovery = discovery::FileDiscovery::new(
        project.source_language,
        config.project.ignore.clone(),
        config.project.exclude.clone(),
        config.skip.framework,
    );
    let files = discovery.discover(&project.root);
    pb.inc(1);
    pb.set_message("discovering files");

    // Resolve dependencies
    let dependencies = deps::resolve_dependencies(&project.root, project.source_language)?;
    pb.inc(1);
    pb.set_message("resolving dependencies");

    // Compatibility matrix
    let mut compatibility = compatibility::CompatibilityMatrix::new(
        project.source_language_str().to_string(),
        project.target_language.clone(),
    );
    if let Some(overrides_file) = &config.compatibility.overrides_file {
        compatibility.load_overrides(&project.root.join(overrides_file))?;
    }
    let compatibility_matrix = compatibility.evaluate(&dependencies);
    pb.inc(1);
    pb.set_message("evaluating compatibility");

    // Dependency graph
    let mut dependency_graph =
        graph::GraphBuilder::build(&project.root, &files, project.source_language)?;
    let cycle_detection = dependency_graph.detect_cycles();
    pb.inc(1);
    pb.set_message("building dependency graph");

    // Symbol extraction and reference extraction are independent CPU-bound stages;
    // run them in parallel.
    let (symbol_results, refs_result) = join(
        || symbols::SymbolExtractor::extract_all(&project.root, &files, project.source_language),
        || match project.source_language {
            project::SourceLanguage::TypeScript => {
                references::typescript::extract_all(&project.root, &files)
            }
            project::SourceLanguage::Rust => references::rust::extract_all(&project.root, &files),
        },
    );
    let symbol_results = symbol_results?;
    let (forward, reverse) = refs_result?;
    pb.inc(1);
    pb.set_message("extracting symbols & references");

    // ── Create migration folder ─────────────────────────────────────────
    let migration_dir_name = format!("{}-migration", source_repo_name);
    let migration_root = project_root.join(&migration_dir_name);
    let report_dir = migration_root.join("report");

    // Create mirrored directory structure in migration folder
    mirror_source_structure(&files, &project.root, &migration_root)?;

    // Write migration config
    let config_dir = migration_root.join("config");
    std::fs::create_dir_all(&config_dir)?;
    let mig_config = generate_migration_config(&project, &config);
    std::fs::write(config_dir.join("migration.toml"), mig_config)?;

    // Update root migration.toml with detected source path and git info
    let (remote, branch, version) = detect_source_git_info(&source_repo_dir);
    // Use the original relative path (avoids Windows backslash / extended-length issues)
    let source_display = args.path.replace('\\', "/");
    let root_config_content = format!(
        r#"# Migration Assessor Configuration
[project]
source = "{}"
source_repo = "{}"
source_branch = "{}"
source_version = "{}"
target_language = "{}"

[skip]
framework = {}
"#,
        source_display,
        remote.as_deref().unwrap_or(""),
        branch.as_deref().unwrap_or(""),
        version.as_deref().unwrap_or(""),
        project.target_language,
        config.skip.framework,
    );
    std::fs::write(&config_path, root_config_content)?;

    // ── Output report ──────────────────────────────────────────────────
    let output_format = output::OutputFormat::from_cli(&args.format)?;
    let output = output::OutputWriter::init(&report_dir, output_format)?;

    let chrono = chrono::Utc::now();
    use serde_json::json;

    let project_meta = json!({
        "schemaVersion": "1.0.0",
        "generatedAt": chrono.to_rfc3339(),
        "sourceLanguage": project.source_language_str(),
        "targetLanguage": project.target_language,
        "sourceRoot": project.root.to_string_lossy(),
        "sourceRepo": source_repo_name,
        "filesAnalyzed": files.len(),
        "dependencyCount": dependencies.len(),
        "partialAnalysisCount": 0
    });

    output.write(&report_dir, output_paths::PROJECT, &project_meta)?;
    output.write(&report_dir, output_paths::ERRORS, &json!([]))?;
    output.write(
        &report_dir,
        output_paths::external::PACKAGES,
        &json!({ "packages": dependencies }),
    )?;
    output.write(
        &report_dir,
        output_paths::external::COMPATIBILITY,
        &compatibility_matrix,
    )?;
    output.write(
        &report_dir,
        output_paths::graph::NODES,
        &dependency_graph.nodes,
    )?;
    output.write(
        &report_dir,
        output_paths::graph::EDGES,
        &dependency_graph.edges,
    )?;
    output.write(&report_dir, output_paths::graph::CYCLES, &cycle_detection)?;

    let mut file_index = serde_json::Map::new();
    for (index, contract) in &symbol_results {
        let symbols_path = output_paths::symbols::for_module(&index.module);
        let contracts_path = output_paths::symbols::api_for_module(&contract.module);
        output.write(&report_dir, &symbols_path, &index)?;
        output.write(&report_dir, &contracts_path, &contract)?;

        file_index.insert(
            index.module.clone(),
            json!({
                "symbol_count": index.symbols.len(),
                "symbols_path": symbols_path,
                "contracts_path": contracts_path,
            }),
        );
    }
    output.write(&report_dir, output_paths::OVERVIEW, &json!(file_index))?;

    // Per-file references
    let file_refs = group_references_by_file(&forward, &reverse);
    for (file, refs) in &file_refs {
        let fwd_path = output_paths::references::forward_for(file);
        let rev_path = output_paths::references::reverse_for(file);
        output.write(&report_dir, &fwd_path, &refs.forward)?;
        output.write(&report_dir, &rev_path, &refs.reverse)?;
    }

    println!(
        "  References extracted: {} forward, {} reverse",
        forward.len(),
        reverse.len()
    );

    // Migration readiness scores
    let readiness = scores::calculate(
        &project.root,
        &files,
        &symbol_results,
        &reverse,
        &compatibility_matrix,
        &cycle_detection,
    )?;

    output.write(&report_dir, output_paths::SCORES, &readiness)?;
    pb.inc(1);
    pb.set_message("calculating scores");
    pb.finish_and_clear();

    let manifest = json!({
        "$schema": "https://migration-analyze.dev/schema/v1/manifest.json",
        "schemaVersion": "1.0.0",
        "generatedAt": chrono.to_rfc3339(),
        "toolVersion": env!("CARGO_PKG_VERSION"),
        "sourceRepo": {
            "analyzedCommit": version,
            "analyzedAt": chrono.to_rfc3339(),
        },
        "files": {
            "project": output_paths::PROJECT,
            "overview": output_paths::OVERVIEW,
            "scores": output_paths::SCORES,
            "errors": output_paths::ERRORS,
            "externalPackages": output_paths::external::PACKAGES,
            "externalCompatibility": output_paths::external::COMPATIBILITY,
            "graphNodes": output_paths::graph::NODES,
            "graphEdges": output_paths::graph::EDGES,
            "graphCycles": output_paths::graph::CYCLES,
        }
    });
    output.write(&report_dir, output_paths::MANIFEST, &manifest)?;

    println!();
    println!(
        "  {} {} files",
        style("Migration scores:").bold(),
        readiness.len()
    );
    if let Some(top) = readiness.first() {
        println!(
            "    {} {} ({})",
            style("Top priority:").bold(),
            style(&top.module).yellow(),
            style(format!("score: {}", top.score)).green()
        );
    }

    for entry in &readiness {
        println!(
            "  #{:2} {:30} score: {:6.2}  (in-degree: {:2}, cycle: {:1})",
            entry.rank, entry.module, entry.score, entry.in_degree, entry.cycle_count,
        );
    }

    // Generate HTML report
    crate::commands::report::generate_html_report(
        &report_dir,
        &project_meta,
        &dependencies,
        &dependency_graph,
        &cycle_detection,
    )?;

    println!();
    println!(
        "  {} {}",
        style("Report generated:").bold().green(),
        style(format!("{}/report/index.html", migration_dir_name)).underlined()
    );
    println!(
        "  {}",
        style("Run 'migration-analyze summary' to view results in terminal.").dim()
    );

    Ok(())
}

/// Detect the source repository directory inside the project root.
/// Scans immediate subdirectories for .git, package.json, Cargo.toml, or tsconfig.json.
/// If config.project.source is set, uses that path directly.
fn detect_source_repo(
    project_root: &Path,
    config: &migration_core::config::Config,
) -> anyhow::Result<PathBuf> {
    // If config specifies a source, use it directly
    if let Some(source) = &config.project.source {
        let source_path = project_root.join(source);
        if source_path.exists() && is_repo_root(&source_path) {
            return Ok(source_path);
        }
        anyhow::bail!(
            "Config specifies project.source = '{}' but '{}' is not a valid repo root (missing .git, package.json, etc.).",
            source,
            source_path.display()
        );
    }
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            // Skip hidden dirs and migration dirs
            if name.starts_with('.') || name.ends_with("-migration") {
                continue;
            }
            if is_repo_root(&path) {
                candidates.push(path);
            }
        }
    }

    match candidates.len() {
        0 => {
            anyhow::bail!(
                "No source repository found in {}. Clone a git repo first:\n  cd {} && git clone <repo-url>",
                project_root.display(),
                project_root.display()
            );
        }
        1 => Ok(candidates.remove(0)),
        _ => {
            anyhow::bail!(
                "Multiple source repositories found: {}. Please specify one in migration.toml [project].source",
                candidates
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
}

fn is_repo_root(path: &Path) -> bool {
    // Check for .git
    if path.join(".git").exists() || path.join("HEAD").exists() {
        return true;
    }
    // Check for project files
    if path.join("package.json").exists()
        || path.join("Cargo.toml").exists()
        || path.join("tsconfig.json").exists()
    {
        return true;
    }
    false
}

/// Mirror the source repo's directory structure into the migration folder.
/// Creates empty placeholder files matching source file paths.
fn mirror_source_structure(
    files: &[PathBuf],
    source_root: &Path,
    migration_root: &Path,
) -> anyhow::Result<()> {
    for file in files {
        let relative = file.strip_prefix(source_root).unwrap_or(file);
        let target = migration_root.join(relative);

        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Create empty placeholder file
        std::fs::write(&target, "")?;
    }

    println!("  Mirrored {} files into migration folder", files.len());
    Ok(())
}

/// Auto-detect git info from the source repo.
fn detect_source_git_info(source_root: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let has_git = source_root.join(".git").exists() || source_root.join("HEAD").exists();
    if !has_git {
        return (None, None, None);
    }

    let remote = run_git_cmd(source_root, &["remote", "get-url", "origin"]);
    let branch = run_git_cmd(source_root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let version = run_git_cmd(
        source_root,
        &["describe", "--tags", "--exact-match", "HEAD"],
    )
    .or_else(|| run_git_cmd(source_root, &["rev-parse", "--short", "HEAD"]));

    (remote, branch, version)
}

struct PerFileRefs {
    forward: serde_json::Value,
    reverse: serde_json::Value,
}

fn group_references_by_file(
    forward: &migration_core::references::ForwardIndex,
    reverse: &migration_core::references::ReverseIndex,
) -> HashMap<String, PerFileRefs> {
    use serde_json::json;
    // Use BTreeMap so the serialized JSON object keys are emitted in a stable,
    // deterministic order regardless of parallel collection order.
    type RefMap = BTreeMap<String, serde_json::Value>;
    let mut grouped: HashMap<String, (RefMap, RefMap)> = HashMap::new();

    for (key, refs) in forward {
        if let Some((file, symbol)) = key.split_once(':') {
            let entry = grouped.entry(file.to_string()).or_default();
            entry.0.insert(
                symbol.to_string(),
                serde_json::to_value(refs).unwrap_or(json!([])),
            );
        }
    }
    for (key, refs) in reverse {
        if let Some((file, symbol)) = key.split_once(':') {
            let entry = grouped.entry(file.to_string()).or_default();
            entry.1.insert(
                symbol.to_string(),
                serde_json::to_value(refs).unwrap_or(json!([])),
            );
        }
    }

    grouped
        .into_iter()
        .map(|(file, (fwd, rev))| {
            (
                file,
                PerFileRefs {
                    forward: json!(fwd),
                    reverse: json!(rev),
                },
            )
        })
        .collect()
}

fn run_git_cmd(cwd: &Path, args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Generate a full migration.toml config for the migration folder.
fn generate_migration_config(
    project: &migration_core::project::Project,
    config: &migration_core::config::Config,
) -> String {
    let (remote, branch, version) = detect_source_git_info(&project.root);

    let source_repo = remote.as_deref().unwrap_or("");
    let source_branch = branch.as_deref().unwrap_or("");
    let source_version = version.as_deref().unwrap_or("");

    let source_str = project.root.to_string_lossy().replace('\\', "/");
    format!(
        r##"[project]
source = "{}"
source_repo = "{}"
source_branch = "{}"
source_version = "{}"
source_language = "{}"
target_language = "{}"
strict = {}

[skip]
framework = {}

[output]
directory = "report"
split_by_directory = true

[compatibility]
# overrides_file = ".migration-assessor-compat.toml"

[scoring.weights]
in_degree = {}
complexity = {}
compatibility = {}
cycles = {}
tests = {}

[mapping]
# override_list = [
#     {{ from = "src/utils.ts", to = "new/src/utils.rs" }},
# ]
"##,
        source_str,
        source_repo,
        source_branch,
        source_version,
        project.source_language_str(),
        project.target_language,
        config.project.strict,
        config.skip.framework,
        config.scoring.weights.in_degree,
        config.scoring.weights.complexity,
        config.scoring.weights.compatibility,
        config.scoring.weights.cycles,
        config.scoring.weights.tests,
    )
}
