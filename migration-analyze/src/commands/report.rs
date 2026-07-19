use std::path::Path;

use migration_core::deps::ResolvedDependency;
use migration_core::graph::CycleDetectionResult;
use migration_core::graph::DependencyGraph;
use serde_json::Value;

pub fn generate_html_report(
    output_dir: &Path,
    project_meta: &Value,
    dependencies: &[ResolvedDependency],
    dag: &DependencyGraph,
    cycles: &CycleDetectionResult,
) -> anyhow::Result<()> {
    let meta = project_meta;

    let source_lang = meta["sourceLanguage"].as_str().unwrap_or("?");
    let target_lang = meta["targetLanguage"].as_str().unwrap_or("?");
    let files_analyzed = meta["filesAnalyzed"].as_u64().unwrap_or(0);
    let dep_count = meta["dependencyCount"].as_u64().unwrap_or(0);
    let source_root = meta["sourceRoot"].as_str().unwrap_or("?");
    let source_repo = meta["sourceRepo"].as_str().unwrap_or("Project");
    let generated_at = meta["generatedAt"].as_str().unwrap_or("");

    // Read compatibility data
    let compat_path = output_dir.join("external").join("compatibility.json");
    let compat_json: Value = if compat_path.exists() {
        let text = std::fs::read_to_string(&compat_path)?;
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let _compat_json_str = serde_json::to_string(&compat_json).unwrap_or_default();

    // Read boundary layers
    let layers_path = output_dir.join("boundaries").join("layers.json");
    let layers_json: Value = if layers_path.exists() {
        let text = std::fs::read_to_string(&layers_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };
    let layers_count = match &layers_json {
        Value::Object(m) => m
            .get("layers")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0),
        _ => 0,
    };

    // Read uncut surfaces
    let uncut_path = output_dir.join("boundaries").join("uncut-surfaces.json");
    let uncut_json: Value = if uncut_path.exists() {
        let text = std::fs::read_to_string(&uncut_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    // Read scores
    let scores_path = output_dir.join("scores.json");
    let _scores_json: Value = if scores_path.exists() {
        let text = std::fs::read_to_string(&scores_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    // Read overview (file index with symbol counts)
    let overview_path = output_dir.join("overview.json");
    let overview_json: Value = if overview_path.exists() {
        let text = std::fs::read_to_string(&overview_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    // Read recommendations
    let recs_path = output_dir.join("external").join("recommendations.json");
    let recs_json: Value = if recs_path.exists() {
        let text = std::fs::read_to_string(&recs_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    let nodes_json = serde_json::to_string(&dag.nodes).unwrap_or_default();
    let edges_json = serde_json::to_string(&dag.edges).unwrap_or_default();
    let total_symbols = count_total_symbols(&overview_json);
    let has_boundaries = layers_count > 0;

    let bilingual_js = r#"<script>
var D = function(en, zh) { return { en: en, zh: zh }; };
var langDict = {};
langDict.metric_files = D('Files Analyzed', '\u5206\u6790\u6587\u4ef6\u6570');
langDict.metric_deps = D('Dependencies', '\u5916\u90e8\u4f9d\u8d56\u6570');
langDict.metric_edges = D('Graph Edges', '\u56fe\u8fb9\u6570');
langDict.metric_symbols = D('Symbols', '\u7b26\u53f7\u6570');
langDict.metric_cycles = D('Cycles', '\u5faa\u73af\u4f9d\u8d56');
langDict.metric_layers = D('Boundary Layers', '\u67b6\u6784\u5c42\u6570');
langDict.section_deps = D('Dependency Cross-Reference', '\u4f9d\u8d56\u4ea4\u53c9\u5bf9\u7167');
langDict.section_cycles = D('Cycle Detection', '\u5faa\u73af\u4f9d\u8d56\u68c0\u6d4b');
langDict.section_boundaries = D('Architecture Boundaries', '\u67b6\u6784\u8fb9\u754c');
langDict.section_references = D('Module References', '\u6a21\u5757\u5f15\u7528\u6982\u89c8');
langDict.section_api = D('Public API', '\u516c\u5f00 API');
langDict.section_graph = D('Dependency Graph', '\u4f9d\u8d56\u5173\u7cfb\u56fe');
langDict.graph_filter = D('Filter nodes by name...', '\u6309\u540d\u79f0\u8fc7\u6ee4\u8282\u70b9...');
langDict.graph_all = D('All nodes', '\u6240\u6709\u8282\u70b9');
langDict.no_deps = D('No external dependencies found.', '\u672a\u53d1\u73b0\u5916\u90e8\u4f9d\u8d56\u3002');
langDict.no_refs = D('No module reference data available.', '\u65e0\u53ef\u7528\u7684\u6a21\u5757\u5f15\u7528\u6570\u636e\u3002');
langDict.no_api = D('No public API data available.', '\u65e0\u53ef\u7528\u7684\u516c\u5f00 API \u6570\u636e\u3002');
langDict.layer_desc = D('Layered architecture derived from dependency depth.', '\u57fa\u4e8e\u4f9d\u8d56\u6df1\u5ea6\u7684\u5206\u5c42\u67b6\u6784\u3002');
langDict.api_desc = D('Exported functions, their parameters and return types.', '\u5bfc\u51fa\u7684\u51fd\u6570\u3001\u53c2\u6570\u53ca\u8fd4\u56de\u7c7b\u578b\u3002');
langDict.col_kind = D('Kind', '\u7c7b\u578b');
langDict.col_function = D('Function', '\u51fd\u6570');
langDict.col_params = D('Parameters', '\u53c2\u6570');
langDict.col_returns = D('Return Type', '\u8fd4\u56de\u7c7b\u578b');
langDict.col_package = D('Package', '\u5305\u540d');
langDict.col_version = D('Version', '\u7248\u672c');
langDict.col_equiv = D('Target Equivalent', '\u76ee\u6807\u7b49\u4ef7\u5e93');
langDict.col_compat = D('Compatibility', '\u517c\u5bb9\u6027');
langDict.col_effort = D('Effort', '\u5de5\u4f5c\u91cf');
langDict.col_risk = D('Risk', '\u98ce\u9669');
langDict.col_guidance = D('Guidance', '\u6307\u5bfc');
langDict.no_cycles = D('No cycles detected.', '\u672a\u68c0\u6d4b\u5230\u5faa\u73af\u4f9d\u8d56\u3002');
langDict.acyclic = D('The dependency graph is acyclic.', '\u4f9d\u8d56\u56fe\u662f\u65e0\u73af\u7684\u3002');
function setLang(lang) {
  document.getElementById('lang-en').classList.toggle('active', lang === 'en');
  document.getElementById('lang-zh').classList.toggle('active', lang === 'zh');
  document.querySelectorAll('[data-lang]').forEach(function(el) {
    el.style.display = el.getAttribute('data-lang') === lang ? '' : 'none';
  });
  document.querySelectorAll('[data-i18n]').forEach(function(el) {
    var key = el.getAttribute('data-i18n');
    if (langDict[key]) el.textContent = langDict[key][lang];
  });
  document.querySelectorAll('[data-i18n-placeholder]').forEach(function(el) {
    var key = el.getAttribute('data-i18n-placeholder');
    if (langDict[key]) el.placeholder = langDict[key][lang];
  });
}
</script>"#;

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>{source_repo} — Migration Assessment</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', sans-serif; background: #f0f2f5; color: #1a1a2e; }}
  .container {{ max-width: 1280px; margin: 0 auto; padding: 0 24px; }}

  /* Header */
  header {{ background: linear-gradient(135deg, #1a1a2e 0%, #16213e 100%); color: #fff; padding: 32px 0; margin-bottom: 24px; }}
  header h1 {{ font-size: 28px; font-weight: 700; letter-spacing: -0.5px; }}
  header h1 small {{ font-size: 16px; font-weight: 400; opacity: 0.7; margin-left: 12px; }}
  header .subtitle {{ font-size: 15px; opacity: 0.85; margin-top: 6px; }}
  header .meta-bar {{ font-size: 13px; opacity: 0.6; margin-top: 8px; display: flex; gap: 16px; flex-wrap: wrap; }}
  header .meta-bar span {{ display: inline-flex; align-items: center; gap: 4px; }}

  /* Metrics grid */
  .metrics {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(170px, 1fr)); gap: 14px; margin-bottom: 28px; }}
  .metric {{ background: #fff; border-radius: 10px; padding: 18px 20px; box-shadow: 0 1px 3px rgba(0,0,0,0.06); border-left: 3px solid #3b82f6; }}
  .metric h3 {{ font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px; color: #888; margin-bottom: 4px; }}
  .metric .value {{ font-size: 26px; font-weight: 700; }}
  .metric .value.green {{ color: #16a34a; }}
  .metric .value.orange {{ color: #d97706; }}
  .metric .value.red {{ color: #dc2626; }}
  .metric .value.blue {{ color: #2563eb; }}
  .metric .value.purple {{ color: #7c3aed; }}
  .metric .sub {{ font-size: 11px; color: #999; margin-top: 2px; }}

  /* Sections */
  .section {{ background: #fff; border-radius: 10px; padding: 24px; box-shadow: 0 1px 3px rgba(0,0,0,0.06); margin-bottom: 20px; }}
  .section h2 {{ font-size: 17px; font-weight: 600; margin-bottom: 16px; padding-bottom: 10px; border-bottom: 1px solid #eef0f2; display: flex; align-items: center; gap: 8px; }}
  .section h2 .count {{ font-size: 12px; font-weight: 500; background: #eef0f2; color: #666; padding: 2px 10px; border-radius: 20px; }}
  .section h3 {{ font-size: 14px; font-weight: 600; margin: 16px 0 8px; color: #374151; }}

  /* Tables */
  .table-wrap {{ overflow-x: auto; }}
  table {{ width: 100%; border-collapse: collapse; font-size: 13px; }}
  th, td {{ text-align: left; padding: 8px 12px; border-bottom: 1px solid #f3f4f6; white-space: nowrap; }}
  th {{ color: #6b7280; font-weight: 600; font-size: 11px; text-transform: uppercase; letter-spacing: 0.3px; background: #f9fafb; }}
  tr:hover td {{ background: #f9fafb; }}
  td:first-child {{ font-family: 'SF Mono', 'Fira Code', 'Cascadia Code', monospace; font-size: 12px; }}

  /* Badges */
  .badge {{ display: inline-block; padding: 2px 10px; border-radius: 20px; font-size: 11px; font-weight: 600; }}
  .badge.full {{ background: #dcfce7; color: #166534; }}
  .badge.partial {{ background: #fef3c7; color: #92400e; }}
  .badge.none {{ background: #fee2e2; color: #991b1b; }}
  .badge.unknown {{ background: #f3f4f6; color: #4b5563; }}
  .effort {{ font-size: 11px; padding: 2px 8px; border-radius: 4px; background: #f3f4f6; }}
  .effort.trivial {{ background: #dcfce7; color: #166534; }}
  .effort.moderate {{ background: #fef3c7; color: #92400e; }}
  .effort.heavy {{ background: #fed7aa; color: #9a3412; }}
  .effort.rewrite {{ background: #fee2e2; color: #991b1b; }}
  .risk-tag {{ display: inline-block; font-size: 10px; padding: 1px 6px; border-radius: 4px; background: #fef2f2; color: #b91c1c; margin: 1px; }}

  /* Cycles */
  .cycles-ok {{ background: #f0fdf4; border: 1px solid #bbf7d0; border-radius: 8px; padding: 16px; }}
  .cycles-ok strong {{ color: #16a34a; }}
  .cycles-warn {{ background: #fef2f2; border: 1px solid #fecaca; border-radius: 8px; padding: 16px; }}
  .cycles-warn strong {{ color: #dc2626; }}
  .cycle-list {{ margin-top: 12px; }}
  .cycle-item {{ background: #fafafa; border: 1px solid #e5e7eb; border-radius: 6px; padding: 12px 16px; margin-bottom: 8px; font-family: 'SF Mono', monospace; font-size: 12px; }}
  .cycle-item .step {{ color: #6b7280; }}

  /* Boundary layers */
  .layer-stack {{ display: flex; flex-direction: column-reverse; gap: 6px; margin: 12px 0; }}
  .layer-bar {{ display: flex; align-items: center; padding: 10px 16px; border-radius: 8px; font-size: 13px; }}
  .layer-bar .level {{ font-weight: 700; min-width: 80px; }}
  .layer-bar .desc {{ flex: 1; }}
  .layer-bar .count {{ font-size: 11px; color: #666; }}
  .layer-bar.l0 {{ background: #eef2ff; border: 1px solid #c7d2fe; }}
  .layer-bar.l1 {{ background: #f0fdf4; border: 1px solid #bbf7d0; }}
  .layer-bar.l2 {{ background: #fefce8; border: 1px solid #fde68a; }}
  .layer-bar.l3 {{ background: #fef2f2; border: 1px solid #fecaca; }}
  .layer-bar.l4 {{ background: #faf5ff; border: 1px solid #e9d5ff; }}
  .uncut-surfaces {{ margin-top: 8px; }}
  .uncut-surfaces td {{ font-family: 'SF Mono', monospace; font-size: 12px; }}

  /* Reference summary */
  .ref-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 10px; margin-top: 8px; }}
  .ref-card {{ background: #fafafa; border: 1px solid #e5e7eb; border-radius: 6px; padding: 10px 14px; }}
  .ref-card .module {{ font-family: 'SF Mono', monospace; font-size: 12px; font-weight: 600; color: #374151; }}
  .ref-card .mod-stats {{ font-size: 11px; color: #888; margin-top: 2px; display: flex; gap: 12px; }}

  /* Graph */
  #graph {{ width: 100%; height: 650px; position: relative; border: 1px solid #eef0f2; border-radius: 8px; overflow: hidden; }}
  #graph svg {{ width: 100%; height: 100%; }}
  #graph .node circle {{ stroke: #fff; stroke-width: 1.5px; cursor: pointer; transition: r 0.2s; }}
  #graph .node:hover circle {{ stroke-width: 3px; }}
  #graph .node text {{ font-size: 10px; pointer-events: none; font-family: 'SF Mono', monospace; }}
  #graph .node text.label-bg {{ stroke: #fff; stroke-width: 3px; stroke-linejoin: round; fill: none; }}
  #graph .link {{ stroke: #999; stroke-opacity: 0.4; }}
  #graph .node.highlighted circle {{ stroke: #000; stroke-width: 2.5px; }}
  #graph .node.faded {{ opacity: 0.15; }}
  #graph .link.faded {{ stroke-opacity: 0.05; }}
  .graph-tooltip {{ position: absolute; background: #1a1a2e; color: #fff; padding: 6px 10px; border-radius: 6px; font-size: 12px; pointer-events: none; white-space: nowrap; max-width: 400px; overflow: hidden; text-overflow: ellipsis; opacity: 0; transition: opacity 0.15s; z-index: 10; }}
  .graph-controls {{ display: flex; gap: 10px; margin-bottom: 12px; }}
  .graph-controls input, .graph-controls select {{ padding: 6px 12px; border: 1px solid #d0d0d0; border-radius: 6px; font-size: 13px; }}
  .graph-controls input {{ flex: 1; }}
  .graph-info {{ padding: 8px 12px; background: #f7f8fa; border-radius: 6px; font-size: 12px; color: #666; margin-top: 8px; }}
  .graph-legend {{ display: flex; flex-wrap: wrap; gap: 8px; margin-top: 10px; font-size: 12px; color: #666; }}

  /* API section */
  .api-module {{ margin-bottom: 4px; border: 1px solid #e5e7eb; border-radius: 6px; overflow: hidden; }}
  .api-toggle {{ width: 100%; display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: #fafafa; border: none; cursor: pointer; font-size: 13px; text-align: left; transition: background 0.15s; }}
  .api-toggle:hover {{ background: #f0f1f3; }}
  .api-arrow {{ font-size: 10px; color: #888; width: 14px; }}
  .api-path {{ font-family: 'SF Mono', monospace; font-size: 12px; color: #374151; flex: 1; }}
  .api-count {{ font-size: 11px; color: #888; background: #eef0f2; padding: 1px 8px; border-radius: 10px; }}
  .api-body {{ padding: 0; }}
  .api-body table {{ margin: 0; }}
  .api-body td {{ font-family: 'SF Mono', 'Fira Code', monospace; font-size: 11px; }}
  .api-body td code {{ font-size: 11px; background: #f3f4f6; padding: 1px 5px; border-radius: 3px; }}
  .fn-kind {{ display: inline-block; font-size: 10px; padding: 1px 6px; border-radius: 4px; background: #eef2ff; color: #4338ca; text-transform: uppercase; }}
  .fn-kind.fn-function {{ background: #eef2ff; color: #4338ca; }}
  .fn-kind.fn-class {{ background: #f0fdf4; color: #166534; }}
  .fn-kind.fn-interface {{ background: #fefce8; color: #92400e; }}
  .fn-kind.fn-type {{ background: #f3e8ff; color: #6b21a8; }}
  .fn-kind.fn-enum {{ background: #fef2f2; color: #991b1b; }}

  /* Language switcher */
  .lang-switch {{ display: inline-flex; gap: 4px; margin-left: 20px; }}
  .lang-switch button {{ background: transparent; border: 1px solid rgba(255,255,255,0.3); color: #fff; padding: 3px 10px; border-radius: 4px; cursor: pointer; font-size: 12px; opacity: 0.6; transition: all 0.15s; }}
  .lang-switch button.active {{ opacity: 1; background: rgba(255,255,255,0.15); border-color: rgba(255,255,255,0.6); }}
  .lang-switch button:hover {{ opacity: 0.9; }}

  @media (max-width: 768px) {{ .metrics {{ grid-template-columns: repeat(2, 1fr); }} .graph-controls {{ flex-direction: column; }} }}
</style>
</head>
<body>

<header>
<div class="container">
  <div style="display:flex;align-items:center;justify-content:space-between;flex-wrap:wrap;gap:8px;">
    <div>
      <h1>{source_repo} <small data-lang="en">Migration Assessment</small><small data-lang="zh" style="display:none;">迁移评估报告</small></h1>
      <p class="subtitle">{source_lang} → {target_lang} &middot; {source_root}</p>
    </div>
    <div class="lang-switch">
      <button onclick="setLang('en')" id="lang-en" class="active">EN</button>
      <button onclick="setLang('zh')" id="lang-zh">中文</button>
    </div>
  </div>
  <div class="meta-bar">
    <span data-lang="en">&#128197; Generated {generated_at}</span>
    <span data-lang="zh" style="display:none;">&#128197; 生成时间 {generated_at}</span>
    <span data-lang="en">&#128196; {files_analyzed} files analyzed</span>
    <span data-lang="zh" style="display:none;">&#128196; 分析 {files_analyzed} 个文件</span>
    <span data-lang="en">&#128279; {total_symbols} symbols</span>
    <span data-lang="zh" style="display:none;">&#128279; {total_symbols} 个符号</span>
  </div>
</div>
</header>

<div class="container">

  <!-- Metrics -->
  <div class="metrics">
    <div class="metric" style="border-left-color:#2563eb;">
      <h3 data-i18n="metric_files">Files Analyzed</h3>
      <div class="value blue">{files_analyzed}</div>
    </div>
    <div class="metric" style="border-left-color:#d97706;">
      <h3 data-i18n="metric_deps">Dependencies</h3>
      <div class="value orange">{dep_count}</div>
    </div>
    <div class="metric" style="border-left-color:#2563eb;">
      <h3 data-i18n="metric_edges">Graph Edges</h3>
      <div class="value blue">{edge_count}</div>
      <div class="sub">{unique_nodes} nodes in dependency graph</div>
    </div>
    <div class="metric" style="border-left-color:#7c3aed;">
      <h3 data-i18n="metric_symbols">Symbols</h3>
      <div class="value purple">{total_symbols}</div>
      <div class="sub">across {files_analyzed} modules</div>
    </div>
    <div class="metric" style="border-left-color:{cycle_color};">
      <h3 data-i18n="metric_cycles">Cycles</h3>
      <div class="value {cycle_class}">{cycle_count}</div>
      <div class="sub">{cycles_detail}</div>
    </div>
    <div class="metric" style="border-left-color:{boundaries_color};">
      <h3 data-i18n="metric_layers">Boundary Layers</h3>
      <div class="value {boundaries_class}">{layers_count}</div>
      <div class="sub">{boundaries_detail}</div>
    </div>
  </div>

  <!-- Dependencies Cross-Reference -->
  <div class="section" id="dependencies">
    <h2 data-i18n="section_deps">&#128279; Dependency Cross-Reference <span class="count">{dep_count}</span></h2>
    {deps_table}
  </div>

  <!-- Recommendations -->
  {recs_section}

  <!-- Cycle Detection -->
  <div class="section" id="cycles">
    <h2 data-i18n="section_cycles">&#128259; Cycle Detection <span class="count">{cycle_count}</span></h2>
    {cycle_html}
  </div>

  <!-- Boundary Layers -->
  {boundary_section}

  <!-- Module References Overview -->
  <div class="section" id="references">
    <h2 data-i18n="section_references">&#128200; Module References <span class="count">{total_symbols}</span></h2>
    {refs_overview}
  </div>

  <!-- Public API -->
  {api_section}

  <!-- Dependency Graph -->
  <div class="section" id="graph-section">
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:12px;">
      <h2 data-i18n="section_graph" style="margin:0;padding:0;border:0;">&#128279; Dependency Graph</h2>
    </div>
    <div class="graph-controls">
      <input id="graph-filter" type="text" data-i18n-placeholder="graph_filter" placeholder="Filter nodes by name...">
      <select id="graph-limit">
          <option data-i18n="graph_all" value="0">All nodes</option>
        <option value="50">Top 50</option>
        <option value="100" selected>Top 100</option>
        <option value="200">Top 200</option>
        <option value="500">Top 500</option>
      </select>
    </div>
    <div id="graph"></div>
    <div id="graph-tooltip" class="graph-tooltip"></div>
    <div id="graph-info" class="graph-info"></div>
    <div id="graph-legend" class="graph-legend"></div>
  </div>

</div>

<script>
  // ── Data ────────────────────────────────────────────────
  const allNodes = {nodes_json};
  const allEdges = {edges_json};

  const maxLinks = Math.max(...allNodes.map(n => n.out_degree), 1);
  const nodeById = new Map(allNodes.map(n => [n.id, n]));

  function topDir(n) {{ return n.top_dir; }}
  function linkCount(n) {{ return n.out_degree; }}
  const dirs = [...new Set(allNodes.map(topDir))].sort();
  const dirColors = d3.scaleOrdinal(d3.schemeTableau10).domain(dirs);

  function buildGraph(limit) {{
    const ranked = [...allNodes].sort((a, b) => b.out_degree - a.out_degree);
    const selected = limit > 0 ? new Set(ranked.slice(0, limit).map(n => n.id)) : new Set(allNodes.map(n => n.id));
    const nodes = allNodes.filter(n => selected.has(n.id));
    const nodeSet = new Set(nodes.map(n => n.id));
    const edges = allEdges.filter(e => nodeSet.has(e.from) && nodeSet.has(e.to));
    return {{ nodes, edges, nodeSet }};
  }}

  let svg, simulation, link, node, tooltip, info;
  let currentData = null;

  function render(limit, filterText) {{
    const container = document.getElementById('graph');
    container.innerHTML = '';

    const {{ nodes, edges, nodeSet }} = buildGraph(limit);

    let visibleNodes = nodes;
    if (filterText) {{
      const lower = filterText.toLowerCase();
      visibleNodes = nodes.filter(n => n.id.toLowerCase().includes(lower));
    }}

    const visibleSet = new Set(visibleNodes.map(n => n.id));
    const visibleEdges = edges.filter(e => visibleSet.has(e.from) && visibleSet.has(e.to));

    currentData = {{ all: nodes, visible: visibleNodes, edges: visibleEdges }};

    const width = container.clientWidth;
    const height = 650;

    svg = d3.select(container).append('svg').attr('width', width).attr('height', height);

    tooltip = d3.select('#graph-tooltip');
    info = d3.select('#graph-info');
    info.text(`Showing ${{visibleNodes.length}} of ${{allNodes.length}} nodes, ${{visibleEdges.length}} of ${{allEdges.length}} edges.`);

    const zoom = d3.zoom().scaleExtent([0.1, 8]).on('zoom', (event) => {{ gMain.attr('transform', event.transform); }});
    svg.call(zoom);

    const gMain = svg.append('g');

    link = gMain.append('g').selectAll('line')
      .data(visibleEdges).join('line')
      .attr('class', 'link')
      .attr('stroke-width', d => Math.min(3, 0.5 + (nodeById.get(d.from)?.out_degree || 0) / maxLinks * 2));

    node = gMain.append('g').selectAll('g')
      .data(visibleNodes).join('g').attr('class', 'node');

    const rScale = d3.scaleSqrt().domain([0, maxLinks]).range([4, Math.min(14, 4 + maxLinks * 0.3)]);

    node.append('circle')
      .attr('r', d => Math.max(4, rScale(linkCount(d) || 0)))
      .attr('fill', d => linkCount(d) > 0 ? dirColors(topDir(d)) : '#e5e7eb')
      .attr('stroke', d => linkCount(d) > 0 ? d3.color(dirColors(topDir(d))).darker(0.5) : '#d1d5db');

    node.append('text').attr('class', 'label-bg')
      .text(d => {{ const p = d.id.split('/'); return p[p.length - 1]; }})
      .attr('x', d => Math.max(4, rScale(linkCount(d) || 0)) + 4).attr('y', 4);

    node.append('text')
      .text(d => {{ const p = d.id.split('/'); return p[p.length - 1]; }})
      .attr('x', d => Math.max(4, rScale(linkCount(d) || 0)) + 4).attr('y', 4).attr('fill', '#333');

    node.append('title').text(d => d.id);

    node.on('mouseenter', function(event, d) {{
      tooltip.style('opacity', 1).style('left', (event.offsetX + 12) + 'px').style('top', (event.offsetY - 6) + 'px').text(d.id);
      d3.select(this).select('circle').attr('stroke-width', 3);
    }}).on('mouseleave', function() {{
      tooltip.style('opacity', 0);
      d3.select(this).select('circle').attr('stroke-width', 1.5);
    }});

    node.call(d3.drag()
      .on('start', (event, d) => {{ if (!event.active) simulation.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; }})
      .on('drag', (event, d) => {{ d.fx = event.x; d.fy = event.y; }})
      .on('end', (event, d) => {{ if (!event.active) simulation.alphaTarget(0); d.fx = null; d.fy = null; }}));

    const c = visibleNodes.length > 200 ? -80 : visibleNodes.length > 100 ? -120 : -200;

    simulation = d3.forceSimulation(visibleNodes)
      .force('link', d3.forceLink(visibleEdges.map(e => ({{ source: e.from, target: e.to }})).id(d => d.id).distance(d => Math.min(180, 40 + (linkCount(d.source) || 0) * 3)))
      .force('charge', d3.forceManyBody().strength(c))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide(10))
      .alphaDecay(0.02);

    simulation.on('tick', () => {{
      link.attr('x1', d => d.source.x).attr('y1', d => d.source.y).attr('x2', d => d.target.x).attr('y2', d => d.target.y);
      node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
    }});

    const legend = d3.select('#graph-legend').html('');
    dirs.forEach(dir => {{
      const count = visibleNodes.filter(n => n.top_dir === dir).length;
      legend.append('span').html(`<span style="display:inline-block;width:10px;height:10px;border-radius:50%;background:${{dirColors(dir)}};margin-right:4px;"></span> ${{dir}} (${{count}})`);
    }});
  }}

  let currentLimit = parseInt(document.getElementById('graph-limit').value) || 100;
  let currentFilter = document.getElementById('graph-filter').value;

  function updateGraph() {{ render(currentLimit, currentFilter); }}

  document.getElementById('graph-limit').addEventListener('change', function() {{ currentLimit = parseInt(this.value) || 0; updateGraph(); }});

  let filterTimeout;
  document.getElementById('graph-filter').addEventListener('input', function() {{
    clearTimeout(filterTimeout);
    filterTimeout = setTimeout(() => {{ currentFilter = this.value; updateGraph(); }}, 300);
  }});

  updateGraph();
  window.addEventListener('resize', updateGraph);
</script>

<!-- Bilingual support -->
{bilingual_js}

</body>
</html>"##,
        source_repo = source_repo,
        source_lang = source_lang,
        target_lang = target_lang,
        source_root = html_escape(source_root),
        generated_at = generated_at,
        files_analyzed = files_analyzed,
        dep_count = dep_count,
        edge_count = dag.edges.len(),
        total_symbols = total_symbols,
        unique_nodes = dag.nodes.len(),
        cycle_color = if cycles.has_cycles {
            "#dc2626"
        } else {
            "#16a34a"
        },
        cycle_class = if cycles.has_cycles { "red" } else { "green" },
        cycle_count = cycles.cycles.len() + cycles.self_loops.len(),
        cycles_detail = format_cycles_detail(cycles),
        boundaries_color = if has_boundaries { "#7c3aed" } else { "#9ca3af" },
        boundaries_class = if has_boundaries { "purple" } else { "" },
        boundaries_detail = if has_boundaries {
            format!("{} layers in architecture", layers_count)
        } else {
            "Not analyzed (run `boundaries`)".to_string()
        },
        layers_count = layers_count,
        deps_table = build_deps_table(dependencies, &compat_json),
        recs_section = build_recs_section(&recs_json),
        cycle_html = build_cycles_section(cycles),
        boundary_section = build_boundary_section(&layers_json, &uncut_json),
        refs_overview = build_refs_overview(&overview_json),
        api_section = build_api_section(output_dir, &overview_json),
        bilingual_js = bilingual_js,
    );

    std::fs::write(output_dir.join("index.html"), html)?;
    Ok(())
}

fn count_total_symbols(overview: &Value) -> u64 {
    match overview {
        Value::Object(map) => {
            let mut total = 0u64;
            for (_key, info) in map {
                if let Some(c) = info["symbol_count"].as_u64() {
                    total += c;
                }
            }
            total
        }
        _ => 0,
    }
}

fn format_cycles_detail(cycles: &CycleDetectionResult) -> String {
    let mut parts = Vec::new();
    if !cycles.cycles.is_empty() {
        parts.push(format!("{} cycle(s)", cycles.cycles.len()));
    }
    if !cycles.self_loops.is_empty() {
        parts.push(format!("{} self-loop(s)", cycles.self_loops.len()));
    }
    if parts.is_empty() {
        "No cycles".to_string()
    } else {
        parts.join(", ")
    }
}

// ── Sections ─────────────────────────────────────────────

fn build_deps_table(deps: &[ResolvedDependency], compat: &Value) -> String {
    if deps.is_empty() {
        return "<p style='color:#888;'>No external dependencies found.</p>".to_string();
    }

    let mut rows = String::new();
    for dep in deps {
        let compat_info = compat.get(&dep.name);

        let (equiv, compat_level, effort, risk_tags, guidance) = match compat_info {
            Some(Value::Object(m)) => (
                m.get("equivalent").and_then(|v| v.as_str()).unwrap_or("—"),
                m.get("compatibility")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown"),
                m.get("effort").and_then(|v| v.as_str()).unwrap_or(""),
                m.get("risk_tags")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                    .unwrap_or_default(),
                m.get("guidance").and_then(|v| v.as_str()).unwrap_or(""),
            ),
            _ => ("—", "unknown", "", Vec::new(), ""),
        };

        let badge_class = match compat_level {
            "full" => "full",
            "partial" => "partial",
            "none" => "none",
            _ => "unknown",
        };

        let effort_class = match effort {
            "trivial" => "trivial",
            "moderate" => "moderate",
            "heavy" => "heavy",
            "rewrite" => "rewrite",
            _ => "",
        };

        let risk_html = if risk_tags.is_empty() {
            String::new()
        } else {
            risk_tags
                .iter()
                .map(|t| format!(r#"<span class="risk-tag">{}</span>"#, html_escape(t)))
                .collect::<Vec<_>>()
                .join(" ")
        };

        rows.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td>{}</td><td><span class="badge {}">{}</span></td>{}{}<td style="max-width:200px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-size:11px;color:#666;">{}</td></tr>"#,
            html_escape(&dep.name),
            html_escape(&dep.version),
            html_escape(equiv),
            badge_class,
            compat_level,
            if effort.is_empty() { String::new() } else { format!(r#"<td><span class="effort {}">{}</span></td>"#, effort_class, effort) },
            if risk_html.is_empty() { String::new() } else { format!("<td>{}</td>", risk_html) },
            html_escape(guidance)
        ));
    }

    format!(
        r#"<div class="table-wrap"><table>
          <thead><tr><th>Package</th><th>Version</th><th>Target Equivalent</th><th>Compatibility</th><th>Effort</th><th>Risk</th><th>Guidance</th></tr></thead>
          <tbody>{}</tbody></table></div>"#,
        rows
    )
}

fn build_recs_section(recs: &Value) -> String {
    let deps = match recs {
        Value::Object(m) => m.get("dependencies").and_then(|v| v.as_array()),
        _ => None,
    };

    let deps = match deps {
        Some(d) if !d.is_empty() => d,
        _ => return String::new(),
    };

    let mut rows = String::new();
    for dep in deps {
        let name = dep["package"].as_str().unwrap_or("?");
        let equiv = dep["equivalent"].as_str().unwrap_or("—");
        let compat = dep["compatibility"].as_str().unwrap_or("unknown");
        let effort_val = dep["effort"].as_str().unwrap_or("");
        let _guidance = dep["guidance"].as_str().unwrap_or("");
        let modules = dep["affected_modules"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let count = dep["affected_module_count"].as_u64().unwrap_or(0);

        let badge_class = match compat {
            "full" => "full",
            "partial" => "partial",
            "none" => "none",
            _ => "unknown",
        };

        rows.push_str(&format!(
            r#"<tr><td>{}</td><td>{}</td><td><span class="badge {}">{}</span></td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
            html_escape(name),
            html_escape(equiv),
            badge_class,
            compat,
            html_escape(effort_val),
            count,
            html_escape(&modules)
        ));
    }

    format!(
        r#"<div class="section">
          <h2>&#128269; Recommendations <span class="count">{}</span></h2>
          <div class="table-wrap"><table>
            <thead><tr><th>Package</th><th>Recommended</th><th>Compatibility</th><th>Effort</th><th>Affected Modules</th><th>Files</th></tr></thead>
            <tbody>{}</tbody></table></div>
        </div>"#,
        deps.len(),
        rows
    )
}

fn build_cycles_section(cycles: &CycleDetectionResult) -> String {
    if !cycles.has_cycles {
        return r#"<div class="cycles-ok"><strong>&#10003; No cycles detected.</strong> The dependency graph is acyclic.</div>"#.to_string();
    }

    let mut parts = String::new();

    if !cycles.self_loops.is_empty() {
        parts.push_str(&format!(
            r#"<div class="cycles-warn"><strong>&#9888; {} self-loop(s)</strong> — modules that reference themselves.</div><div class="cycle-list">"#,
            cycles.self_loops.len()
        ));
        for sl in &cycles.self_loops {
            parts.push_str(&format!(
                r#"<div class="cycle-item">&#128259; {}</div>"#,
                html_escape(sl)
            ));
        }
        parts.push_str("</div>");
    }

    if !cycles.cycles.is_empty() {
        parts.push_str(&format!(
            r#"<div class="cycles-warn"><strong>&#9888; {} cycle(s) detected</strong> — circular dependencies that may need refactoring.</div><div class="cycle-list">"#,
            cycles.cycles.len()
        ));

        for (i, cycle) in cycles.cycles.iter().enumerate() {
            parts.push_str(&format!(
                r#"<div class="cycle-item"><strong>Cycle {}:</strong> "#,
                i + 1
            ));
            for (j, node) in cycle.nodes.iter().enumerate() {
                if j > 0 {
                    parts.push_str(r#" <span class="step">&#8594;</span> "#);
                }
                parts.push_str(&html_escape(node).to_string());
            }
            parts.push_str("</div>");
        }
        parts.push_str("</div>");
    }

    parts
}

fn build_boundary_section(layers: &Value, uncut: &Value) -> String {
    let layer_list = match layers {
        Value::Object(m) => m.get("layers").and_then(|v| v.as_array()),
        _ => None,
    };

    let layer_list = match layer_list {
        Some(l) if !l.is_empty() => l,
        _ => return String::new(),
    };

    let mut layer_bars = String::new();
    for layer in layer_list.iter().rev() {
        let level = layer["level"].as_u64().unwrap_or(0);
        let desc = layer["description"].as_str().unwrap_or("");
        let modules = layer["modules"].as_array().map(|a| a.len()).unwrap_or(0);

        layer_bars.push_str(&format!(
            r#"<div class="layer-bar l{}"><span class="level">L{}</span><span class="desc">{}</span><span class="count">{} module(s)</span></div>"#,
            level, level, html_escape(desc), modules
        ));
    }

    let mut uncut_html = String::new();
    if let Value::Array(arr) = uncut
        && !arr.is_empty()
    {
        let mut rows = String::new();
        for surface in arr.iter().take(50) {
            let consumer = surface["consumer_module"].as_str().unwrap_or("?");
            let provider = surface["provider_module"].as_str().unwrap_or("?");
            let symbol = surface["symbol"].as_str().unwrap_or("?");
            let direction = surface["direction"].as_str().unwrap_or("?");
            rows.push_str(&format!(
                r#"<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>"#,
                html_escape(consumer),
                html_escape(provider),
                html_escape(symbol),
                html_escape(direction)
            ));
        }
        uncut_html = format!(
            r#"<h3>&#9888; Uncut Surfaces <span class="count">{}</span></h3>
                <div class="table-wrap uncut-surfaces"><table>
                  <thead><tr><th>Consumer</th><th>Provider</th><th>Symbol</th><th>Direction</th></tr></thead>
                  <tbody>{}</tbody></table></div>
                  {}"#,
            arr.len(),
            rows,
            if arr.len() > 50 {
                format!(
                    r#"<p style="font-size:12px;color:#888;margin-top:6px;">Showing 50 of {} uncut surfaces.</p>"#,
                    arr.len()
                )
            } else {
                String::new()
            }
        );
    }

    format!(
        r#"<div class="section" id="boundaries">
          <h2>&#127912; Architecture Boundaries <span class="count">{}</span></h2>
          <p style="font-size:13px;color:#666;margin-bottom:12px;">Layered architecture derived from dependency depth. Foundation (L0) is the deepest layer.</p>
          <div class="layer-stack">{}</div>
          {}
        </div>"#,
        layer_list.len(),
        layer_bars,
        uncut_html
    )
}

fn build_refs_overview(overview: &Value) -> String {
    let modules = match overview {
        Value::Object(m) if !m.is_empty() => m,
        _ => {
            return r#"<p style="color:#888;">No module reference data available.</p>"#.to_string();
        }
    };

    let mut cards = String::new();
    let mut entries: Vec<(&str, u64)> = modules
        .iter()
        .filter_map(|(k, v)| {
            v.as_object()
                .and_then(|m| m.get("symbol_count"))
                .and_then(|c| c.as_u64())
                .map(|c| (k.as_str(), c))
        })
        .collect();

    entries.sort_by_key(|(_, count)| std::cmp::Reverse(*count));

    for (module, count) in entries.iter().take(100) {
        cards.push_str(&format!(
            r#"<div class="ref-card"><div class="module">{}</div><div class="mod-stats"><span>&#128279; {} symbol(s)</span></div></div>"#,
            html_escape(module),
            count
        ));
    }

    format!(
        r#"<p style="font-size:13px;color:#666;margin-bottom:8px;">Symbol count per module (top {} of {}).</p><div class="ref-grid">{}</div>{}"#,
        entries.len().min(100),
        modules.len(),
        cards,
        if entries.len() > 100 {
            format!(
                r#"<p style="font-size:12px;color:#888;margin-top:8px;">Showing 100 of {} modules.</p>"#,
                entries.len()
            )
        } else {
            String::new()
        }
    )
}

fn build_api_section(output_dir: &Path, overview: &Value) -> String {
    let modules = match overview {
        Value::Object(m) if !m.is_empty() => m,
        _ => return String::new(),
    };

    let mut entries: Vec<(&str, u64, &str)> = modules
        .iter()
        .filter_map(|(k, v)| {
            let obj = v.as_object()?;
            let count = obj.get("symbol_count")?.as_u64()?;
            let path = obj.get("contracts_path")?.as_str()?;
            Some((k.as_str(), count, path))
        })
        .collect();

    entries.sort_by_key(|b| std::cmp::Reverse(b.1));

    let mut sections = String::new();
    let mut total_exports = 0u64;

    for (module, _sym_count, contract_path) in entries.iter().take(20) {
        let full_path = output_dir.join(contract_path);
        if !full_path.exists() {
            continue;
        }
        let content = match std::fs::read_to_string(&full_path) {
            Ok(c) => c,
            _ => continue,
        };
        let contract: Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            _ => continue,
        };

        let exports = match contract.get("exports").and_then(|v| v.as_array()) {
            Some(e) => e,
            _ => continue,
        };

        if exports.is_empty() {
            continue;
        }

        let mut rows = String::new();
        for export in exports.iter().take(30) {
            let name = export["name"].as_str().unwrap_or("?");
            let kind = export["kind"].as_str().unwrap_or("?");
            let ret = export["return_type"].as_str().unwrap_or("—");
            let _signature = export["signature"].as_str().unwrap_or("");

            let params: String = export["params"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|p| {
                            let pname = p["name"].as_str()?;
                            let ptype = p["type"].as_str().unwrap_or("any");
                            Some(format!("{}: {}", pname, ptype))
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();

            rows.push_str(&format!(
                r#"<tr><td><span class="fn-kind fn-{}">{}</span></td><td><code>{}</code></td><td><code>({})</code></td><td><code>{}</code></td></tr>"#,
                kind.to_lowercase(),
                html_escape(kind),
                html_escape(name),
                html_escape(&params),
                html_escape(ret)
            ));
            total_exports += 1;
        }

        if rows.is_empty() {
            continue;
        }

        let mod_id = module.replace(['/', '.', '-', ' '], "_");
        sections.push_str(&format!(
            r#"<div class="api-module">
              <button class="api-toggle" onclick="toggleApi('{mod_id}')">
                <span class="api-arrow">&#9654;</span>
                <span class="api-path">{}</span>
                <span class="api-count">{}</span>
              </button>
              <div id="api-{mod_id}" class="api-body" style="display:none;">
                <div class="table-wrap"><table>
                  <thead><tr><th>Kind</th><th style="min-width:160px;">Function</th><th style="min-width:200px;">Parameters</th><th style="min-width:120px;">Return Type</th></tr></thead>
                  <tbody>{}</tbody></table></div>
              </div>
            </div>"#,
            html_escape(module),
            exports.len(),
            rows
        ));
    }

    if sections.is_empty() {
        return String::new();
    }

    let toggle_js = r#"
<script>
function toggleApi(id) {
  var body = document.getElementById('api-' + id);
  var arrow = body.previousElementSibling.querySelector('.api-arrow');
  if (body.style.display === 'none') {
    body.style.display = '';
    arrow.innerHTML = '&#9660;';
  } else {
    body.style.display = 'none';
    arrow.innerHTML = '&#9654;';
  }
}
</script>"#;

    format!(
        r#"<div class="section" id="public-api">
          <h2 data-i18n="section_api">&#128220; Public API <span class="count">{}</span></h2>
          <p style="font-size:13px;color:#666;margin-bottom:12px;">Exported functions, their parameters and return types across top modules.</p>
          {}
        </div>
        {}"#,
        total_exports, sections, toggle_js
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
