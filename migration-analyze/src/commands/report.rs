use std::path::Path;

use migration_core::deps::ResolvedDependency;
use migration_core::graph::CycleDetectionResult;
use migration_core::graph::DependencyGraph;
use serde_json::Value;

use self::html::{build_html, format_cycles_detail, get_bilingual_js, html_escape};

mod html;

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

    let compat_path = output_dir.join("external").join("compatibility.json");
    let compat_json: Value = if compat_path.exists() {
        let text = std::fs::read_to_string(&compat_path)?;
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

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

    let uncut_path = output_dir.join("boundaries").join("uncut-surfaces.json");
    let uncut_json: Value = if uncut_path.exists() {
        let text = std::fs::read_to_string(&uncut_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

    let overview_path = output_dir.join("overview.json");
    let overview_json: Value = if overview_path.exists() {
        let text = std::fs::read_to_string(&overview_path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or(Value::Null)
    } else {
        Value::Null
    };

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

    let bilingual_js = get_bilingual_js();

    let html = build_html(
        source_repo,
        source_lang,
        target_lang,
        source_root,
        generated_at,
        files_analyzed,
        dep_count,
        dag.edges.len(),
        total_symbols,
        dag.nodes.len(),
        if cycles.has_cycles { "#dc2626" } else { "#16a34a" },
        if cycles.has_cycles { "red" } else { "green" },
        cycles.cycles.len() + cycles.self_loops.len(),
        &format_cycles_detail(cycles),
        if has_boundaries { "#7c3aed" } else { "#9ca3af" },
        if has_boundaries { "purple" } else { "" },
        &if has_boundaries {
            format!("{} layers in architecture", layers_count)
        } else {
            "Not analyzed (run `boundaries`)".to_string()
        },
        layers_count,
        &build_deps_table(dependencies, &compat_json),
        &build_recs_section(&recs_json),
        &build_cycles_section(cycles),
        &build_boundary_section(&layers_json, &uncut_json),
        &build_refs_overview(&overview_json),
        &build_api_section(output_dir, &overview_json),
        bilingual_js,
        &nodes_json,
        &edges_json,
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
    if let Value::Array(arr) = uncut && !arr.is_empty() {
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