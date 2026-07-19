use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::commands::resolve_project_path;

#[derive(Args)]
pub struct BoundariesArgs {
    /// Project root directory (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: String,

    /// Report output format: json, text, or all
    #[arg(long, default_value = "all")]
    pub format: String,
}

#[derive(Debug, Serialize)]
struct LayerModule {
    module: String,
    layer: usize,
    in_degree: u32,
    out_degree: usize,
    public_symbols: Vec<String>,
    internal_symbols: Vec<String>,
    score: f64,
}

#[derive(Debug, Serialize)]
struct Layer {
    level: usize,
    description: String,
    modules: Vec<LayerModule>,
    total_public_symbols: usize,
}

#[derive(Debug, Serialize)]
struct UncutSurface {
    consumer_module: String,
    provider_module: String,
    symbol: String,
    kind: String,
    direction: String,
}

#[derive(Debug, Serialize)]
struct BoundariesReport {
    generated_at: String,
    source_language: String,
    target_language: String,
    total_layers: usize,
    layers: Vec<Layer>,
    uncut_surface: Vec<UncutSurface>,
}

// ── Input types ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct DagNode {
    #[serde(default)]
    nodes: Vec<String>,
    #[serde(default)]
    edges: Vec<DagEdge>,
}

#[derive(Debug, Deserialize)]
struct DagEdge {
    from: String,
    to: String,
}

#[derive(Debug, Deserialize)]
struct ScoreEntry {
    module: String,
    score: f64,
    rank: u32,
    #[serde(default)]
    in_degree: u32,
}

#[derive(Debug, Deserialize)]
struct ApiContractModule {
    module: String,
    exports: Vec<ApiExport>,
}

#[derive(Debug, Deserialize)]
struct ApiExport {
    name: String,
    kind: String,
}

#[derive(Debug, Deserialize, Default)]
struct ProjectMeta {
    #[serde(default)]
    source_language: String,
    #[serde(default)]
    target_language: String,
}

type ReverseIndex = HashMap<String, Vec<ReverseRef>>;

#[derive(Debug, Deserialize)]
struct ReverseRef {
    symbol: String,
    #[serde(default)]
    location: Option<ReverseLocation>,
    kind: String,
}

#[derive(Debug, Deserialize)]
struct ReverseLocation {
    file: String,
}

pub fn run(args: &BoundariesArgs) -> anyhow::Result<()> {
    let project_root = resolve_project_path(&args.path);

    if !project_root.join("migration.toml").exists() {
        anyhow::bail!(
            "migration.toml not found in {}. Run 'migration-analyze analyze' first.",
            project_root.display()
        );
    }

    let migration_dir = detect_migration_folder(&project_root)?;
    let report_dir = migration_dir.join("report");

    if !report_dir.exists() {
        anyhow::bail!(
            "Report folder not found at {}. Run 'migration-analyze analyze' first.",
            report_dir.display()
        );
    }

    // Load all report data
    let meta: ProjectMeta = read_json_or_default(&report_dir.join("project.json"));
    let dag: DagNode = read_json_or_default(&report_dir.join("internal-deps/dag.json"));
    let scores: Vec<ScoreEntry> = read_json_or_default(&report_dir.join("scores.json"));
    let reverse_index: ReverseIndex =
        read_json_or_default(&report_dir.join("references/reverse.json"));
    let api_contracts = load_all_api_contracts(&report_dir)?;

    let layer_map = compute_layers(&dag);
    let layers = build_layer_groups(&dag, &layer_map, &scores, &api_contracts, &reverse_index);
    let uncut = find_uncut_surfaces(&reverse_index, &layer_map);

    let report = BoundariesReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        source_language: meta.source_language,
        target_language: meta.target_language,
        total_layers: layers.len(),
        layers,
        uncut_surface: uncut,
    };

    // Output
    if args.format == "json" || args.format == "all" {
        let out_path = report_dir.join("interface-boundaries.json");
        std::fs::write(&out_path, serde_json::to_string_pretty(&report)?)?;
        println!("  Interface boundaries: {}", out_path.display());
    }

    if args.format == "text" || args.format == "all" {
        print_text_report(&report);
    }

    if args.format == "html" || args.format == "all" {
        let out_path = report_dir.join("interface-boundaries.html");
        std::fs::write(&out_path, render_html_report(&report))?;
        println!("  HTML report: {}", out_path.display());
    }

    Ok(())
}

// ── Layer computation: BFS from leaves ──────────────────────────────────

/// Compute dependency layers for migration ordering.
///
/// Layer 0 = foundation (most depended-on, no deps on higher layers).
/// Higher layers depend only on lower layers.
///
/// Algorithm: compute each node's topological "depth" as the longest path
/// from any root (leaf with no incoming edges), then subtract from the
/// maximum so the deepest nodes become layer 0.
fn compute_layers(dag: &DagNode) -> HashMap<String, usize> {
    let mut predecessors: HashMap<String, Vec<String>> = HashMap::new();
    let mut out_edges: HashMap<String, Vec<String>> = HashMap::new();

    for node in &dag.nodes {
        predecessors.entry(node.clone()).or_default();
        out_edges.entry(node.clone()).or_default();
    }

    for edge in &dag.edges {
        predecessors
            .entry(edge.to.clone())
            .or_default()
            .push(edge.from.clone());
        out_edges
            .entry(edge.from.clone())
            .or_default()
            .push(edge.to.clone());
    }

    // Longest-path depth via memoized DFS
    let mut depth: HashMap<String, usize> = HashMap::new();
    let mut visiting: HashSet<String> = HashSet::new();

    fn dfs(
        node: &str,
        predecessors: &HashMap<String, Vec<String>>,
        depth: &mut HashMap<String, usize>,
        visiting: &mut HashSet<String>,
    ) -> usize {
        if let Some(&d) = depth.get(node) {
            return d;
        }
        if !visiting.insert(node.to_string()) {
            return 0; // cycle guard
        }

        let max_pred_depth = predecessors
            .get(node)
            .unwrap_or(&vec![])
            .iter()
            .map(|p| dfs(p, predecessors, depth, visiting))
            .max()
            .unwrap_or(0);

        visiting.remove(node);

        let d = if predecessors.get(node).unwrap_or(&vec![]).is_empty() {
            0
        } else {
            max_pred_depth + 1
        };
        depth.insert(node.to_string(), d);
        d
    }

    for node in &dag.nodes {
        dfs(node, &predecessors, &mut depth, &mut visiting);
    }

    // Invert: deepest → layer 0
    let max_depth = depth.values().copied().max().unwrap_or(0);
    depth
        .into_iter()
        .map(|(node, d)| (node, max_depth - d))
        .collect()
}

// ── Build layer groups ──────────────────────────────────────────────────

fn build_layer_groups(
    dag: &DagNode,
    layer_map: &HashMap<String, usize>,
    scores: &[ScoreEntry],
    contracts: &[ApiContractModule],
    reverse_index: &ReverseIndex,
) -> Vec<Layer> {
    let score_map: HashMap<&str, u32> = scores
        .iter()
        .map(|s| (s.module.as_str(), s.in_degree))
        .collect();
    let score_val_map: HashMap<&str, f64> = scores
        .iter()
        .map(|s| (s.module.as_str(), s.score))
        .collect();
    let contract_map: HashMap<&str, &ApiContractModule> =
        contracts.iter().map(|c| (c.module.as_str(), c)).collect();

    let mut out_degree: HashMap<String, usize> = HashMap::new();
    for edge in &dag.edges {
        *out_degree.entry(edge.from.clone()).or_insert(0) += 1;
    }

    let max_layer = layer_map.values().copied().max().unwrap_or(0);
    let mut layers = Vec::new();

    for level in 0..=max_layer {
        let mut modules: Vec<LayerModule> = Vec::new();

        for (module, &layer) in layer_map {
            if layer != level {
                continue;
            }

            let (public_syms, internal_syms) =
                match contract_map.get(module.as_str()) {
                    Some(c) => classify_exports(&c.exports, module, reverse_index),
                    None => (vec![], vec![]),
                };

            modules.push(LayerModule {
                module: module.clone(),
                layer: level,
                in_degree: score_map.get(module.as_str()).copied().unwrap_or(0),
                out_degree: out_degree.get(module.as_str()).copied().unwrap_or(0),
                public_symbols: public_syms,
                internal_symbols: internal_syms,
                score: score_val_map
                    .get(module.as_str())
                    .copied()
                    .unwrap_or(0.0),
            });
        }

        modules.sort_by(|a, b| b.in_degree.cmp(&a.in_degree));
        let total_public: usize = modules.iter().map(|m| m.public_symbols.len()).sum();

        let description = match level {
            0 => "Foundation: zero external dependencies, migrate first",
            1 => "Core logic: depends on foundation only",
            2 => "Feature modules: core + cross-cutting",
            _ => "Higher-level composition",
        };

        layers.push(Layer {
            level,
            description: description.to_string(),
            modules,
            total_public_symbols: total_public,
        });
    }

    layers
}

fn classify_exports(
    exports: &[ApiExport],
    module: &str,
    reverse_index: &ReverseIndex,
) -> (Vec<String>, Vec<String>) {
    let mut public = Vec::new();
    let mut internal = Vec::new();

    for export in exports {
        let symbol_key = format!("{}:{}", module, export.name);
        let has_external = reverse_index
            .get(&symbol_key)
            .map(|refs| {
                refs.iter()
                    .any(|r| r.location.as_ref().map_or(false, |loc| loc.file != module))
            })
            .unwrap_or(false);

        if has_external {
            public.push(export.name.clone());
        } else {
            internal.push(export.name.clone());
        }
    }

    (public, internal)
}

// ── Find uncut surfaces ─────────────────────────────────────────────────

fn find_uncut_surfaces(
    reverse_index: &ReverseIndex,
    layer_map: &HashMap<String, usize>,
) -> Vec<UncutSurface> {
    let mut uncut = Vec::new();
    let mut seen = HashSet::new();

    for (symbol_key, refs) in reverse_index {
        let Some((provider_mod, symbol_name)) = symbol_key.split_once(':') else {
            continue;
        };
        let provider_layer = layer_map.get(provider_mod).copied().unwrap_or(0);

        for r in refs {
            let Some(loc) = &r.location else { continue };
            let consumer_layer = layer_map.get(&loc.file).copied().unwrap_or(0);

            if consumer_layer > provider_layer {
                let key = format!("{}->{}:{}", loc.file, provider_mod, symbol_name);
                if seen.insert(key) {
                    uncut.push(UncutSurface {
                        consumer_module: loc.file.clone(),
                        provider_module: provider_mod.to_string(),
                        symbol: symbol_name.to_string(),
                        kind: r.kind.clone(),
                        direction: format!("L{}->L{}", consumer_layer, provider_layer),
                    });
                }
            }
        }
    }

    uncut.sort_by(|a, b| a.direction.cmp(&b.direction));
    uncut
}

// ── HTML rendering ──────────────────────────────────────────────────────

fn render_html_report(report: &BoundariesReport) -> String {
    let mut html = String::with_capacity(8192);
    html.push_str("<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">\n");
    html.push_str("<title>Interface Boundaries</title>\n");
    html.push_str("<style>\n");
    html.push_str(include_str!("../static/boundaries.css"));
    html.push_str("</style></head><body>\n");

    html.push_str("<h1>Interface Boundary Report</h1>\n");
    html.push_str(&format!(
        "<p class=\"subtitle\">{} &rarr; {} | {} layers | {}</p>\n",
        report.source_language,
        report.target_language,
        report.total_layers,
        &report.generated_at[..19]
    ));

    for layer in &report.layers {
        html.push_str(&format!(
            "<div class=\"layer\">\n<div class=\"layer-header\"><span>Layer {}: {}</span><span>{} public symbols</span></div>\n",
            layer.level, layer.description, layer.total_public_symbols
        ));
        html.push_str("<div class=\"layer-body\">\n");

        for module in &layer.modules {
            html.push_str(&format!(
                "<div class=\"module\"><span class=\"module-name\">{}</span>",
                module.module
            ));
            html.push_str(&format!(
                "<span class=\"stats\">in={} out={} score={:.1}</span>\n",
                module.in_degree, module.out_degree, module.score
            ));

            if !module.public_symbols.is_empty() {
                html.push_str("<div class=\"sym-list\">");
                for sym in &module.public_symbols {
                    html.push_str(&format!(
                        "<span class=\"sym pub\">{} <span class=\"tag tag-pub\">pub</span></span>",
                        sym
                    ));
                }
                html.push_str("</div>\n");
            }

            if !module.internal_symbols.is_empty() {
                html.push_str("<div class=\"sym-list\">");
                for sym in &module.internal_symbols {
                    html.push_str(&format!(
                        "<span class=\"sym int\">{} <span class=\"tag tag-int\">int</span></span>",
                        sym
                    ));
                }
                html.push_str("</div>\n");
            }

            html.push_str("</div>\n");
        }

        html.push_str("</div></div>\n");
    }

    if !report.uncut_surface.is_empty() {
        html.push_str("<div class=\"uncut-section\"><h2>Uncut Cross-Layer Interfaces</h2>\n");
        html.push_str("<table><tr><th>Dir</th><th>Consumer</th><th>Provider</th><th>Symbol</th><th>Kind</th></tr>\n");
        for s in &report.uncut_surface {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>\n",
                s.direction, s.consumer_module, s.provider_module, s.symbol, s.kind
            ));
        }
        html.push_str("</table></div>\n");
    }

    html.push_str("</body></html>\n");
    html
}

// ── Text rendering ──────────────────────────────────────────────────────

fn print_text_report(report: &BoundariesReport) {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║          Interface Boundary Report                      ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!(
        "  {} → {} | {} layers",
        report.source_language, report.target_language, report.total_layers
    );
    println!();

    for layer in &report.layers {
        println!(
            "━━━ Layer {}: {} ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━",
            layer.level, layer.description
        );
        println!("  Public symbols: {}", layer.total_public_symbols);

        for module in &layer.modules {
            println!(
                "  ▸ {:<35} in_deg={:>2}  pub={} int={}  score={:.1}",
                module.module,
                module.in_degree,
                module.public_symbols.len(),
                module.internal_symbols.len(),
                module.score
            );
            if !module.public_symbols.is_empty() {
                println!("    pub: {}", module.public_symbols.join(", "));
            }
            if !module.internal_symbols.is_empty() {
                println!("    int: {}", module.internal_symbols.join(", "));
            }
        }
        println!();
    }

    if !report.uncut_surface.is_empty() {
        println!("━━━ Uncut Interfaces ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        for s in &report.uncut_surface {
            println!(
                "  {}  {} → {} :: {} ({})",
                s.direction, s.consumer_module, s.provider_module, s.symbol, s.kind
            );
        }
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

fn detect_migration_folder(project_root: &Path) -> anyhow::Result<PathBuf> {
    for entry in std::fs::read_dir(project_root)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.ends_with("-migration") && path.join("report").exists() {
            return Ok(path);
        }
    }
    anyhow::bail!(
        "No migration folder (*-migration/) found in {}",
        project_root.display()
    )
}

fn load_all_api_contracts(report_dir: &Path) -> anyhow::Result<Vec<ApiContractModule>> {
    let contracts_dir = report_dir.join("api-contracts").join("by-dir");
    let mut contracts = Vec::new();

    if !contracts_dir.exists() {
        return Ok(contracts);
    }

    fn visit(dir: &Path, contracts: &mut Vec<ApiContractModule>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    visit(&path, contracts);
                } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(c) = serde_json::from_str::<ApiContractModule>(&content) {
                            contracts.push(c);
                        }
                    }
                }
            }
        }
    }

    visit(&contracts_dir, &mut contracts);
    Ok(contracts)
}

fn read_json_or_default<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    if !path.exists() {
        return T::default();
    }
    let content = std::fs::read_to_string(path).unwrap_or_default();
    serde_json::from_str(&content).unwrap_or_default()
}
