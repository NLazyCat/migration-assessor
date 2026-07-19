use axum::Json;
use axum::extract::{Path, State};
use axum::response::Html;
use migration_core::output_paths;
use serde_json::Value;
use std::collections::HashMap;

use std::sync::Arc;

use crate::commands::context::ProjectContext;

use super::templates;

#[derive(Clone)]
pub struct AppState {
    pub ctx: Arc<ProjectContext>,
}

// ── Read helpers ───────────────────────────────────────────────────

fn read_json(ctx: &ProjectContext, path: &str) -> Option<Value> {
    ctx.load_json::<Value>(path).ok()
}

fn read_scores(ctx: &ProjectContext) -> Vec<Value> {
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
        if let Some(val) = read_json(ctx, path) {
            results.push((module.clone(), val));
        }
    }
    // Sort by module name for deterministic order
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

fn load_file_index(ctx: &ProjectContext) -> HashMap<String, Value> {
    ctx.index()
        .ok()
        .and_then(|v| v.as_object().cloned())
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

// ── API handlers ───────────────────────────────────────────────────

pub async fn api_project(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(state.ctx.project_meta().ok())
}

pub async fn api_files(State(state): State<Arc<AppState>>) -> Json<Vec<(String, Value)>> {
    Json(collect_symbols(&state.ctx))
}

pub async fn api_deps(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(read_json(&state.ctx, output_paths::external::PACKAGES))
}

pub async fn api_compat(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(read_json(&state.ctx, output_paths::external::COMPATIBILITY))
}

pub async fn api_graph(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(state.ctx.dag().ok())
}

pub async fn api_scores(State(state): State<Arc<AppState>>) -> Json<Vec<Value>> {
    Json(read_scores(&state.ctx))
}

pub async fn api_references(State(state): State<Arc<AppState>>) -> Json<Value> {
    let mut files = Vec::new();
    let dir = state.ctx.report_dir.join("references").join("forward");
    collect_shard_files(&dir, &dir, &mut files);
    files.sort();
    files.dedup();
    Json(serde_json::json!({ "files": files }))
}

fn collect_shard_files(base: &std::path::Path, current: &std::path::Path, out: &mut Vec<String>) {
    if let Ok(entries) = std::fs::read_dir(current) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                collect_shard_files(base, &path, out);
            } else if let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name.ends_with(".json")
            {
                let relative = path.strip_prefix(base).unwrap_or(&path);
                let relative_str = relative.to_string_lossy().replace('\\', "/");
                let base = relative_str.trim_end_matches(".json").to_string();
                out.push(base);
            }
        }
    }
}

pub async fn api_file_references(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Json<Option<Value>> {
    // file comes as path segments joined by /, e.g. "src/types.ts"
    let fwd = read_json(&state.ctx, &output_paths::references::forward_for(&file));
    let rev = read_json(&state.ctx, &output_paths::references::reverse_for(&file));
    Json(Some(serde_json::json!({ "forward": fwd, "reverse": rev })))
}

// ── Page handlers ──────────────────────────────────────────────────

pub async fn page_shell() -> Html<String> {
    Html(templates::shell())
}

pub async fn page_overview(State(state): State<Arc<AppState>>) -> Html<String> {
    let project = state.ctx.project_meta().ok();
    let scores = read_scores(&state.ctx);
    let deps = read_json(&state.ctx, output_paths::external::PACKAGES);
    let symbols = collect_symbols(&state.ctx);
    Html(templates::overview(&project, &scores, &deps, &symbols))
}

pub async fn page_files(State(state): State<Arc<AppState>>) -> Html<String> {
    let symbols = collect_symbols(&state.ctx);
    Html(templates::files(&symbols))
}

pub async fn page_deps(State(state): State<Arc<AppState>>) -> Html<String> {
    let deps = read_json(&state.ctx, output_paths::external::PACKAGES);
    let compat = read_json(&state.ctx, output_paths::external::COMPATIBILITY);
    Html(templates::deps(&deps, &compat))
}

pub async fn page_scores(State(state): State<Arc<AppState>>) -> Html<String> {
    let scores = read_scores(&state.ctx);
    Html(templates::scores(&scores))
}

pub async fn page_graph() -> Html<String> {
    Html(templates::graph_page())
}

pub async fn page_report_ref(State(state): State<Arc<AppState>>) -> Html<String> {
    let files = load_file_index(&state.ctx);
    let file_list: Vec<String> = files.into_keys().collect();
    Html(templates::report_ref(&file_list))
}

fn load_boundaries(ctx: &ProjectContext) -> Option<Value> {
    let mut layers = read_json(ctx, output_paths::boundaries::LAYERS)?;
    let uncut = read_json(ctx, output_paths::boundaries::UNCUT_SURFACES)?;
    let obj = layers.as_object_mut()?;
    if let Some(surface) = uncut.get("uncut_surface") {
        obj.insert("uncut_surface".to_string(), surface.clone());
    }
    Some(layers)
}

pub async fn page_boundaries(State(state): State<Arc<AppState>>) -> Html<String> {
    let boundaries = load_boundaries(&state.ctx);
    Html(templates::boundaries(&boundaries))
}

pub async fn api_boundaries(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(load_boundaries(&state.ctx))
}
