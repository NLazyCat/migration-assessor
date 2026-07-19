mod common;

#[test]
fn test_e2e_serve_api_endpoints() {
    let (_tmp_dir, project_root) = common::setup_project();

    // Run analyze first to generate report data
    let bin = common::binary_path();
    let mut analyze_cmd = assert_cmd::Command::new(&bin);
    analyze_cmd.current_dir(&project_root).arg("analyze");
    analyze_cmd.assert().success();

    // Start the serve binary in background
    let port = find_free_port().expect("find free port");
    let mut serve_handle = std::process::Command::new(&bin)
        .current_dir(&project_root)
        .arg("serve")
        .arg("--port")
        .arg(port.to_string())
        .spawn()
        .expect("failed to spawn serve");

    // Wait for server to start
    let base_url = format!("http://localhost:{}", port);
    wait_for_server(&base_url, 10).expect("server did not start in time");

    // Test API endpoints
    let client = reqwest::blocking::Client::new();

    // /api/project
    let resp = client
        .get(format!("{}/api/project", base_url))
        .send()
        .expect("GET /api/project");
    assert_eq!(resp.status(), 200);
    let project: serde_json::Value = resp.json().expect("parse project API");
    assert!(project.get("sourceLanguage").is_some());

    // /api/files
    let resp = client
        .get(format!("{}/api/files", base_url))
        .send()
        .expect("GET /api/files");
    assert_eq!(resp.status(), 200);
    let files: serde_json::Value = resp.json().expect("parse files API");
    assert!(files.as_array().map(|a| !a.is_empty()).unwrap_or(false));

    // /api/scores
    let resp = client
        .get(format!("{}/api/scores", base_url))
        .send()
        .expect("GET /api/scores");
    assert_eq!(resp.status(), 200);
    let scores: serde_json::Value = resp.json().expect("parse scores API");
    let scores_arr = scores.as_array().expect("scores should be array");
    assert!(!scores_arr.is_empty());

    // /api/deps
    let resp = client
        .get(format!("{}/api/deps", base_url))
        .send()
        .expect("GET /api/deps");
    assert_eq!(resp.status(), 200);

    // /api/compat
    let resp = client
        .get(format!("{}/api/compat", base_url))
        .send()
        .expect("GET /api/compat");
    assert_eq!(resp.status(), 200);

    // /api/graph
    let resp = client
        .get(format!("{}/api/graph", base_url))
        .send()
        .expect("GET /api/graph");
    assert_eq!(resp.status(), 200);

    // /api/references
    let resp = client
        .get(format!("{}/api/references", base_url))
        .send()
        .expect("GET /api/references");
    assert_eq!(resp.status(), 200);

    // /api/references/:file
    let resp = client
        .get(format!("{}/api/references/src%2Ftypes.ts", base_url))
        .send()
        .expect("GET /api/references/src/types.ts");
    assert_eq!(resp.status(), 200);

    // / (HTML shell page)
    let resp = client
        .get(&base_url)
        .send()
        .expect("GET /");
    assert_eq!(resp.status(), 200);
    let body = resp.text().expect("read HTML body");
    assert!(body.contains("migration") || body.contains("<html"), "root page should return HTML");

    // Clean up: kill the server
    let _ = serve_handle.kill();
    let _ = serve_handle.wait();
}

/// Finds a free TCP port by binding to port 0.
fn find_free_port() -> Option<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").ok()?;
    listener.local_addr().ok().map(|addr| addr.port())
}

/// Waits up to `timeout_secs` seconds for the server's root endpoint to respond.
fn wait_for_server(base_url: &str, timeout_secs: u64) -> Result<(), &'static str> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .map_err(|_| "build client")?;

    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(timeout_secs) {
        if client.get(base_url).send().is_ok() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    Err("server did not respond")
}
