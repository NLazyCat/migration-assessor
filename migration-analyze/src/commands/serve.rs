use axum::Router;
use clap::Args;
use std::net::SocketAddr;

use std::sync::Arc;
use tower_http::services::ServeDir;

use crate::commands::context::ProjectContext;
use crate::commands::resolve_project_path;
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
    let project_root = resolve_project_path(&args.path);

    let ctx = ProjectContext::load(&project_root)?;

    if !ctx.report_dir.exists() {
        anyhow::bail!(
            "Report directory not found at {}. Run 'migration-analyze analyze' first.",
            ctx.report_dir.display()
        );
    }

    let state = web::routes::AppState { ctx: Arc::new(ctx) };

    let state = Arc::new(state);

    let app = Router::new()
        .nest_service("/static", ServeDir::new("static"))
        .route("/", axum::routing::get(web::routes::page_shell))
        .route("/overview", axum::routing::get(web::routes::page_overview))
        .route("/files", axum::routing::get(web::routes::page_files))
        .route("/deps", axum::routing::get(web::routes::page_deps))
        .route("/scores", axum::routing::get(web::routes::page_scores))
        .route("/graph", axum::routing::get(web::routes::page_graph))
        .route(
            "/report-ref",
            axum::routing::get(web::routes::page_report_ref),
        )
        .route("/api/project", axum::routing::get(web::routes::api_project))
        .route("/api/files", axum::routing::get(web::routes::api_files))
        .route("/api/deps", axum::routing::get(web::routes::api_deps))
        .route("/api/compat", axum::routing::get(web::routes::api_compat))
        .route("/api/graph", axum::routing::get(web::routes::api_graph))
        .route("/api/scores", axum::routing::get(web::routes::api_scores))
        .route(
            "/api/references",
            axum::routing::get(web::routes::api_references),
        )
        .route(
            "/api/references/*file",
            axum::routing::get(web::routes::api_file_references),
        )
        .route(
            "/boundaries",
            axum::routing::get(web::routes::page_boundaries),
        )
        .route(
            "/api/boundaries",
            axum::routing::get(web::routes::api_boundaries),
        )
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
