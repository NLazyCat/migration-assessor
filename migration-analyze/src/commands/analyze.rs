use clap::Args;
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use migration_core::ast;
use migration_core::db;
use migration_core::deps::module_map;
use migration_core::language::LanguageRegistry;
use migration_core::output_paths;
use migration_core::recommendation;
use migration_core::spec_writer;
use migration_core::*;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::commands::{resolve_project_path, run_git_cmd};

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
}

pub fn run(args: &AnalyzeArgs) -> anyhow::Result<()> {
    let project_root = resolve_project_path(&args.path);

    // Load or auto-create migration.toml
    let config_path = project_root.join("migration.toml");
    let mut config = if config_path.exists() {
        migration_core::config::Config::load(&config_path)?
    } else {
        eprintln!("  No migration.toml found — creating default config.");
        let guessed_lang = guess_source_language(&project_root);
        let default_config = format!(
            r#"# Migration Assessor Configuration
[project]
source = "."
source_lang = "{lang}"
target_language = "rust"
"#,
            lang = guessed_lang,
        );
        std::fs::write(&config_path, default_config)?;
        migration_core::config::Config::load(&config_path)?
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

    // Ensure source repo is available (clone remote repo if configured)
    ensure_source_repo(&mut config, &project_root)?;

    // Auto-detect source repo
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

    // ── Progress bar ─────────────────────────────────────────────────────
    let total_steps: u64 = 6;
    let pb = ProgressBar::new(total_steps);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{prefix:.bold} {bar:30.cyan/blue} {pos}/{len}  {msg:.dim}")?
            .progress_chars("##-"),
    );
    pb.set_prefix("Analyze");

    // Step 1: Discover files
    let discovery = discovery::FileDiscovery::new(
        project.source_language,
        config.project.ignore.clone(),
        config.project.exclude.clone(),
        config.skip.framework,
    );
    let files = discovery.discover(&project.root);
    pb.inc(1);

    // Step 2: Resolve dependencies
    let dependencies = deps::resolve_dependencies(&project.root, project.source_language)?;
    pb.inc(1);

    // Step 3: Compatibility matrix
    let mut compatibility = compatibility::CompatibilityMatrix::new(
        project.source_language_str().to_string(),
        project.target_language.clone(),
    );
    if let Some(overrides_file) = &config.compatibility.overrides_file {
        compatibility.load_overrides(&project.root.join(overrides_file))?;
    }
    let compatibility_matrix = compatibility.evaluate(&dependencies);
    pb.inc(1);

    // Step 4: Unified AST extraction (parallel — single pass per file)
    pb.set_message("parsing files…");
    let ast_outputs: Vec<ast::AstOutput> = files
        .par_iter()
        .filter_map(|file_path| {
            let source = std::fs::read_to_string(file_path).ok()?;
            // Use relative path for AstOutput (strip project root)
            let relative = file_path.strip_prefix(&project.root).unwrap_or(file_path);
            match ast::parse(&source, relative, project.source_language) {
                Ok(output) => Some(output),
                Err(e) => {
                    eprintln!("  {} {}: {}", style("WARN").yellow(), file_path.display(), e);
                    None
                }
            }
        })
        .collect();

    // Build dependency graph from pre-parsed imports (no re-parsing)
    let mut dependency_graph = build_graph_from_ast(&ast_outputs);
    let cycle_detection = dependency_graph.detect_cycles();
    pb.inc(1);

    // ── Create migration folder ─────────────────────────────────────────
    let migration_dir_name = format!("{}-migration", source_repo_name);
    let migration_root = project_root.join(&migration_dir_name);
    let report_dir = migration_root.join("report");

    // Mirror source structure
    pb.set_message("mirroring files…");
    for file in &files {
        let relative = file.strip_prefix(&project.root).unwrap_or(file);
        let target = migration_root.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, "")?;
    }
    pb.inc(1);
    pb.finish_and_clear();
    println!("  Mirrored {} files", files.len());

    // Write migration config
    let config_dir = migration_root.join("config");
    std::fs::create_dir_all(&config_dir)?;
    let mig_config = generate_migration_config(&project, &config);
    std::fs::write(config_dir.join("migration.toml"), mig_config)?;

    // Ensure report directory exists
    std::fs::create_dir_all(&report_dir)?;

    // Update root migration.toml with detected source path and git info
    let (_, _, version) = detect_source_git_info(&source_repo_dir);
    let source_path_display = source_repo_dir.to_string_lossy().replace('\\', "/");
    let source_lang = project.source_language_str();
    let root_config_content = format!(
        r#"# Migration Assessor Configuration
[project]
source = "{}"
source_lang = "{}"
target_language = "{}"
source_version = "{}"

[skip]
framework = {}
"#,
        source_path_display,
        source_lang,
        project.target_language,
        version.as_deref().unwrap_or(""),
        config.skip.framework,
    );
    std::fs::write(&config_path, root_config_content)?;

    // ── Step 5: Write spec JSON + SQLite + scores ──────────────────────
    println!();
    println!("  {} {} files…", style("Writing artifacts:").bold(), ast_outputs.len());

    // Build reverse dependency map from the graph
    let reverse_deps = build_reverse_deps(&dependency_graph.edges);

    // Build layer map from topological order
    let layer_map = compute_layers(&dependency_graph);

    // Compute module-level external deps
    let module_deps =
        module_map::module_external_deps(&project.root, &files, project.source_language);

    // ── Reverse reference extraction ─────────────────────────────────
    pb.set_message("extracting references…");
    pb.inc(1);
    let reverse_index = match project.source_language {
        migration_core::project::SourceLanguage::TypeScript => {
            migration_core::references::typescript::extract_all(&project.root, &files)
                .map(|(_, rev)| rev)
                .unwrap_or_else(|e| {
                    eprintln!("  {} reference extraction failed: {}", style("WARN").yellow(), e);
                    HashMap::new()
                })
        }
        migration_core::project::SourceLanguage::Rust => {
            migration_core::references::rust::extract_all(&project.root, &files)
                .map(|(_, rev)| rev)
                .unwrap_or_else(|e| {
                    eprintln!("  {} reference extraction failed: {}", style("WARN").yellow(), e);
                    HashMap::new()
                })
        }
        migration_core::project::SourceLanguage::JavaScript => {
            migration_core::references::javascript::extract_all(&project.root, &files)
                .map(|(_, rev)| rev)
                .unwrap_or_else(|e| {
                    eprintln!("  {} reference extraction failed: {}", style("WARN").yellow(), e);
                    HashMap::new()
                })
        }
    };
    println!("  Extracted {} symbol reference entries", reverse_index.len());

    // Write reverse index shards to disk for later use by `diff` command
    migration_core::references::write_reverse_shards(&reverse_index, &report_dir)?;

    // Compute migration readiness scores (needs reverse refs, compatibility, cycles)
    let readiness = scores::calculate(
        &project.root,
        &files,
        &build_symbol_pairs(&ast_outputs),
        &reverse_index,
        &compatibility_matrix,
        &cycle_detection,
        Some(&module_deps),
    )?;

    // Open SQLite database
    let conn = db::open_or_create(&report_dir.join(output_paths::DB))?;
    db::write_modules(&conn, &readiness)?;
    db::write_edges(&conn, &dependency_graph.edges)?;
    db::write_cycles(&conn, &cycle_detection)?;
    db::init_task_queue(&conn)?;
    db::write_metadata(&conn, "source_repo", source_repo_name)?;
    db::write_metadata(
        &conn,
        "source_language",
        project.source_language_str(),
    )?;
    db::write_metadata(&conn, "target_language", &project.target_language)?;
    db::write_metadata(
        &conn,
        "analyzed_at",
        &chrono::Utc::now().to_rfc3339(),
    )?;
    if let Some(v) = &version {
        db::write_metadata(&conn, "source_version", v)?;
    }
    println!("  {} migration.db", style("✓").green());

    // Write spec/ directory (one JSON per file)
    let spec_dir = report_dir.join("spec");
    std::fs::create_dir_all(&spec_dir)?;

    let readiness_map: HashMap<&str, &scores::ModuleReadiness> = readiness
        .iter()
        .map(|m| (m.module.as_str(), m))
        .collect();

    let mut migration_order: Vec<(usize, &str)> = Vec::new();

    for ast_output in &ast_outputs {
        let refs = reverse_deps
            .get(&ast_output.file_path)
            .cloned()
            .unwrap_or_default();
        let layer = layer_map.get(&ast_output.file_path).copied().unwrap_or(0);
        let effort = readiness_map
            .get(ast_output.file_path.as_str())
            .map(|m| m.migration_effort.as_str())
            .unwrap_or("unknown");
        let has_tests = readiness_map
            .get(ast_output.file_path.as_str())
            .map(|m| m.has_tests)
            .unwrap_or(false);

        let spec = spec_writer::build_spec(ast_output, refs, layer, effort, has_tests);

        let spec_path = spec_dir.join(format!("{}.json", ast_output.file_path));
        if let Some(parent) = spec_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&spec)?;
        std::fs::write(&spec_path, json)?;

        // Track migration order by rank/score
        let rank = readiness_map
            .get(ast_output.file_path.as_str())
            .map(|m| m.rank)
            .unwrap_or(usize::MAX);
        migration_order.push((rank, ast_output.file_path.as_str()));
    }

    // Write migration_order.json
    migration_order.sort_by_key(|(rank, _)| *rank);
    let ordered_files: Vec<&str> = migration_order.iter().map(|(_, f)| *f).collect();
    let order_json = serde_json::json!({
        "order": ordered_files,
        "layers": layer_map,
    });
    std::fs::write(spec_dir.join("migration_order.json"), serde_json::to_string_pretty(&order_json)?)?;
    println!("  {} {} spec files", style("✓").green(), ast_outputs.len());

    // Dependency migration recommendations
    let recommendations =
        recommendation::build_recommendations(&dependencies, &compatibility, Some(&module_deps));
    println!("  {} {} recommendations", style("✓").green(), recommendations.dependencies.len());

    // ── Backward-compatible report files (for summary/boundaries/diff) ──
    use serde_json::json;
    let chrono = chrono::Utc::now();

    // project.json
    let project_meta = json!({
        "schemaVersion": "1.0.0",
        "generatedAt": chrono.to_rfc3339(),
        "sourceLanguage": project.source_language_str(),
        "targetLanguage": project.target_language,
        "sourceRoot": project.root.to_string_lossy(),
        "sourceRepo": source_repo_name,
        "filesAnalyzed": files.len(),
        "dependencyCount": dependencies.len(),
        "partialAnalysisCount": 0,
    });
    std::fs::write(
        report_dir.join(output_paths::PROJECT),
        serde_json::to_string_pretty(&project_meta)?,
    )?;

    // scores.json (backward compat)
    std::fs::write(
        report_dir.join(output_paths::SCORES),
        serde_json::to_string_pretty(&readiness)?,
    )?;

    // errors.json
    std::fs::write(
        report_dir.join(output_paths::ERRORS),
        serde_json::to_string_pretty(&json!([]))?,
    )?;

    // external/packages.json
    std::fs::create_dir_all(report_dir.join("external"))?;
    std::fs::write(
        report_dir.join(output_paths::external::PACKAGES),
        serde_json::to_string_pretty(&json!({ "packages": dependencies }))?,
    )?;

    // external/compatibility.json
    std::fs::write(
        report_dir.join(output_paths::external::COMPATIBILITY),
        serde_json::to_string_pretty(&compatibility_matrix)?,
    )?;

    // graph/nodes.json + graph/edges.json
    std::fs::create_dir_all(report_dir.join("graph"))?;
    std::fs::write(
        report_dir.join(output_paths::graph::NODES),
        serde_json::to_string_pretty(&dependency_graph.nodes)?,
    )?;
    std::fs::write(
        report_dir.join(output_paths::graph::EDGES),
        serde_json::to_string_pretty(&dependency_graph.edges)?,
    )?;
    std::fs::write(
        report_dir.join(output_paths::graph::CYCLES),
        serde_json::to_string_pretty(&cycle_detection)?,
    )?;

    // external/recommendations.json
    std::fs::write(
        report_dir.join(output_paths::external::RECOMMENDATIONS),
        serde_json::to_string_pretty(&recommendations)?,
    )?;

    // overview.json (backward compat - per-file symbol overview)
    let mut file_index = serde_json::Map::new();
    for ast_output in ast_outputs.iter() {
        file_index.insert(
            ast_output.file_path.clone(),
            json!({
                "symbol_count": ast_output.symbols.len(),
                "spec_path": format!("spec/{}.json", ast_output.file_path),
            }),
        );
    }
    std::fs::write(
        report_dir.join(output_paths::OVERVIEW),
        serde_json::to_string_pretty(&json!(file_index))?,
    )?;

    // ── Manifest ────────────────────────────────────────────────────────
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
            "externalRecommendations": output_paths::external::RECOMMENDATIONS,
            "database": output_paths::DB,
        }
    });
    std::fs::write(
        report_dir.join(output_paths::MANIFEST),
        serde_json::to_string_pretty(&manifest)?,
    )?;

    // ── AI-facing checklists (manifests) ────────────────────────────────
    std::fs::create_dir_all(report_dir.join("manifest"))?;

    let symbol_checklist = manifest::build_symbol_checklist(&conn, &report_dir)?;
    std::fs::write(
        report_dir.join(output_paths::manifest::SYMBOLS_CHECKLIST),
        serde_json::to_string_pretty(&symbol_checklist)?,
    )?;
    println!("  {} symbols-checklist", style("✓").green());

    let todo_list = manifest::build_todo_list(&conn, &report_dir)?;
    std::fs::write(
        report_dir.join(output_paths::manifest::TODO_LIST),
        serde_json::to_string_pretty(&todo_list)?,
    )?;
    println!("  {} todo-list", style("✓").green());

    let module_progress = manifest::build_module_progress(&conn)?;
    std::fs::write(
        report_dir.join(output_paths::manifest::MODULE_PROGRESS),
        serde_json::to_string_pretty(&module_progress)?,
    )?;
    println!("  {} module-progress", style("✓").green());

    // Output summary
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

    for entry in readiness.iter().take(10) {
        println!(
            "  #{:2} {:30} score: {:6.2}  (in-degree: {:2}, cycle: {:1})",
            entry.rank, entry.module, entry.score, entry.in_degree, entry.cycle_count,
        );
    }
    if readiness.len() > 10 {
        println!("  ... and {} more files", readiness.len() - 10);
    }

    // Generate HTML report
    crate::commands::report::generate_html_report(
        &report_dir,
        &json!({
            "schemaVersion": "1.0.0",
            "generatedAt": chrono.to_rfc3339(),
            "sourceLanguage": project.source_language_str(),
            "targetLanguage": project.target_language,
            "sourceRoot": project.root.to_string_lossy(),
            "sourceRepo": source_repo_name,
            "filesAnalyzed": files.len(),
            "dependencyCount": dependencies.len(),
            "partialAnalysisCount": 0
        }),
        &dependencies,
        &dependency_graph,
        &cycle_detection,
    )?;

    println!();
    println!(
        "  {} {}",
        style("Report generated:").bold().green(),
        style(format!("{}/report/", migration_dir_name)).underlined()
    );
    println!(
        "  {}",
        style("Run 'migration-analyze summary' to view results in terminal.").dim()
    );

    Ok(())
}

// ── Graph construction from pre-parsed AST ────────────────────────────────

/// Build a dependency graph from pre-parsed `AstOutput`s.
/// No file I/O or re-parsing — just import resolution.
fn build_graph_from_ast(ast_outputs: &[ast::AstOutput]) -> graph::DependencyGraph {
    use graph::{DependencyGraph, Edge, Node as GraphNode};

    let mut nodes: Vec<GraphNode> = Vec::new();
    let mut edges: Vec<Edge> = Vec::new();
    let mut file_indices: HashMap<&str, usize> = HashMap::new();

    // Collect all files as nodes
    for (i, output) in ast_outputs.iter().enumerate() {
        nodes.push(GraphNode {
            id: output.file_path.clone(),
            in_degree: 0,
            out_degree: 0,
            in_cycle: false,
            top_dir: "".to_string(),
            dir_path: "".to_string(),
        });
        file_indices.insert(output.file_path.as_str(), i);
    }

    // Resolve imports to edges
    for output in ast_outputs {
        let source_name = &output.file_path;
        for import_path in &output.imports.relative_imports {
            // Resolve relative import to a known file
            let target = resolve_relative_import_simple(import_path, source_name, &file_indices);
            if let Some(target_name) = target && target_name != output.file_path {
                edges.push(Edge {
                    from: output.file_path.clone(),
                    to: target_name.clone(),
                });
            }
        }
    }

    // Update in/out degree
    for edge in &edges {
        for node in &mut nodes {
            if node.id == edge.from {
                node.out_degree += 1;
            }
            if node.id == edge.to {
                node.in_degree += 1;
            }
        }
    }

    DependencyGraph {
        nodes,
        edges,
    }
}

/// Simple relative import resolver using file_indices keys.
fn resolve_relative_import_simple(
    import: &str,
    source_file: &str,
    file_indices: &HashMap<&str, usize>,
) -> Option<String> {
    if !import.starts_with('.') {
        return None;
    }

    let source_path = Path::new(source_file);
    let source_dir = source_path.parent().unwrap_or(Path::new(""));

    // Build resolved path and normalize
    let import_path = Path::new(import);
    let resolved = source_dir.join(import_path);

    // Normalize: strip ./ and ../ segments, convert backslashes
    let normalized = normalize_resolved_path(resolved);

    // Try exact match
    if file_indices.contains_key(normalized.as_str()) {
        return Some(normalized);
    }

    // Try with .ts, .tsx, .js extensions
    for ext in ["ts", "tsx", "js", "jsx"] {
        let with_ext = format!("{}.{}", normalized, ext);
        if file_indices.contains_key(with_ext.as_str()) {
            return Some(with_ext);
        }
    }

    // Try as directory with index file
    for ext in ["ts", "tsx"] {
        let index_path = format!("{}/index.{}", normalized, ext);
        if file_indices.contains_key(index_path.as_str()) {
            return Some(index_path);
        }
    }

    None
}

/// Normalize a resolved path: strip `./` segments, collapse `../`, convert `\` to `/`.
fn normalize_resolved_path(path: std::path::PathBuf) -> String {
    use std::path::Component;
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {} // skip `./`
            Component::ParentDir => {
                components.pop(); // collapse `../`
            }
            other => {
                if let Some(s) = other.as_os_str().to_str() {
                    components.push(s.to_string());
                }
            }
        }
    }
    let normalized = components.join("/");
    normalized.replace('\\', "/")
}

/// Build a reverse dependency map: file → list of files that import it.
fn build_reverse_deps(edges: &[graph::Edge]) -> HashMap<String, Vec<String>> {
    let mut deps: HashMap<String, Vec<String>> = HashMap::new();
    for edge in edges {
        deps.entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
    }
    deps
}

/// Compute topological layers (0 = no deps, migrated first).
fn compute_layers(graph: &graph::DependencyGraph) -> HashMap<String, usize> {
    let mut layers: HashMap<String, usize> = HashMap::new();
    let mut in_degree_map: HashMap<String, usize> = HashMap::new();
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

    for node in &graph.nodes {
        in_degree_map.entry(node.id.clone()).or_insert(0);
        adjacency.entry(node.id.clone()).or_default();
    }
    for edge in &graph.edges {
        *in_degree_map.entry(edge.to.clone()).or_insert(0) += 1;
        adjacency.entry(edge.from.clone()).or_default().push(edge.to.clone());
    }

    let mut queue: Vec<String> = in_degree_map
        .iter()
        .filter(|(_, deg)| **deg == 0)
        .map(|(f, _)| f.clone())
        .collect();

    let mut current_layer = 0;
    while !queue.is_empty() {
        let mut next_queue = Vec::new();
        for file in &queue {
            layers.insert(file.clone(), current_layer);
            if let Some(neighbors) = adjacency.get(file) {
                for neighbor in neighbors {
                    if let Some(deg) = in_degree_map.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            next_queue.push(neighbor.clone());
                        }
                    }
                }
            }
        }
        queue = next_queue;
        current_layer += 1;
    }

    // Remaining nodes (cycles) get the last layer
    for node in &graph.nodes {
        layers.entry(node.id.clone()).or_insert(current_layer);
    }

    layers
}

/// Build symbol pairs in the format expected by `scores::calculate`.
fn build_symbol_pairs(
    ast_outputs: &[ast::AstOutput],
) -> Vec<(symbols::SymbolIndex, symbols::ApiContract)> {
    ast_outputs
        .iter()
        .map(|output| {
            let index = symbols::SymbolIndex {
                module: output.file_path.clone(),
                symbols: output.symbols.clone(),
            };
            (index, output.api_contract.clone())
        })
        .collect()
}

// ── Detection helpers (unchanged from original) ───────────────────────────

/// Detect the source repository directory inside the project root.
fn detect_source_repo(
    project_root: &Path,
    config: &migration_core::config::Config,
) -> anyhow::Result<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    if is_repo_root(project_root) {
        candidates.push(project_root.to_path_buf());
    }

    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
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
            if let Some(source) = &config.project.source {
                let source_path = if Path::new(source).is_absolute() {
                    PathBuf::from(source)
                } else {
                    project_root.join(source)
                };
                if source_path.exists() && is_repo_root(&source_path) {
                    return Ok(source_path);
                }
            }
            anyhow::bail!(
                "No source repository found in {}.\n\
                 \n\
                 The project directory should contain either:\n\
                 \x20  - A .git/ folder (source repo is here)\n\
                 \x20  - package.json or tsconfig.json (TypeScript project)\n\
                 \x20  - Cargo.toml (Rust project)\n\
                 \n\
                 If the source is at a different path, create a migration.toml with:\n\
                 \x20  [project]\n\
                 \x20  source = \"path/to/source\"\n\
                 \x20  source_lang = \"typescript\"",
                project_root.display()
            );
        }
        1 => Ok(candidates.remove(0)),
        _ => {
            if let Some(source) = &config.project.source {
                let source_path = if Path::new(source).is_absolute() {
                    PathBuf::from(source)
                } else {
                    project_root.join(source)
                };
                if source_path.exists() && is_repo_root(&source_path) {
                    return Ok(source_path);
                }
            }
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
    if path.join(".git").exists() || path.join("HEAD").exists() {
        return true;
    }
    if path.join("package.json").exists()
        || path.join("Cargo.toml").exists()
        || path.join("tsconfig.json").exists()
    {
        return true;
    }
    false
}

fn ensure_source_repo(
    config: &mut migration_core::config::Config,
    project_root: &Path,
) -> anyhow::Result<()> {
    let Some(repo_url) = &config.project.source_repo else {
        return Ok(());
    };
    if repo_url.is_empty() {
        return Ok(());
    }

    if !is_remote_url(repo_url) {
        config.project.source = Some(repo_url.clone());
        return Ok(());
    }

    let repo_name = repo_name_from_url(repo_url);
    let target_dir = project_root.join(&repo_name);

    if target_dir.join(".git").exists() || target_dir.join("HEAD").exists() {
        println!(
            "  {} {}",
            style("Source repo:").bold(),
            target_dir.display()
        );
        config.project.source = Some(repo_name);
        return Ok(());
    }

    println!("  {} {} ...", style("Cloning:").bold(), repo_url);
    for attempt in 1..=4 {
        let output = std::process::Command::new("git")
            .args(["clone", repo_url, &repo_name])
            .current_dir(project_root)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                println!(
                    "  {} {}",
                    style("Clone successful:").bold().green(),
                    target_dir.display()
                );
                config.project.source = Some(repo_name);
                return Ok(());
            }
            _ => {
                if attempt < 4 {
                    eprintln!(
                        "  {} (attempt {}/4), retrying in 2s...",
                        style("Clone failed").yellow(),
                        attempt
                    );
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        }
    }

    eprintln!("  {}", style("Clone failed after 4 attempts.").red());
    eprintln!("  {}", style("Please clone manually:").bold());
    eprintln!("    cd {}", project_root.display());
    eprintln!("    git clone {} <folder-name>", repo_url);
    anyhow::bail!(
        "Failed to clone repository after 4 attempts: {}",
        repo_url
    );
}

fn is_remote_url(s: &str) -> bool {
    s.contains("://") || s.starts_with("git@")
}

fn repo_name_from_url(url: &str) -> String {
    let stem = url.trim_end_matches(".git").trim_end_matches('/');
    if let Some((_, name)) = stem.rsplit_once('/') {
        name.to_string()
    } else if let Some((_, name)) = stem.rsplit_once(':') {
        name.to_string()
    } else {
        "source".to_string()
    }
}

fn detect_source_git_info(source_root: &Path) -> (Option<String>, Option<String>, Option<String>) {
    let has_git = source_root.join(".git").exists() || source_root.join("HEAD").exists();
    if !has_git {
        return (None, None, None);
    }

    let remote = run_git_cmd(source_root, &["remote", "get-url", "origin"]);
    let branch = run_git_cmd(source_root, &["rev-parse", "--abbrev-ref", "HEAD"]);
    let version = run_git_cmd(source_root, &["describe", "--tags", "--exact-match", "HEAD"])
        .or_else(|| run_git_cmd(source_root, &["rev-parse", "--short", "HEAD"]));

    (remote, branch, version)
}

fn guess_source_language(project_root: &Path) -> String {
    LanguageRegistry::get()
        .detect_language(project_root)
        .unwrap_or_else(|| {
            if project_root.join("tsconfig.json").exists()
                || project_root.join("package.json").exists()
            {
                "typescript".to_string()
            } else if project_root.join("Cargo.toml").exists() {
                "rust".to_string()
            } else {
                "typescript".to_string()
            }
        })
}

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
source = "{source_str}"
source_repo = "{source_repo}"
source_branch = "{source_branch}"
source_version = "{source_version}"
source_language = "{source_lang}"
target_language = "{target_lang}"
strict = {strict}

[skip]
framework = {framework}

[output]
directory = "report"
split_by_directory = true

[compatibility]
# overrides_file = ".migration-assessor-compat.toml"

[scoring.weights]
in_degree = {w_in}
complexity = {w_complex}
compatibility = {w_compat}
cycles = {w_cycles}
tests = {w_tests}

[mapping]
# override_list = [
#     {{ from = "src/utils.ts", to = "new/src/utils.rs" }},
# ]
"##,
        source_str = source_str,
        source_repo = source_repo,
        source_branch = source_branch,
        source_version = source_version,
        source_lang = project.source_language_str(),
        target_lang = project.target_language,
        strict = config.project.strict,
        framework = config.skip.framework,
        w_in = config.scoring.weights.in_degree,
        w_complex = config.scoring.weights.complexity,
        w_compat = config.scoring.weights.compatibility,
        w_cycles = config.scoring.weights.cycles,
        w_tests = config.scoring.weights.tests,
    )
}
