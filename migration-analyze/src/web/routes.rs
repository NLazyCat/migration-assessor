use axum::extract::{Path, State};
use axum::response::Html;
use axum::Json;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::templates;

#[derive(Clone)]
pub struct AppState {
    pub report_dir: PathBuf,
}

// ── Read helpers ───────────────────────────────────────────────────

fn read_json(dir: &PathBuf, path: &str) -> Option<Value> {
    let full = dir.join(path);
    let content = std::fs::read_to_string(full).ok()?;
    serde_json::from_str(&content).ok()
}

fn read_scores(dir: &PathBuf) -> Vec<Value> {
    read_json(dir, "scores.json").and_then(|v| v.as_array().cloned()).unwrap_or_default()
}

fn collect_symbols(dir: &PathBuf) -> Vec<(String, Value)> {
    let index = match read_json(dir, "index.json") {
        Some(v) => v,
        None => return Vec::new(),
    };
    let map = match index.as_object() {
        Some(m) => m,
        None => return Vec::new(),
    };
    let mut results = Vec::new();
    for (module, info) in map {
        let path = info.get("symbols_path").and_then(|v| v.as_str()).unwrap_or("");
        if path.is_empty() { continue; }
        if let Some(val) = read_json(dir, path) {
            results.push((module.clone(), val));
        }
    }
    // Sort by module name for deterministic order
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results
}

fn load_file_index(dir: &PathBuf) -> HashMap<String, Value> {
    read_json(dir, "index.json")
        .and_then(|v| v.as_object().cloned())
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

// ── API handlers ───────────────────────────────────────────────────

pub async fn api_project(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(read_json(&state.report_dir, "project.json"))
}

pub async fn api_files(State(state): State<Arc<AppState>>) -> Json<Vec<(String, Value)>> {
    Json(collect_symbols(&state.report_dir))
}

pub async fn api_deps(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(read_json(&state.report_dir, "external-deps/resolved.json"))
}

pub async fn api_compat(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(read_json(&state.report_dir, "external-deps/compatibility.json"))
}

pub async fn api_graph(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(read_json(&state.report_dir, "internal-deps/dag.json"))
}

pub async fn api_scores(State(state): State<Arc<AppState>>) -> Json<Vec<Value>> {
    Json(read_scores(&state.report_dir))
}

pub async fn api_references(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(read_json(&state.report_dir, "references/reverse.json"))
}

pub async fn api_file_references(
    State(state): State<Arc<AppState>>,
    Path(file): Path<String>,
) -> Json<Option<Value>> {
    // file comes as path segments joined by /, e.g. "src/types.ts"
    let fwd = read_json(&state.report_dir, &format!("references/by-dir/{}.forward.json", file));
    let rev = read_json(&state.report_dir, &format!("references/by-dir/{}.reverse.json", file));
    Json(Some(serde_json::json!({ "forward": fwd, "reverse": rev })))
}

// ── Page handlers ──────────────────────────────────────────────────

pub async fn page_shell() -> Html<String> {
    Html(templates::shell())
}

pub async fn page_overview(State(state): State<Arc<AppState>>) -> Html<String> {
    let project = read_json(&state.report_dir, "project.json");
    let scores = read_scores(&state.report_dir);
    let deps = read_json(&state.report_dir, "external-deps/resolved.json");
    let symbols = collect_symbols(&state.report_dir);
    Html(templates::overview(&project, &scores, &deps, &symbols))
}

pub async fn page_files(State(state): State<Arc<AppState>>) -> Html<String> {
    let symbols = collect_symbols(&state.report_dir);
    Html(templates::files(&symbols))
}

pub async fn page_deps(State(state): State<Arc<AppState>>) -> Html<String> {
    let deps = read_json(&state.report_dir, "external-deps/resolved.json");
    let compat = read_json(&state.report_dir, "external-deps/compatibility.json");
    Html(templates::deps(&deps, &compat))
}

pub async fn page_scores(State(state): State<Arc<AppState>>) -> Html<String> {
    let scores = read_scores(&state.report_dir);
    Html(templates::scores(&scores))
}

pub async fn page_graph() -> Html<String> {
    Html(templates::graph_page())
}

pub async fn page_report_ref(State(state): State<Arc<AppState>>) -> Html<String> {
    let files = load_file_index(&state.report_dir);
    let file_list: Vec<String> = files.into_keys().collect();
    Html(templates::report_ref(&file_list))
}
