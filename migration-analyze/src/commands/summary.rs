use clap::Args;
use migration_core::output_paths;
use serde_json::Value;

use crate::commands::context::ProjectContext;
use crate::commands::resolve_project_path;

#[derive(Args)]
pub struct SummaryArgs {
    /// Project root directory (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: String,

    /// Output format: text or json
    #[arg(long, default_value = "text")]
    pub format: String,
}

pub fn run(args: &SummaryArgs) -> anyhow::Result<()> {
    let project_root = resolve_project_path(&args.path);
    let ctx = ProjectContext::load(&project_root)?;

    if !ctx.report_dir.exists() {
        anyhow::bail!(
            "Report directory not found at {}. Run 'migration-analyze analyze' first.",
            ctx.report_dir.display()
        );
    }

    let project_meta = ctx.project_meta().ok();
    let scores = load_scores(&ctx);
    let deps = ctx
        .load_json::<Value>(output_paths::external::PACKAGES)
        .ok();
    let compat = ctx
        .load_json::<Value>(output_paths::external::COMPATIBILITY)
        .ok();
    let symbols = collect_symbols(&ctx);
    let dag = ctx.dag().ok();
    let boundaries_layers = ctx
        .load_json::<Value>(output_paths::boundaries::LAYERS)
        .ok();

    match args.format.as_str() {
        "json" => print_json_summary(&project_meta, &scores, &deps, &symbols, &dag),
        _ => print_text_summary(
            &project_meta,
            &scores,
            &deps,
            &compat,
            &symbols,
            &dag,
            &boundaries_layers,
        ),
    }

    Ok(())
}

fn load_scores(ctx: &ProjectContext) -> Vec<Value> {
    ctx.scores()
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
}

fn collect_symbols(ctx: &ProjectContext) -> Vec<(String, Value)> {
    let index = match ctx.index().ok() {
        Some(v) => v,
        None => return Vec::new(),
    };
    let map = match index.as_object() {
        Some(m) => m,
        None => return Vec::new(),
    };
    let mut results = Vec::new();
    for (module, info) in map {
        let path = info
            .get("symbols_path")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if path.is_empty() {
            continue;
        }
        if let Some(val) = ctx.load_json::<Value>(path).ok() {
            results.push((module.clone(), val));
        }
    }
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

fn print_text_summary(
    project_meta: &Option<Value>,
    scores: &[Value],
    deps: &Option<Value>,
    compat: &Option<Value>,
    symbols: &[(String, Value)],
    dag: &Option<Value>,
    boundaries: &Option<Value>,
) {
    use console::{Emoji, style};

    let check = Emoji("✓", "+");
    let cross = Emoji("✗", "x");
    let arrow = Emoji("→", "->");

    println!();
    println!(
        "{}",
        style("━━━ Migration Analysis Summary ━━━").bold().cyan()
    );
    println!();

    // Project info
    if let Some(meta) = project_meta {
        let lang = meta["sourceLanguage"].as_str().unwrap_or("?");
        let target = meta["targetLanguage"].as_str().unwrap_or("?");
        let files = meta["filesAnalyzed"].as_u64().unwrap_or(0);
        let dep_count = meta["dependencyCount"].as_u64().unwrap_or(0);
        let repo = meta["sourceRepo"].as_str().unwrap_or("?");

        println!(
            "  {} {} {} {}",
            style("Project:").bold(),
            repo,
            style(arrow),
            style(format!("{} {}", lang, target)).yellow()
        );
        println!("  {} {}", style("Files analyzed:").bold(), files);
        println!(
            "  {} {} ({} with symbols)",
            style("Source files:").bold(),
            symbols.len(),
            symbols
                .iter()
                .filter(|(_, v)| v["symbols"].as_array().is_some())
                .count()
        );
        println!("  {} {}", style("Dependencies:").bold(), dep_count);
    }

    println!();

    // Scores
    if let Some(last_score) = scores.last() {
        let score_val = last_score["score"].as_f64().unwrap_or(0.0);
        let migrated = last_score["files_migrated"].as_u64().unwrap_or(0);
        let total = last_score["files_total"].as_u64().unwrap_or(0);
        let pct = score_val * 100.0;

        let score_style = if pct >= 80.0 {
            style(format!("{:.1}%", pct)).green()
        } else if pct >= 50.0 {
            style(format!("{:.1}%", pct)).yellow()
        } else {
            style(format!("{:.1}%", pct)).red()
        };

        println!(
            "  {} {}  ({}/{} files migrated)",
            style("Migration Score:").bold(),
            score_style,
            migrated,
            total
        );
    }

    // Top 10 priority files
    if !scores.is_empty() {
        println!();
        println!("  {}", style("Top Priority Files:").bold());
        let limit = scores.len().min(10);
        for entry in &scores[..limit] {
            let module = entry["module"].as_str().unwrap_or("?");
            let score = entry["score"].as_f64().unwrap_or(0.0);
            let in_deg = entry["in_degree"].as_u64().unwrap_or(0);
            let cycle = entry["cycle_count"].as_u64().unwrap_or(0);

            let cycle_indicator = if cycle > 0 {
                style(format!("cycle:{}", cycle)).red().to_string()
            } else {
                String::new()
            };

            println!(
                "    {:2}. {:35} score: {:5.2}  in_deg: {:2}  {}",
                entry["rank"].as_u64().unwrap_or(0),
                module,
                score,
                in_deg,
                cycle_indicator
            );
        }
    }

    // Dependencies
    let dep_arr = deps
        .as_ref()
        .and_then(|d| d.get("packages"))
        .and_then(|p| p.as_array());
    if let Some(arr) = dep_arr {
        println!();
        println!("  {} {} packages", style("Dependencies:").bold(), arr.len());

        let compat_map = compat.as_ref().and_then(|c| c.as_object());
        let mut available = 0;
        let mut partial = 0;
        let mut unavailable = 0;

        for dep in arr {
            let name = dep["name"].as_str().unwrap_or("?");
            if let Some(cm) = compat_map {
                if let Some(info) = cm.get(name) {
                    match info["status"].as_str().unwrap_or("") {
                        "available" => available += 1,
                        "partial" => partial += 1,
                        _ => unavailable += 1,
                    }
                } else {
                    unavailable += 1;
                }
            }
        }

        if compat_map.is_some() {
            println!(
                "    {} available  {} partial  {} no alternative",
                style(format!("{}{}", check, available)).green(),
                style(format!("{}{}", arrow, partial)).yellow(),
                style(format!("{}{}", cross, unavailable)).red()
            );
        }
    }

    // Graph info
    if let Some(dag) = dag {
        let node_count = dag["nodes"].as_array().map(|a| a.len()).unwrap_or(0);
        let edge_count = dag["edges"].as_array().map(|a| a.len()).unwrap_or(0);
        println!();
        println!(
            "  {} {} nodes, {} edges",
            style("Dependency Graph:").bold(),
            node_count,
            edge_count
        );
    }

    // Boundaries info
    if let Some(b) = boundaries {
        let total_layers = b["total_layers"].as_u64().unwrap_or(0);
        let uncut = b["uncut_surface"].as_array().map(|a| a.len()).unwrap_or(0);
        println!();
        println!(
            "  {} {} layers, {} uncut interfaces",
            style("Boundaries:").bold(),
            total_layers,
            uncut
        );
    }

    println!();
    println!(
        "  {}",
        style(format!(
            "Report: {}/index.html",
            ctx_report_dir_display(project_meta)
        ))
        .dim()
    );
    println!();
}

fn ctx_report_dir_display(project_meta: &Option<Value>) -> String {
    project_meta
        .as_ref()
        .and_then(|m| m["sourceRepo"].as_str())
        .map(|r| format!("{}-migration/report", r))
        .unwrap_or_else(|| "<repo>-migration/report".to_string())
}

fn print_json_summary(
    project_meta: &Option<Value>,
    scores: &[Value],
    deps: &Option<Value>,
    symbols: &[(String, Value)],
    dag: &Option<Value>,
) {
    use serde_json::json;

    let summary = json!({
        "project": project_meta,
        "scores": scores,
        "dependencies": deps,
        "sourceFiles": symbols.len(),
        "graph": dag.as_ref().map(|d| json!({
            "nodes": d["nodes"].as_array().map(|a| a.len()).unwrap_or(0),
            "edges": d["edges"].as_array().map(|a| a.len()).unwrap_or(0),
        })),
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&summary).unwrap_or_default()
    );
}
