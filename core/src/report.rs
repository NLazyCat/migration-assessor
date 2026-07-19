use std::path::Path;

use crate::deps::ResolvedDependency;
use crate::graph::CycleDetectionResult;
use crate::graph::DependencyGraph;
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

    // Read compatibility data
    let compat_path = output_dir.join("external").join("compatibility.json");
    let compat_json: Value = if compat_path.exists() {
        let text = std::fs::read_to_string(&compat_path)?;
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    let nodes_json = serde_json::to_string(&dag.nodes).unwrap_or_default();
    let edges_json = serde_json::to_string(&dag.edges).unwrap_or_default();
    let compat_json_str = serde_json::to_string(&compat_json).unwrap_or_default();

    let html = format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Migration Assessment Report</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f5f7fa; color: #1a1a2e; padding: 24px; }}
  .container {{ max-width: 1200px; margin: 0 auto; }}
  h1 {{ font-size: 24px; margin-bottom: 8px; }}
  .subtitle {{ color: #666; margin-bottom: 24px; font-size: 14px; }}
  .cards {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; margin-bottom: 24px; }}
  .card {{ background: #fff; border-radius: 10px; padding: 20px; box-shadow: 0 1px 3px rgba(0,0,0,0.08); }}
  .card h3 {{ font-size: 12px; text-transform: uppercase; color: #888; margin-bottom: 6px; }}
  .card .value {{ font-size: 28px; font-weight: 700; }}
  .card .value.green {{ color: #22c55e; }}
  .card .value.orange {{ color: #f59e0b; }}
  .card .value.red {{ color: #ef4444; }}
  .card .value.blue {{ color: #3b82f6; }}
  section {{ background: #fff; border-radius: 10px; padding: 20px; box-shadow: 0 1px 3px rgba(0,0,0,0.08); margin-bottom: 24px; }}
  section h2 {{ font-size: 16px; margin-bottom: 16px; padding-bottom: 8px; border-bottom: 1px solid #eee; }}
  table {{ width: 100%; border-collapse: collapse; font-size: 13px; }}
  th, td {{ text-align: left; padding: 8px 12px; border-bottom: 1px solid #f0f0f0; }}
  th {{ color: #888; font-weight: 600; font-size: 11px; text-transform: uppercase; }}
  .badge {{ display: inline-block; padding: 2px 8px; border-radius: 20px; font-size: 11px; font-weight: 600; }}
  .badge.full {{ background: #dcfce7; color: #166534; }}
  .badge.partial {{ background: #fef3c7; color: #92400e; }}
  .badge.none {{ background: #fee2e2; color: #991b1b; }}
  .badge.unknown {{ background: #f3f4f6; color: #4b5563; }}
  #graph {{ width: 100%; height: 650px; position: relative; }}
  #graph svg {{ width: 100%; height: 100%; }}
  #graph .node circle {{ stroke: #fff; stroke-width: 1.5px; cursor: pointer; transition: r 0.2s; }}
  #graph .node:hover circle {{ stroke-width: 3px; }}
  #graph .node text {{ font-size: 10px; pointer-events: none; font-family: monospace; }}
  #graph .node text.label-bg {{ stroke: #fff; stroke-width: 3px; stroke-linejoin: round; fill: none; }}
  #graph .link {{ stroke: #999; stroke-opacity: 0.4; }}
  #graph .node.highlighted circle {{ stroke: #000; stroke-width: 2.5px; }}
  #graph .node.faded {{ opacity: 0.15; }}
  #graph .link.faded {{ stroke-opacity: 0.05; }}
  .graph-tooltip {{ position: absolute; background: #1a1a2e; color: #fff; padding: 6px 10px; border-radius: 6px; font-size: 12px; pointer-events: none; white-space: nowrap; max-width: 400px; overflow: hidden; text-overflow: ellipsis; opacity: 0; transition: opacity 0.15s; z-index: 10; }}
  .cycles-warning {{ background: #fef2f2; border: 1px solid #fecaca; border-radius: 8px; padding: 12px 16px; margin-bottom: 16px; }}
  .cycles-ok {{ background: #f0fdf4; border: 1px solid #bbf7d0; border-radius: 8px; padding: 12px 16px; margin-bottom: 16px; }}
  .cycles-warning strong {{ color: #dc2626; }}
  .cycles-ok strong {{ color: #16a34a; }}
  .graph-info {{ padding: 8px 12px; background: #f7f8fa; border-radius: 6px; font-size: 12px; color: #666; margin-top: 8px; }}
</style>
</head>
<body>
<div class="container">
  <h1>Migration Assessment Report</h1>
  <p class="subtitle">{source_lang} → {target_lang} &middot; {source_root}</p>

  <div class="cards">
    <div class="card">
      <h3>Files Analyzed</h3>
      <div class="value blue">{files_analyzed}</div>
    </div>
    <div class="card">
      <h3>Dependencies</h3>
      <div class="value orange">{dep_count}</div>
    </div>
    <div class="card">
      <h3>Graph Edges</h3>
      <div class="value blue">{edge_count}</div>
    </div>
    <div class="card">
      <h3>Cycles</h3>
      <div class="value {cycle_class}">{cycle_count}</div>
    </div>
  </div>

  <!-- Cycles -->
  <section>
    <h2>Cycle Detection</h2>
    {cycle_html}
  </section>

  <!-- DAG Visualization -->
  <section>
    <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:12px;">
      <h2 style="margin:0;padding:0;border:0;">Dependency Graph</h2>
      <div>
        <input id="graph-filter" type="text" placeholder="Filter nodes..." style="padding:6px 12px;border:1px solid #d0d0d0;border-radius:6px;font-size:13px;width:220px;">
        <select id="graph-limit" style="padding:6px 8px;border:1px solid #d0d0d0;border-radius:6px;font-size:13px;margin-left:8px;">
          <option value="0">All nodes</option>
          <option value="50">Top 50</option>
          <option value="100" selected>Top 100</option>
          <option value="200">Top 200</option>
          <option value="500">Top 500</option>
        </select>
      </div>
    </div>
    <div id="graph" style="border:1px solid #eee;border-radius:8px;overflow:hidden;position:relative;"></div>
    <div id="graph-tooltip" class="graph-tooltip"></div>
    <div id="graph-info" class="graph-info"></div>
    <div id="graph-legend" style="display:flex;flex-wrap:wrap;gap:8px;margin-top:10px;font-size:12px;color:#666;"></div>
  </section>

  <!-- External Dependencies -->
  <section>
    <h2>External Dependencies</h2>
    {deps_table}
  </section>

  <!-- Compatibility -->
  <section>
    <h2>Compatibility</h2>
    {compat_table}
  </section>
</div>

<script>
  // ── Data ────────────────────────────────────────────────
  const allNodes = {nodes_json};
  const allEdges = {edges_json};

  const maxLinks = Math.max(...allNodes.map(n => n.out_degree), 1);
  const nodeById = new Map(allNodes.map(n => [n.id, n]));

  // Directory metadata is precomputed on each node.
  function topDir(n) {{ return n.top_dir; }}
  function linkCount(n) {{ return n.out_degree; }}
  const dirs = [...new Set(allNodes.map(topDir))].sort();
  const dirColors = d3.scaleOrdinal(d3.schemeTableau10).domain(dirs);

  // Build filtered graph
  function buildGraph(limit) {{
    // Rank nodes by connection count
    const ranked = [...allNodes].sort((a, b) => b.out_degree - a.out_degree);
    const selected = limit > 0 ? new Set(ranked.slice(0, limit).map(n => n.id)) : new Set(allNodes.map(n => n.id));

    const nodes = allNodes.filter(n => selected.has(n.id));
    const nodeSet = new Set(nodes.map(n => n.id));
    const edges = allEdges.filter(e => nodeSet.has(e.from) && nodeSet.has(e.to));

    return {{ nodes, edges, nodeSet }};
  }}

  // ── Render ──────────────────────────────────────────────
  let svg, simulation, link, node, tooltip, info;
  let currentData = null;

  function render(limit, filterText) {{
    const container = document.getElementById('graph');
    container.innerHTML = '';

    const {{ nodes, edges, nodeSet }} = buildGraph(limit);

    // Apply text filter
    let visibleNodes = nodes;
    if (filterText) {{
      const lower = filterText.toLowerCase();
      visibleNodes = nodes.filter(n => n.id.toLowerCase().includes(lower));
    }}

    // Filter edges to only visible nodes
    const visibleSet = new Set(visibleNodes.map(n => n.id));
    const visibleEdges = edges.filter(e => visibleSet.has(e.from) && visibleSet.has(e.to));

    currentData = {{ all: nodes, visible: visibleNodes, edges: visibleEdges }};

    const width = container.clientWidth;
    const height = 650;

    svg = d3.select(container)
      .append('svg')
      .attr('width', width)
      .attr('height', height);

    // Tooltip
    tooltip = d3.select('#graph-tooltip');

    // Info
    info = d3.select('#graph-info');
    info.text(`Showing ${{visibleNodes.length}} of ${{allNodes.length}} nodes, ${{visibleEdges.length}} of ${{allEdges.length}} edges.`);

    // Zoom
    const zoom = d3.zoom()
      .scaleExtent([0.1, 8])
      .on('zoom', (event) => {{
        gMain.attr('transform', event.transform);
      }});
    svg.call(zoom);

    const gMain = svg.append('g');

    // Links
    link = gMain.append('g')
      .selectAll('line')
      .data(visibleEdges)
      .join('line')
      .attr('class', 'link')
      .attr('stroke-width', d => Math.min(3, 0.5 + (nodeById.get(d.from)?.out_degree || 0) / maxLinks * 2));

    // Nodes
    node = gMain.append('g')
      .selectAll('g')
      .data(visibleNodes)
      .join('g')
      .attr('class', 'node');

    // Node circles - size proportional to importance
    const rScale = d3.scaleSqrt()
      .domain([0, maxLinks])
      .range([4, Math.min(14, 4 + maxLinks * 0.3)]);

    node.append('circle')
      .attr('r', d => Math.max(4, rScale(linkCount(d) || 0)))
      .attr('fill', d => linkCount(d) > 0 ? dirColors(topDir(d)) : '#e5e7eb')
      .attr('stroke', d => linkCount(d) > 0 ? d3.color(dirColors(topDir(d))).darker(0.5) : '#d1d5db');

    // Labels with background (only for important nodes or on hover)
    node.append('text')
      .attr('class', 'label-bg')
      .text(d => {{
        const parts = d.id.split('/');
        return parts[parts.length - 1];
      }})
      .attr('x', d => Math.max(4, rScale(linkCount(d) || 0)) + 4)
      .attr('y', 4);

    node.append('text')
      .text(d => {{
        const parts = d.id.split('/');
        return parts[parts.length - 1];
      }})
      .attr('x', d => Math.max(4, rScale(linkCount(d) || 0)) + 4)
      .attr('y', 4)
      .attr('fill', '#333');

    // Full path on hover
    node.append('title').text(d => d.id);

    // Highlight on hover
    node.on('mouseenter', function(event, d) {{
      tooltip.style('opacity', 1)
        .style('left', (event.offsetX + 12) + 'px')
        .style('top', (event.offsetY - 6) + 'px')
        .text(d.id);
      d3.select(this).select('circle').attr('stroke-width', 3);
    }}).on('mouseleave', function() {{
      tooltip.style('opacity', 0);
      d3.select(this).select('circle').attr('stroke-width', 1.5);
    }});

    // Drag
    node.call(d3.drag()
      .on('start', (event, d) => {{
        if (!event.active) simulation.alphaTarget(0.3).restart();
        d.fx = d.x; d.fy = d.y;
      }})
      .on('drag', (event, d) => {{ d.fx = event.x; d.fy = event.y; }})
      .on('end', (event, d) => {{
        if (!event.active) simulation.alphaTarget(0);
        d.fx = null; d.fy = null;
      }}));

    // Simulation
    const nodeCount = visibleNodes.length;
    const charge = nodeCount > 200 ? -80 : nodeCount > 100 ? -120 : -200;

    simulation = d3.forceSimulation(visibleNodes)
      .force('link', d3.forceLink(visibleEdges.map(e => ({{ source: e.from, target: e.to }})))
        .id(d => d.id)
        .distance(d => Math.min(180, 40 + (linkCount(d.source) || 0) * 3)))
      .force('charge', d3.forceManyBody().strength(charge))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide(10))
      .alphaDecay(0.02);

    simulation.on('tick', () => {{
      link
        .attr('x1', d => d.source.x)
        .attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x)
        .attr('y2', d => d.target.y);
      node.attr('transform', d => `translate(${{d.x}},${{d.y}})`);
    }});

    // Legend
    const legend = d3.select('#graph-legend');
    legend.html('');
    dirs.forEach(dir => {{
      const count = visibleNodes.filter(n => n.top_dir === dir).length;
      legend.append('span')
        .html(`<span style="display:inline-block;width:10px;height:10px;border-radius:50%;background:${{dirColors(dir)}};margin-right:4px;"></span> ${{dir}} (${{count}})`);
    }});
  }}

  // ── Controls ────────────────────────────────────────────
  let currentLimit = parseInt(document.getElementById('graph-limit').value) || 100;
  let currentFilter = document.getElementById('graph-filter').value;

  function updateGraph() {{
    render(currentLimit, currentFilter);
  }}

  document.getElementById('graph-limit').addEventListener('change', function() {{
    currentLimit = parseInt(this.value) || 0;
    updateGraph();
  }});

  let filterTimeout;
  document.getElementById('graph-filter').addEventListener('input', function() {{
    clearTimeout(filterTimeout);
    filterTimeout = setTimeout(() => {{
      currentFilter = this.value;
      updateGraph();
    }}, 300);
  }});

  // Initial render
  updateGraph();

  window.addEventListener('resize', updateGraph);
</script>

</body>
</html>"##,
        source_lang = source_lang,
        target_lang = target_lang,
        source_root = source_root,
        files_analyzed = files_analyzed,
        dep_count = dep_count,
        edge_count = dag.edges.len(),
        cycle_class = if cycles.has_cycles { "red" } else { "green" },
        cycle_count = cycles.cycles.len() + cycles.self_loops.len(),
        cycle_html = build_cycles_section(cycles),
        deps_table = build_deps_table(dependencies),
        compat_table = build_compat_table(&compat_json_str),
        nodes_json = nodes_json,
        edges_json = edges_json,
    );

    std::fs::write(output_dir.join("index.html"), html)?;
    Ok(())
}

fn build_cycles_section(cycles: &CycleDetectionResult) -> String {
    if !cycles.has_cycles {
        return r#"<div class="cycles-ok"><strong>No cycles detected.</strong> The dependency graph is acyclic.</div>"#.to_string();
    }

    let mut parts = String::new();
    parts.push_str(&format!(
        r#"<div class="cycles-warning"><strong>{} cycle(s)</strong> and <strong>{} self-loop(s)</strong> detected.</div>"#,
        cycles.cycles.len(),
        cycles.self_loops.len()
    ));

    if !cycles.self_loops.is_empty() {
        parts.push_str("<h3>Self-loops</h3><ul>");
        for sl in &cycles.self_loops {
            parts.push_str(&format!("<li>{}</li>", sl));
        }
        parts.push_str("</ul>");
    }

    for (i, cycle) in cycles.cycles.iter().enumerate() {
        parts.push_str(&format!("<h3>Cycle {}</h3><ol>", i + 1));
        for node in &cycle.nodes {
            parts.push_str(&format!("<li>{}</li>", node));
        }
        parts.push_str("</ol>");
    }

    parts
}

fn build_deps_table(deps: &[ResolvedDependency]) -> String {
    if deps.is_empty() {
        return "<p style='color: #888;'>No external dependencies found.</p>".to_string();
    }

    let mut rows = String::new();
    for dep in deps {
        rows.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td></tr>",
            html_escape(&dep.name),
            html_escape(&dep.version),
            dep.dependencies.join(", ")
        ));
    }

    format!(
        r#"<table><thead><tr><th>Package</th><th>Version</th><th>Sub-dependencies</th></tr></thead><tbody>{}</tbody></table>"#,
        rows
    )
}

fn build_compat_table(compat_json_str: &str) -> String {
    let compat: Value = serde_json::from_str(compat_json_str).unwrap_or(Value::Null);

    match &compat {
        Value::Object(map) if !map.is_empty() => {
            let mut rows = String::new();
            for (name, info) in map {
                let eq = info["equivalent"].as_str().unwrap_or("—");
                let compat_level = info["compatibility"].as_str().unwrap_or("unknown");
                let note = info["note"].as_str().unwrap_or("");
                let badge_class = match compat_level {
                    "full" => "full",
                    "partial" => "partial",
                    "none" => "none",
                    _ => "unknown",
                };
                rows.push_str(&format!(
                    r#"<tr><td>{}</td><td>{}</td><td><span class="badge {}">{}</span></td><td>{}</td></tr>"#,
                    html_escape(name),
                    html_escape(eq),
                    badge_class,
                    compat_level,
                    html_escape(note)
                ));
            }
            format!(
                r#"<table><thead><tr><th>Dependency</th><th>Equivalent</th><th>Compatibility</th><th>Note</th></tr></thead><tbody>{}</tbody></table>"#,
                rows
            )
        }
        _ => "<p style='color: #888;'>No compatibility data available.</p>".to_string(),
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
