use serde_json::Value;

// ── Shell / Layout ──────────────────────────────────────────────────

pub fn shell() -> String {
    let mut h = String::new();
    push(&mut h, "<!DOCTYPE html><html lang=\"en\"><head><meta charset=\"UTF-8\">");
    push(&mut h, "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">");
    push(&mut h, "<title>Migration Analyzer</title>");
    push(&mut h, "<script src=\"https://unpkg.com/htmx.org@2\"></script>");
    push(&mut h, "<script src=\"https://d3js.org/d3.v7.min.js\"></script>");
    push(&mut h, "<style>");
    push(&mut h, "*{margin:0;padding:0;box-sizing:border-box}");
    push(&mut h, "body{font-family:system-ui,-apple-system,sans-serif;background:#f5f5f5;color:#222;display:flex;min-height:100vh}");
    push(&mut h, "nav{width:220px;background:#1a1a2e;color:#eee;padding:1.5rem 0;display:flex;flex-direction:column}");
    push(&mut h, "nav h1{font-size:1rem;padding:0 1rem 1rem;border-bottom:1px solid #333;margin-bottom:0.5rem}");
    push(&mut h, "nav a{color:#ccc;text-decoration:none;padding:0.5rem 1rem;display:block;font-size:0.9rem}");
    push(&mut h, "nav a:hover{background:#16213e;color:#fff}");
    push(&mut h, "main{flex:1;padding:2rem;max-width:1200px}");
    push(&mut h, "h2{margin-bottom:1rem}");
    push(&mut h, ".card{background:#fff;border-radius:8px;padding:1.25rem;box-shadow:0 1px 3px rgba(0,0,0,0.1);margin-bottom:1.25rem}");
    push(&mut h, ".grid{display:grid;grid-template-columns:repeat(auto-fit,minmax(200px,1fr));gap:1rem}");
    push(&mut h, ".stat{text-align:center}");
    push(&mut h, ".stat .num{font-size:2rem;font-weight:700;color:#1a1a2e}");
    push(&mut h, ".stat .label{font-size:0.8rem;color:#666;margin-top:0.25rem}");
    push(&mut h, "table{width:100%;border-collapse:collapse;font-size:0.85rem}");
    push(&mut h, "th,td{text-align:left;padding:0.5rem;border-bottom:1px solid #eee}");
    push(&mut h, "th{font-weight:600;color:#555}");
    push(&mut h, "tr:hover{background:#fafafa}");
    push(&mut h, "pre{background:#f8f8f8;padding:0.75rem;border-radius:4px;overflow-x:auto;font-size:0.8rem}");
    push(&mut h, ".badge{display:inline-block;padding:0.15rem 0.5rem;border-radius:99px;font-size:0.75rem;font-weight:600}");
    push(&mut h, ".badge-ok{background:#d4edda;color:#155724}");
    push(&mut h, ".badge-warn{background:#fff3cd;color:#856404}");
    push(&mut h, ".badge-err{background:#f8d7da;color:#721c24}");
    push(&mut h, "#graph{width:100%;height:600px;background:#fff}");
    push(&mut h, "</style></head><body>");
    push(&mut h, "<nav><h1>Migration Analyzer</h1>");
    push(&mut h, "<a href=\"/overview\" hx-get=\"/overview\" hx-target=\"#content\" hx-push-url=\"true\">Overview</a>");
    push(&mut h, "<a href=\"/files\" hx-get=\"/files\" hx-target=\"#content\" hx-push-url=\"true\">Files</a>");
    push(&mut h, "<a href=\"/deps\" hx-get=\"/deps\" hx-target=\"#content\" hx-push-url=\"true\">Dependencies</a>");
    push(&mut h, "<a href=\"/scores\" hx-get=\"/scores\" hx-target=\"#content\" hx-push-url=\"true\">Scores</a>");
    push(&mut h, "<a href=\"/graph\" hx-get=\"/graph\" hx-target=\"#content\" hx-push-url=\"true\">Dep Graph</a>");
    push(&mut h, "<a href=\"/report-ref\" hx-get=\"/report-ref\" hx-target=\"#content\" hx-push-url=\"true\">References</a>");
    push(&mut h, "<a href=\"/boundaries\" hx-get=\"/boundaries\" hx-target=\"#content\" hx-push-url=\"true\">Boundaries</a>");
    push(&mut h, "</nav><main id=\"content\"></main></body></html>");
    h
}

fn push(s: &mut String, v: &str) { s.push_str(v); }

// ── Overview ────────────────────────────────────────────────────────

pub fn overview(project: &Option<Value>, scores: &[Value], deps: &Option<Value>, symbols: &[(String, Value)]) -> String {
    let mut h = String::new();
    page_head(&mut h, "Overview");

    stat_card(&mut h, "Source Files", &symbols.len().to_string());
    stat_card(&mut h, "Dependencies", &dep_count(deps));
    stat_card(&mut h, "Scores", &scores.len().to_string());

    push(&mut h, "</div>");

    if let Some(p) = project {
        card(&mut h, "Project", &format!("<pre>{}</pre>", serde_json::to_string_pretty(p).unwrap_or_default()));
    }

    if let Some(s) = scores.last() {
        if let Some(score) = s.get("score").and_then(|v| v.as_f64()) {
            card(&mut h, "Latest Migration Score", &format!(
                "<div class=\"stat\"><div class=\"num\">{:.1}%</div></div>", score * 100.0
            ));
        }
    }

    h
}

// ── Files page ──────────────────────────────────────────────────────

pub fn files(symbols: &[(String, Value)]) -> String {
    let mut h = String::new();
    page_head(&mut h, "Source Files");

    push(&mut h, "<div class=\"card\"><table><thead><tr>");
    push(&mut h, "<th>Module</th><th>Functions</th><th>Classes</th><th>Exports</th><th>Imports</th>");
    push(&mut h, "</tr></thead><tbody>");

    for (module, data) in symbols {
        let funcs = array_len(data, "functions");
        let classes = array_len(data, "classes");
        let exports = array_len(data, "exports");
        let imports = array_len(data, "imports");
        push(&mut h, &format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            module, funcs, classes, exports, imports
        ));
    }

    push(&mut h, "</tbody></table></div>");
    h
}

// ── Deps page ───────────────────────────────────────────────────────

pub fn deps(deps: &Option<Value>, compat: &Option<Value>) -> String {
    let mut h = String::new();
    page_head(&mut h, "External Dependencies");

    let deps_arr = deps.as_ref().and_then(|d| d.as_array());
    let compat_map = compat.as_ref().and_then(|c| c.as_object());

    push(&mut h, "<div class=\"card\"><table><thead><tr>");
    push(&mut h, "<th>Package</th><th>Version</th><th>Status</th><th>Rust Alternative</th>");
    push(&mut h, "</tr></thead><tbody>");

    if let Some(arr) = deps_arr {
        for dep in arr {
            let name = dep.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let ver = dep.get("version").and_then(|v| v.as_str()).unwrap_or("");
            let compat_info = compat_map.and_then(|m| m.get(name));
            let (status, alt) = compat_info
                .map(|c| (
                    c.get("status").and_then(|v| v.as_str()).unwrap_or("unknown"),
                    c.get("alternative").and_then(|v| v.as_str()).unwrap_or(""),
                ))
                .unwrap_or(("unknown", ""));
            let badge = match status {
                "available" => "badge badge-ok",
                "partial" => "badge badge-warn",
                _ => "badge badge-err",
            };
            push(&mut h, &format!(
                "<tr><td>{}</td><td>{}</td><td><span class=\"{}\">{}</span></td><td>{}</td></tr>",
                name, ver, badge, status, alt
            ));
        }
    }

    push(&mut h, "</tbody></table></div>");
    h
}

// ── Scores page ─────────────────────────────────────────────────────

pub fn scores(scores: &[Value]) -> String {
    let mut h = String::new();
    page_head(&mut h, "Migration Scores");

    if scores.is_empty() {
        push(&mut h, "<div class=\"card\">No scores recorded yet.</div>");
        return h;
    }

    push(&mut h, "<div class=\"card\"><table><thead><tr>");
    push(&mut h, "<th>#</th><th>Score</th><th>Files Migrated</th><th>Files Total</th><th>Timestamp</th>");
    push(&mut h, "</tr></thead><tbody>");

    for (i, s) in scores.iter().enumerate() {
        let score_val = s.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let migrated = s.get("files_migrated").and_then(|v| v.as_u64()).unwrap_or(0);
        let total = s.get("files_total").and_then(|v| v.as_u64()).unwrap_or(0);
        let ts = s.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        push(&mut h, &format!(
            "<tr><td>{}</td><td>{:.1}%</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            i + 1, score_val * 100.0, migrated, total, ts
        ));
    }

    push(&mut h, "</tbody></table></div>");
    h
}

// ── D3 Graph page ──────────────────────────────────────────────────

pub fn graph_page() -> String {
    let mut h = String::new();
    page_head(&mut h, "Dependency Graph");
    push(&mut h, "<div class=\"card\">");
    push(&mut h, "<div id=\"graph\"></div>");
    push(&mut h, "<script>");
    push(&mut h, "fetch('/api/graph').then(r=>r.json()).then(data=>{");
    push(&mut h, "if(!data||!data.nodes)return;");
    push(&mut h, "const w=document.getElementById('graph').clientWidth;");
    push(&mut h, "const h=600;");
    push(&mut h, "const sim=d3.forceSimulation(data.nodes).force('link',d3.forceLink(data.edges).id(d=>d.id).distance(80))");
    push(&mut h, ".force('charge',d3.forceManyBody().strength(-200)).force('center',d3.forceCenter(w/2,h/2));");
    push(&mut h, "const svg=d3.select('#graph').append('svg').attr('width',w).attr('height',h);");
    push(&mut h, "const link=svg.append('g').selectAll('line').data(data.edges).join('line').attr('stroke','#aaa');");
    push(&mut h, "const node=svg.append('g').selectAll('circle').data(data.nodes).join('circle').attr('r',6).attr('fill','#1a1a2e');");
    push(&mut h, "const label=svg.append('g').selectAll('text').data(data.nodes).join('text').text(d=>d.id).attr('font-size','10px').attr('dx',8).attr('dy',3);");
    push(&mut h, "sim.on('tick',()=>{link.attr('x1',d=>d.source.x).attr('y1',d=>d.source.y).attr('x2',d=>d.target.x).attr('y2',d=>d.target.y);");
    push(&mut h, "node.attr('cx',d=>d.x).attr('cy',d=>d.y);label.attr('x',d=>d.x).attr('y',d=>d.y);});");
    push(&mut h, "});");
    push(&mut h, "</script></div>");
    h
}

// ── Report / References page ────────────────────────────────────────

pub fn report_ref(files: &[String]) -> String {
    let mut h = String::new();
    page_head(&mut h, "References");
    push(&mut h, "<div class=\"card\">");
    push(&mut h, "<label for=\"file-select\">File:</label> ");
    push(&mut h, "<select id=\"file-select\" onchange=\"loadFileRefs(this.value)\">");
    push(&mut h, "<option value=\"\">-- select file --</option>");
    for f in files {
        push(&mut h, &format!("<option value=\"{}\">{}</option>", f, f));
    }
    push(&mut h, "</select>");
    push(&mut h, "</div>");
    push(&mut h, "<div id=\"ref-display\"></div>");
    push(&mut h, "<script>");
    push(&mut h, "function loadFileRefs(file){");
    push(&mut h, "if(!file){document.getElementById('ref-display').innerHTML='';return;}");
    push(&mut h, "fetch('/api/references/'+file).then(r=>r.json()).then(data=>{");
    push(&mut h, "let html='<div class=\"card\"><h3>Forward (what this file imports)</h3>';");
    push(&mut h, "if(data.forward){html+='<pre>'+JSON.stringify(data.forward,null,2)+'</pre>';}else{html+='<p>None</p>';}");
    push(&mut h, "html+='</div><div class=\"card\"><h3>Reverse (what imports this file)</h3>';");
    push(&mut h, "if(data.reverse){html+='<pre>'+JSON.stringify(data.reverse,null,2)+'</pre>';}else{html+='<p>None</p>';}");
    push(&mut h, "html+='</div>';document.getElementById('ref-display').innerHTML=html;");
    push(&mut h, "});}");
    push(&mut h, "</script>");
    h
}

// ── Helpers ─────────────────────────────────────────────────────────

fn card(s: &mut String, title: &str, body: &str) {
    push(s, &format!("<div class=\"card\"><h3>{}</h3>{}</div>", title, body));
}

fn page_head(s: &mut String, title: &str) {
    push(s, &format!("<h2>{}</h2><div class=\"grid\">", title));
}

fn stat_card(s: &mut String, label: &str, value: &str) {
    push(s, &format!("<div class=\"stat\"><div class=\"num\">{}</div><div class=\"label\">{}</div></div>", value, label));
}

fn dep_count(deps: &Option<Value>) -> String {
    deps.as_ref()
        .and_then(|d| d.as_array())
        .map(|a| a.len().to_string())
        .unwrap_or_else(|| "0".to_string())
}

fn array_len(data: &Value, key: &str) -> String {
    data.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.len().to_string())
        .unwrap_or_else(|| "0".to_string())
}

// ── Boundaries page ─────────────────────────────────────────────────

pub fn boundaries(data: &Option<Value>) -> String {
    let mut h = String::new();

    push(&mut h, "<h2>Interface Boundaries</h2>");

    let report = match data {
        Some(Value::Object(m)) => m,
        _ => {
            push(&mut h, "<div class=\"card\"><p>No interface boundary data. Run <code>migration-analyze boundaries</code> first.</p></div>");
            return h;
        }
    };

    let layers = report.get("layers").and_then(|v| v.as_array());
    let uncut = report.get("uncut_surface").and_then(|v| v.as_array());
    let total_layers = report.get("total_layers").and_then(|v| v.as_u64()).unwrap_or(0);

    // Summary stats
    push(&mut h, "<div class=\"grid\">");
    stat_card(&mut h, "Total Layers", &total_layers.to_string());
    stat_card(
        &mut h,
        "Uncut Interfaces",
        &uncut.map(|a| a.len()).unwrap_or(0).to_string(),
    );
    push(&mut h, "</div>");

    // Layers
    if let Some(layers) = layers {
        for layer in layers.iter().rev() {
            let level = layer.get("level").and_then(|v| v.as_u64()).unwrap_or(0);
            let desc = layer.get("description").and_then(|v| v.as_str()).unwrap_or("");
            let total_pub = layer.get("total_public_symbols").and_then(|v| v.as_u64()).unwrap_or(0);
            let modules = layer.get("modules").and_then(|v| v.as_array());

            push(&mut h, "<div class=\"card\" style=\"margin-top:1.5rem\">");
            push(
                &mut h,
                &format!(
                    "<h3>Layer {}: {} <span class=\"badge badge-ok\">{} public</span></h3>",
                    level, desc, total_pub
                ),
            );

            if let Some(modules) = modules {
                push(&mut h, "<table><tr><th>Module</th><th>In</th><th>Out</th><th>Public</th><th>Internal</th><th>Score</th></tr>");
                for m in modules {
                    let name = m.get("module").and_then(|v| v.as_str()).unwrap_or("");
                    let in_d = m.get("in_degree").and_then(|v| v.as_u64()).unwrap_or(0);
                    let out_d = m.get("out_degree").and_then(|v| v.as_u64()).unwrap_or(0);
                    let score = m.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let pub_syms: Vec<String> = m
                        .get("public_symbols")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                        .unwrap_or_default();
                    let int_syms: Vec<String> = m
                        .get("internal_symbols")
                        .and_then(|v| v.as_array())
                        .map(|a| a.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                        .unwrap_or_default();

                    let pub_display = if pub_syms.len() <= 3 {
                        pub_syms.join(", ")
                    } else {
                        format!("{} ({} total)", pub_syms[..3].join(", "), pub_syms.len())
                    };
                    let int_display = if int_syms.len() <= 2 {
                        int_syms.join(", ")
                    } else {
                        format!("{} ({} total)", int_syms[..2].join(", "), int_syms.len())
                    };

                    push(&mut h, &format!(
                        "<tr><td><strong>{}</strong></td><td>{}</td><td>{}</td><td style=\"color:#4ecdc4\">{}</td><td style=\"color:#888\">{}</td><td>{:.1}</td></tr>",
                        name, in_d, out_d, pub_display, int_display, score
                    ));
                }
                push(&mut h, "</table>");
            }

            push(&mut h, "</div>");
        }
    }

    // Uncut surfaces
    if let Some(uncut) = uncut {
        if !uncut.is_empty() {
            push(&mut h, "<div class=\"card\" style=\"margin-top:2rem;border-left:3px solid #e94560\">");
            push(&mut h, "<h3>Uncut Cross-Layer Interfaces</h3>");
            push(&mut h, "<p style=\"color:#888;font-size:0.85rem\">These symbols cross layer boundaries and define the cut plane for incremental migration.</p>");
            push(&mut h, "<table><tr><th>Direction</th><th>Consumer</th><th>Provider</th><th>Symbol</th><th>Kind</th></tr>");

            // Show first 60 entries to avoid huge pages
            let limit = 60.min(uncut.len());
            for entry in uncut.iter().take(limit) {
                let dir = entry.get("direction").and_then(|v| v.as_str()).unwrap_or("");
                let consumer = entry.get("consumer_module").and_then(|v| v.as_str()).unwrap_or("");
                let provider = entry.get("provider_module").and_then(|v| v.as_str()).unwrap_or("");
                let symbol = entry.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let kind = entry.get("kind").and_then(|v| v.as_str()).unwrap_or("");
                push(&mut h, &format!(
                    "<tr><td><code>{}</code></td><td>{}</td><td>{}</td><td><code>{}</code></td><td>{}</td></tr>",
                    dir, consumer, provider, symbol, kind
                ));
            }

            if uncut.len() > limit {
                push(&mut h, &format!(
                    "<tr><td colspan=\"5\" style=\"color:#888\">... and {} more</td></tr>",
                    uncut.len() - limit
                ));
            }

            push(&mut h, "</table></div>");
        }
    }

    h
}
