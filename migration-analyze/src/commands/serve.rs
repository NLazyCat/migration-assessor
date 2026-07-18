use axum::Router;
use clap::Args;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tower_http::services::ServeDir;

use crate::web;

#[derive(Args)]
pub struct ServeArgs {
    /// Port to listen on
    #[arg(long, default_value = "8080")]
    pub port: u16,

    /// Path to the migration project root
    #[arg(default_value = ".")]
    pub path: String,

    /// Open browser automatically
    #[arg(long)]
    pub open: bool,
}

pub async fn run(args: &ServeArgs) -> anyhow::Result<()> {
    let project_root = std::path::Path::new(&args.path);
    let project_root = project_root.canonicalize()?;

    let migration_dir = detect_migration_folder(&project_root)?;
    let report_dir = migration_dir.join("report");

    if !report_dir.exists() {
        anyhow::bail!(
            "Report directory not found at {}. Run 'migration-analyze analyze' first.",
            report_dir.display()
        );
    }

    let state = web::routes::AppState { report_dir };

    let state = Arc::new(state);

    let app = Router::new()
        .nest_service("/static", ServeDir::new("static"))
        .route("/", axum::routing::get(web::routes::page_shell))
        .route("/overview", axum::routing::get(web::routes::page_overview))
        .route("/files", axum::routing::get(web::routes::page_files))
        .route("/deps", axum::routing::get(web::routes::page_deps))
        .route("/scores", axum::routing::get(web::routes::page_scores))
        .route("/graph", axum::routing::get(web::routes::page_graph))
        .route("/report-ref", axum::routing::get(web::routes::page_report_ref))
        .route("/api/project", axum::routing::get(web::routes::api_project))
        .route("/api/files", axum::routing::get(web::routes::api_files))
        .route("/api/deps", axum::routing::get(web::routes::api_deps))
        .route("/api/compat", axum::routing::get(web::routes::api_compat))
        .route("/api/graph", axum::routing::get(web::routes::api_graph))
        .route("/api/scores", axum::routing::get(web::routes::api_scores))
        .route("/api/references", axum::routing::get(web::routes::api_references))
        .route("/api/references/*file", axum::routing::get(web::routes::api_file_references))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], args.port));
    println!("  Web UI: http://localhost:{}", args.port);

    if args.open {
        let url = format!("http://localhost:{}", args.port);
        let _ = open::that(&url);
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

fn detect_migration_folder(project_root: &std::path::Path) -> anyhow::Result<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(project_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.ends_with("-migration") && path.join("report").exists() {
                return Ok(path);
            }
        }
    }
    anyhow::bail!(
        "No migration folder (*-migration/) found in {}. Run 'migration-analyze analyze' first.",
        project_root.display()
    );
}
