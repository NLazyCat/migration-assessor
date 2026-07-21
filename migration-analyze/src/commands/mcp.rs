use clap::Args;
use migration_core::db;
use migration_core::manifest;
use migration_core::output_paths;
use migration_core::task_queue::TaskQueue;
use serde::{Deserialize, Serialize};
use std::io::BufRead;

use crate::commands::context::ProjectContext;
use crate::commands::{analyze, diff, resolve_project_path};

// ── Args ──────────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct McpArgs {
    /// Project root directory (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: String,
}

// ── JSON-RPC 2.0 types ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[expect(dead_code)]
    jsonrpc: String,
    id: serde_json::Value,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
}

// ── Public entrypoint ─────────────────────────────────────────────────────

pub fn run(args: &McpArgs) -> anyhow::Result<()> {
    let project_root = resolve_project_path(&args.path);

    // Load project context upfront (validates migration.toml, report dir, etc.)
    let ctx = ProjectContext::load(&project_root).map_err(|e| {
        anyhow::anyhow!(
            "Failed to load project context: {}\n\
             Run 'migration-analyze analyze' first in a migration project.",
            e
        )
    })?;

    run_stdin_stdout(&ctx)
}

// ── Stdin/stdout transport ────────────────────────────────────────────────

fn run_stdin_stdout(ctx: &ProjectContext) -> anyhow::Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Error reading stdin: {}", e);
                break;
            }
        };
        if line.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let error_resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: serde_json::Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("Parse error: {}", e),
                        data: None,
                    }),
                };
                let json = serde_json::to_string(&error_resp)?;
                let mut out = stdout.lock();
                use std::io::Write;
                writeln!(out, "{}", json)?;
                out.flush()?;
                continue;
            }
        };

        let response = handle_request(ctx, &request);
        let json = serde_json::to_string(&response)?;
        let mut out = stdout.lock();
        use std::io::Write;
        writeln!(out, "{}", json)?;
        out.flush()?;
    }

    Ok(())
}

// ── Request dispatch ──────────────────────────────────────────────────────

fn handle_request(ctx: &ProjectContext, request: &JsonRpcRequest) -> JsonRpcResponse {
    let id = request.id.clone();

    let result = match request.method.as_str() {
        "analyze" => handle_analyze(ctx, &request.params),
        "get_spec" => handle_get_spec(ctx, &request.params),
        "verify" => handle_verify(ctx, &request.params),
        "get_next_task" => handle_get_next_task(ctx),
        "verify_file" => handle_verify_file(ctx, &request.params),
        "list_tasks" => handle_list_tasks(ctx, &request.params),
        "get_progress" => handle_get_progress(ctx),
        "diff" => handle_diff(ctx, &request.params),
        "verify_migration" => handle_verify_migration(ctx, &request.params),
        "task_complete" => handle_task_complete(ctx, &request.params),
        _ => {
            return JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                }),
            };
        }
    };

    match result {
        Ok(val) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(val),
            error: None,
        },
        Err(e) => JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code: -32000,
                message: format!("{}", e),
                data: None,
            }),
        },
    }
}

// ── Method handlers ───────────────────────────────────────────────────────

fn handle_analyze(_ctx: &ProjectContext, _params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    // Run analyze with default args
    let args = analyze::AnalyzeArgs {
        path: ".".to_string(),
        output: None,
        strict: false,
        score_weights: None,
    };
    analyze::run(&args)?;

    // Read and return the manifest
    let project_root = resolve_project_path(".");
    let ctx = ProjectContext::load(&project_root)?;
    let manifest: serde_json::Value = ctx.load_json(output_paths::MANIFEST)?;
    Ok(serde_json::json!({ "status": "ok", "manifest": manifest }))
}

fn handle_get_spec(ctx: &ProjectContext, params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let file = params
        .get("file")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: 'file'"))?;

    let spec_path = ctx.report_dir.join("spec").join(format!("{}.json", file));
    if !spec_path.exists() {
        anyhow::bail!("Spec file not found: {}", spec_path.display());
    }
    let content = std::fs::read_to_string(&spec_path)?;
    let spec: serde_json::Value = serde_json::from_str(&content)?;
    Ok(spec)
}

fn handle_get_next_task(ctx: &ProjectContext) -> anyhow::Result<serde_json::Value> {
    let db_guard = ctx.db()?;
    let conn = db_guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Database not available"))?;

    let response = TaskQueue::get_next_task(conn, &ctx.report_dir)?;
    match response {
        Some(task_resp) => Ok(serde_json::to_value(&task_resp)?),
        None => Ok(serde_json::json!({
            "has_next": false,
            "task": null
        })),
    }
}

fn handle_verify_file(ctx: &ProjectContext, params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let file = params
        .get("file")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: 'file'"))?;
    let content = params
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Missing required parameter: 'content'"))?;
    let threshold = params
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.85);

    let db_guard = ctx.db()?;
    let conn = db_guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Database not available"))?;

    let result = TaskQueue::verify_file(conn, &ctx.report_dir, file, content, threshold)?;
    Ok(serde_json::to_value(&result)?)
}

fn handle_list_tasks(ctx: &ProjectContext, params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let db_guard = ctx.db()?;
    let conn = db_guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Database not available"))?;

    let status_filter = params.get("status").and_then(|v| v.as_str());
    let tasks = db::list_tasks(conn, status_filter)?;
    Ok(serde_json::to_value(&tasks)?)
}

fn handle_get_progress(ctx: &ProjectContext) -> anyhow::Result<serde_json::Value> {
    let db_guard = ctx.db()?;
    let conn = db_guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Database not available"))?;

    let progress = db::get_progress(conn)?;
    Ok(serde_json::to_value(&progress)?)
}

fn handle_diff(ctx: &ProjectContext, params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let auto = params.get("auto").and_then(|v| v.as_bool()).unwrap_or(false);
    let new_version = params.get("new_version").and_then(|v| v.as_str()).map(|s| s.to_string());

    let args = diff::DiffArgs {
        path: ctx.project_root.to_string_lossy().to_string(),
        new_version,
        auto,
    };
    diff::run(&args)?;

    // Read and return the latest diff report
    let migration_dir = &ctx.migration_folder;
    let diffs_dir = migration_dir.join("diffs");
    let latest_path = diffs_dir.join("latest.json");
    if latest_path.exists() {
        let content = std::fs::read_to_string(&latest_path)?;
        let report: serde_json::Value = serde_json::from_str(&content)?;
        Ok(report)
    } else {
        Ok(serde_json::json!({ "status": "no_changes" }))
    }
}

fn handle_verify_migration(_ctx: &ProjectContext, _params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    // The verify subcommand will be run as a separate process.
    // For now, return a message indicating this.
    Ok(serde_json::json!({
        "status": "not_implemented",
        "message": "Use 'migration-analyze verify --new-version <version>' directly"
    }))
}

/// Mark one or more files as completed (verified).
///
/// Accepts:
/// - `file` (string) — single file
/// - `files` (array of strings) — multiple files
///
/// Returns updated progress + todo list so AI can read them immediately.
fn handle_task_complete(ctx: &ProjectContext, params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    // Collect file paths from params (supports "file" or "files")
    let file_paths: Vec<String> = if let Some(files) = params.get("files").and_then(|v| v.as_array()) {
        files
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    } else if let Some(file) = params.get("file").and_then(|v| v.as_str()) {
        vec![file.to_string()]
    } else {
        anyhow::bail!("Missing required parameter: 'file' (string) or 'files' (array)")
    };

    if file_paths.is_empty() {
        anyhow::bail!("No file paths provided")
    }

    let db_guard = ctx.db()?;
    let conn = db_guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Database not available"))?;

    // Batch-mark all as done
    let refs: Vec<&str> = file_paths.iter().map(|s| s.as_str()).collect();
    db::mark_tasks_done(conn, &refs)?;

    // Refresh manifests so AI sees updated state immediately
    let updated = manifest::refresh_all(conn, &ctx.report_dir)?;

    Ok(serde_json::json!({
        "status": "ok",
        "completed": file_paths,
        "count": file_paths.len(),
        "updated": updated,
    }))
}

/// Batch verify — supports single file, directory, or all.
///
/// Parameters:
/// - `file` (string) — verify a single file (requires `content`)
/// - `content` (string) — Rust code for single-file verify
/// - `directory` (string) — verify all spec files under this directory path
/// - `all` (bool) — verify all spec files
/// - `project_path` (string) — path to Rust project root (needed for dir/all mode)
/// - `threshold` (number) — similarity threshold (default: 0.5)
fn handle_verify(ctx: &ProjectContext, params: &serde_json::Value) -> anyhow::Result<serde_json::Value> {
    let file = params.get("file").and_then(|v| v.as_str());
    let content = params.get("content").and_then(|v| v.as_str());
    let directory = params.get("directory").and_then(|v| v.as_str());
    let all = params.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
    let project_path = params
        .get("project_path")
        .and_then(|v| v.as_str())
        .map(std::path::PathBuf::from);
    let threshold = params
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.5);
    let deep = params
        .get("deep")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let db_guard = ctx.db()?;
    let conn = db_guard
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Database not available"))?;

    let result = TaskQueue::verify_batch(
        conn,
        &ctx.report_dir,
        file,
        content,
        directory,
        all,
        project_path.as_deref(),
        threshold,
        deep,
    )?;

    Ok(serde_json::to_value(&result)?)
}
