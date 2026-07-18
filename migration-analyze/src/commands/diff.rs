use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

/// ── CLI args ──────────────────────────────────────────────────────────────

#[derive(Args)]
pub struct DiffArgs {
    /// Project root directory (defaults to current directory)
    #[arg(default_value = ".")]
    pub path: String,

    /// New version to diff against (tag, commit hash, or branch name)
    #[arg(long, required_unless_present = "auto", conflicts_with = "auto")]
    pub new_version: Option<String>,

    /// Auto-detect the latest version from the remote repository
    #[arg(long)]
    pub auto: bool,
}

/// ── Report types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
struct DiffReport {
    generated_at: String,
    source_repo: Option<String>,
    from_version: Option<String>,
    to_version: String,
    files: Vec<String>,
    file_changes: Vec<FileChangeGroup>,
    propagation: PropagationResult,
}

#[derive(Debug, Clone, Serialize)]
struct FileChangeGroup {
    file: String,
    source_attached: bool,
    changes: Vec<SymbolChangeDetail>,
}

#[derive(Debug, Clone, Serialize)]
struct SymbolChangeDetail {
    symbol: String,
    kind: String,
    change_type: String,
    full_body: String,
    position: RelativePosition,
}

#[derive(Debug, Clone, Serialize)]
struct RelativePosition {
    #[serde(skip_serializing_if = "Option::is_none")]
    parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    above: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    below: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PropagationResult {
    triggered_by: Vec<String>,
    affected_files: Vec<String>,
    chain: Vec<PropagationLink>,
}

#[derive(Debug, Clone, Serialize)]
struct PropagationLink {
    from: String,
    to: String,
    via: String,
}

/// ── Symbol index types (parsed from stored JSON) ──────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct StoredSymbol {
    name: String,
    kind: String,
    #[serde(rename = "line_range")]
    line_range: [usize; 2],
    #[serde(default)]
    children: Vec<StoredSymbol>,
}

#[derive(Debug, Clone, Deserialize)]
struct SymbolIndexFile {
    #[allow(dead_code)]
    module: String,
    symbols: Vec<StoredSymbol>,
}

/// ── Reverse index types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
struct ReverseRef {
    symbol: String,
    #[allow(dead_code)]
    location: ReverseLocation,
    kind: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ReverseLocation {
    #[allow(dead_code)]
    file: String,
    #[allow(dead_code)]
    line: usize,
    #[allow(dead_code)]
    column: usize,
}

type ReverseIndex = HashMap<String, Vec<ReverseRef>>;

/// ── Diff line type for internal processing ────────────────────────────────

#[derive(Debug)]
struct DiffLine {
    kind: char,
    content: String,
}

/// ── Main entry ────────────────────────────────────────────────────────────

pub fn run(args: &DiffArgs) -> anyhow::Result<()> {
    let project_root = Path::new(&args.path).canonicalize()?;
    let config_path = project_root.join("migration.toml");

    if !config_path.exists() {
        anyhow::bail!("migration.toml not found in {}. Run 'migration-analyze init' first.", project_root.display());
    }

    let config = migration_core::config::Config::load(&config_path)?;

    // Detect migration folder: find <repo>-migration/ in project root
    let migration_dir = detect_migration_folder(&project_root)?;
    let report_dir = migration_dir.join("report");

    if !report_dir.exists() {
        anyhow::bail!("Report folder not found at {}. Run 'migration-analyze analyze' first.", report_dir.display());
    }

    let source_repo = config.project.source_repo.clone();
    let from_version = config.project.source_version.clone();
    // Determine target version
    let new_version = if args.auto {
        let repo = source_repo.as_deref().ok_or_else(|| {
            anyhow::anyhow!("--auto requires source_repo in migration.toml")
        })?;
        let latest = fetch_latest_version(repo)?;
        println!("  Auto-detected latest version: {}", latest);
        latest
    } else {
        args.new_version.clone().ok_or_else(|| {
            anyhow::anyhow!("Either --new-version or --auto is required")
        })?
    };
    let source_path = config.project.source.clone();

    println!("Running incremental diff analysis...");
    if let Some(r) = &source_repo { println!("  Source repo: {}", r); }
    if let Some(f) = &from_version { println!("  From: {}", f); }
    println!("  To:   {}", new_version);

    // Step 1: Fetch the raw diff
    let raw_diff = fetch_diff(
        source_repo.as_deref(),
        from_version.as_deref(),
        &new_version,
        source_path.as_deref(),
        &project_root,
    )?;

    if raw_diff.trim().is_empty() {
        println!("No differences between versions.");
        return Ok(());
    }

    // Step 2: Parse per-file diffs
    let file_diffs = parse_file_diffs(&raw_diff);
    if file_diffs.is_empty() {
        println!("No changed files detected.");
        return Ok(());
    }

    println!("\nChanged files ({}):", file_diffs.len());
    for (status, path, _) in &file_diffs {
        println!("  {}  {}", status, path);
    }

    // Step 3: Load symbol indexes and reverse index from report
    let symbols_dir = report_dir.join("symbols").join("by-dir");
    let reverse_index = load_reverse_index(&report_dir);

    // Step 4: Analyze each file for symbol changes with full body extraction
    let diff_dir = migration_dir.join("diffs");
    let changed_dir = diff_dir.join("changed");
    std::fs::create_dir_all(&changed_dir)?;

    let mut all_file_changes: Vec<FileChangeGroup> = Vec::new();
    let mut all_files: Vec<String> = Vec::new();
    let mut all_triggered_symbols: Vec<String> = Vec::new();

    for (status, file_path, diff_lines) in &file_diffs {
        all_files.push(file_path.clone());

        if !is_analyzable_file(file_path) {
            continue;
        }

        // Reconstruct new file content for body extraction
        let new_content = reconstruct_full_file(diff_lines);

        // Get symbol changes with full body extraction
        let symbol_index = load_symbol_index(file_path, &symbols_dir);
        let file_changes = analyze_file_changes(
            status,
            file_path,
            diff_lines,
            &new_content,
            &symbol_index,
        );

        if let Some(mut fc) = file_changes {
            if !fc.changes.is_empty() {
                // Write aggregated change file
                let changed_path = changed_dir.join(file_path);
                if let Some(parent) = changed_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let change_content = serde_json::to_string_pretty(&fc.changes)?;
                std::fs::write(&changed_path, change_content)?;

                fc.source_attached = true;
                for ch in &fc.changes {
                    let symbol_id = format!("{}:{}", file_path, ch.symbol);
                    all_triggered_symbols.push(symbol_id);
                }
                all_file_changes.push(fc);
            }
        }
    }

    // Step 5: Propagation analysis
    let propagation = propagate_changes(&all_triggered_symbols, &reverse_index);

    // Step 6: Write main report
    let report = DiffReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        source_repo,
        from_version,
        to_version: new_version,
        files: all_files,
        file_changes: all_file_changes,
        propagation,
    };

    let timestamp = chrono::Utc::now().format("%Y-%m-%d");
    let report_path = diff_dir.join(format!("diff-{}.json", timestamp));
    std::fs::write(&report_path, serde_json::to_string_pretty(&report)?)?;

    // Write affected files summary
    let affected_path = diff_dir.join("affected-files.json");
    let affected_summary = serde_json::json!({
        "triggered_by": report.propagation.triggered_by,
        "affected_files": report.propagation.affected_files,
        "total_affected": report.propagation.affected_files.len(),
    });
    std::fs::write(&affected_path, serde_json::to_string_pretty(&affected_summary)?)?;

    println!("  Affected files: {}", report.propagation.affected_files.len());

    Ok(())
}

/// ── Step 3 helper: Load reverse index ─────────────────────────────────────

fn load_reverse_index(migration_dir: &Path) -> ReverseIndex {
    let path = migration_dir.join("references").join("reverse.json");
    if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        HashMap::new()
    }
}

/// ── Analyze changes for one file ──────────────────────────────────────────

fn analyze_file_changes(
    status: &str,
    file_path: &str,
    diff_lines: &[DiffLine],
    new_content: &Option<String>,
    symbol_index: &Option<SymbolIndexFile>,
) -> Option<FileChangeGroup> {
    let index = match symbol_index {
        Some(i) => i,
        None => return None,
    };

    let all_symbols = flatten_symbols(&index.symbols);
    if all_symbols.is_empty() {
        return None;
    }

    // Build old-line-number to symbol mapping
    let line_to_symbol: HashMap<usize, &StoredSymbol> = all_symbols
        .iter()
        .flat_map(|sym| {
            (sym.line_range[0]..=sym.line_range[1]).map(move |l| (l, sym))
        })
        .collect();

    // Find changed symbol names by tracking old line numbers through diff
    let mut changed_names: Vec<String> = Vec::new();
    let mut old_lineno: usize = 1;
    let mut inside_symbol: Option<String> = None;
    let max_old_line = all_symbols.iter().map(|s| s.line_range[1]).max().unwrap_or(0);

    for dl in diff_lines {
        match dl.kind {
            '@' => {
                old_lineno = dl.content.parse::<usize>().unwrap_or(1);
                inside_symbol = line_to_symbol.get(&old_lineno).map(|s| s.name.clone());
            }
            ' ' | '-' => {
                if dl.kind == '-' {
                    if let Some(sym) = line_to_symbol.get(&old_lineno) {
                        if !changed_names.contains(&sym.name) {
                            changed_names.push(sym.name.clone());
                        }
                    }
                }
                inside_symbol = line_to_symbol.get(&old_lineno).map(|s| s.name.clone());
                old_lineno += 1;
            }
            '+' => {
                if let Some(ref sym_name) = inside_symbol {
                    if let Some(sym) = all_symbols.iter().find(|s| s.name == *sym_name) {
                        if !changed_names.contains(sym_name) && old_lineno <= max_old_line && old_lineno <= sym.line_range[1] {
                            changed_names.push(sym_name.clone());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Also handle added files (all symbols)
    if status == "A" {
        changed_names = all_symbols.iter().map(|s| s.name.clone()).collect();
    }

    if changed_names.is_empty() {
        return None;
    }

    // For each changed symbol, extract full body and relative position
    let mut changes: Vec<SymbolChangeDetail> = Vec::new();

    // Build adjacency map from symbol index ordering
    let ordered_names: Vec<&str> = index.symbols.iter().map(|s| s.name.as_str()).collect();

    for sym_name in &changed_names {
        let orig = all_symbols.iter().find(|s| s.name == *sym_name)?;
        let stable_change_type = if status == "A" { "added" } else if status == "D" { "removed" } else { "modified" };

        let full_body = match status {
            "D" => String::new(),
            _ => extract_full_body(sym_name, &orig.kind, new_content.as_deref().unwrap_or("")),
        };

        let position = compute_relative_position(sym_name, &ordered_names, &index);

        changes.push(SymbolChangeDetail {
            symbol: sym_name.clone(),
            kind: orig.kind.clone(),
            change_type: stable_change_type.to_string(),
            full_body,
            position,
        });
    }

    if changes.is_empty() {
        return None;
    }

    Some(FileChangeGroup {
        file: file_path.to_string(),
        source_attached: false,
        changes,
    })
}

/// ── Full body extraction ──────────────────────────────────────────────────

fn extract_full_body(symbol_name: &str, kind: &str, source: &str) -> String {
    // Determine the declaration keyword(s) for the given kind
    let keywords: &[&str] = match kind {
        "function" | "method" => &["function ", "fn "],
        "class" => &["class "],
        "interface" => &["interface "],
        "struct" => &["struct "],
        "enum" => &["enum "],
        "trait" => &["trait "],
        "type" | "type_alias" => &["type "],
        "const" | "constant" => &["const "],
        "arrow_function" | "variable" => &["const ", "let ", "var "],
        _ => &[],
    };

    // Find the start of the symbol definition
    let mut start_pos = None;
    for &kw in keywords {
        let search: Vec<usize> = source.match_indices(kw)
            .filter_map(|(pos, _)| {
                let after_kw = &source[pos + kw.len()..];
                // Check if symbol name follows the keyword
                after_kw.trim_start().starts_with(symbol_name).then_some(pos)
            })
            .collect();

        if let Some(&pos) = search.first() {
            start_pos = Some(pos);
            break;
        }
    }

    // Fallback: search for symbol name directly (arrow functions, exports)
    if start_pos.is_none() {
        start_pos = source.find(&format!("{} ", symbol_name))
            .or_else(|| source.find(&format!("{}({}", symbol_name, symbol_name)))
            .or_else(|| source.find(&format!("{}.{}", symbol_name, symbol_name)));
    }

    let start = match start_pos {
        Some(p) => p,
        None => return String::new(),
    };

    // Find end by brace matching
    let from_start = &source[start..];
    let brace_start = from_start.find('{');

    let end = match brace_start {
        Some(brace_pos) => {
            let body_start = start + brace_pos;
            match find_matching_brace(&source[body_start..]) {
                Some(brace_len) => {
                    let end_pos = body_start + brace_len;
                    // Include trailing newline if present
                    if end_pos < source.len() && source.as_bytes().get(end_pos) == Some(&b'\n') {
                        end_pos + 1
                    } else {
                        end_pos
                    }
                }
                None => source.len(),
            }
        }
        None => {
            // No braces: type alias, const, etc. — find end of line
            let rest = &source[start..];
            rest.find('\n').map(|n| start + n + 1).unwrap_or(source.len())
        }
    };

    source[start..end].to_string()
}

fn find_matching_brace(text: &str) -> Option<usize> {
    let mut depth = 0u32;
    let mut in_string = false;
    let mut string_char = '"';
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut escaped = false;

    for (i, ch) in text.char_indices() {
        if in_line_comment {
            if ch == '\n' { in_line_comment = false; }
            continue;
        }
        if in_block_comment {
            if ch == '*' && text[i+1..].starts_with('/') { in_block_comment = false; }
            continue;
        }
        if in_string {
            if escaped { escaped = false; continue; }
            if ch == '\\' { escaped = true; continue; }
            if ch == string_char { in_string = false; }
            continue;
        }
        match ch {
            '/' if text[i+1..].starts_with('/') => { in_line_comment = true; }
            '/' if text[i+1..].starts_with('*') => { in_block_comment = true; }
            '"' | '\'' | '`' => { in_string = true; string_char = ch; }
            '{' => depth += 1,
            '}' => {
                if depth == 0 { return None; }
                depth -= 1;
                if depth == 0 { return Some(i + 1); }
            }
            _ => {}
        }
    }
    None
}

/// ── Relative position computation ─────────────────────────────────────────

fn compute_relative_position(
    symbol_name: &str,
    ordered_names: &[&str],
    index: &SymbolIndexFile,
) -> RelativePosition {
    // Find parent (the symbol whose children contain this symbol)
    let parent = find_parent(symbol_name, &index.symbols);

    // Find adjacent siblings from the ordered flat list
    let pos = ordered_names.iter().position(|n| *n == symbol_name);
    let above = pos.and_then(|p| {
        if p > 0 { Some(ordered_names[p - 1].to_string()) } else { None }
    });
    let below = pos.and_then(|p| {
        if p + 1 < ordered_names.len() { Some(ordered_names[p + 1].to_string()) } else { None }
    });

    RelativePosition {
        parent,
        above,
        below,
    }
}

fn find_parent(name: &str, symbols: &[StoredSymbol]) -> Option<String> {
    for sym in symbols {
        for child in &sym.children {
            if child.name == name {
                return Some(format!("{} {}", sym.kind, sym.name));
            }
            // Check deeper nesting
            if let Some(grandparent) = find_parent_recursive(name, &sym.children, &sym.name, &sym.kind) {
                return Some(grandparent);
            }
        }
    }
    None
}

fn find_parent_recursive(
    name: &str,
    children: &[StoredSymbol],
    parent_name: &str,
    parent_kind: &str,
) -> Option<String> {
    for child in children {
        if child.name == name {
            return Some(format!("{} {}", parent_kind, parent_name));
        }
        if !child.children.is_empty() {
            if let Some(result) = find_parent_recursive(name, &child.children, &child.name, &child.kind) {
                return Some(result);
            }
        }
    }
    None
}

/// ── Propagation analysis ──────────────────────────────────────────────────

fn propagate_changes(
    triggered_symbols: &[String],
    reverse_index: &ReverseIndex,
) -> PropagationResult {
    let mut visited: HashSet<String> = HashSet::new();
    let mut chain: Vec<PropagationLink> = Vec::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut affected_files: HashSet<String> = HashSet::new();

    // Seed the queue with triggered symbols
    for sym in triggered_symbols {
        visited.insert(sym.clone());
        queue.push_back(sym.clone());
    }

    while let Some(current) = queue.pop_front() {
        // Look up who references this symbol (callers depend on it)
        if let Some(refs) = reverse_index.get(&current) {
            for r in refs {
                let dependent_symbol = &r.symbol;
                if !visited.contains(dependent_symbol) {
                    visited.insert(dependent_symbol.clone());
                    queue.push_back(dependent_symbol.clone());

                    // Record the dependent's file
                    if let Some(file) = dependent_symbol.rsplitn(2, ':').nth(1) {
                        affected_files.insert(file.to_string());
                    }

                    chain.push(PropagationLink {
                        from: current.clone(),
                        to: dependent_symbol.clone(),
                        via: r.kind.clone(),
                    });
                }
            }
        }
    }

    // Sort affected files for deterministic output
    let mut sorted_files: Vec<String> = affected_files.into_iter().collect();
    sorted_files.sort();

    PropagationResult {
        triggered_by: triggered_symbols.to_vec(),
        affected_files: sorted_files,
        chain,
    }
}

/// ── Step 1: Fetch diff between old and new versions ───────────────────────

fn resolve_source_path(src: &str, project_root: &Path) -> PathBuf {
    let trimmed = src.trim_start_matches("//?/");
    let p = Path::new(trimmed);
    if p.is_absolute() { p.to_path_buf() } else { project_root.join(p) }
}

fn fetch_diff(
    source_repo: Option<&str>,
    from_version: Option<&str>,
    to_version: &str,
    source_path: Option<&str>,
    project_root: &Path,
) -> anyhow::Result<String> {
    // Try local source first if available
    if let Some(src) = source_path {
        let candidate = resolve_source_path(src, project_root);
        let has_git = candidate.join(".git").exists() || candidate.join("HEAD").exists();
        if has_git {
            match run_local_diff(&candidate, from_version, to_version) {
                Ok(diff) => {
                    println!("  Using local source at: {}", candidate.display());
                    return Ok(diff);
                }
                Err(e) => {
                    eprintln!("  Local diff failed ({}), falling back to remote fetch...", e);
                }
            }
        }
    }

    let repo = source_repo
        .ok_or_else(|| anyhow::anyhow!("No source_repo in migration.toml and no local source found."))?;
    let from = from_version
        .ok_or_else(|| anyhow::anyhow!("No source_version in migration.toml. Cannot diff without a known base version."))?;

    println!("  Fetching diff from remote: {} {}..{}", repo, from, to_version);
    fetch_remote_diff(repo, from, to_version)
}

/// Fetch the latest version tag from a remote repository.
/// Prefers semantic version tags, falls back to latest annotated tag, then HEAD commit.
fn fetch_latest_version(repo: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["ls-remote", "--tags", repo])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git ls-remote failed for {}: {}", repo, stderr);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().collect();

    // Collect all tags, filter semver-like ones
    let mut tags: Vec<String> = Vec::new();
    for line in &lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 { continue; }
        let ref_str = parts[1];

        // Extract tag name from refs/tags/<name>
        if let Some(tag) = ref_str.strip_prefix("refs/tags/") {
            // Skip ^{} peeled tags
            if tag.ends_with("^{}") { continue; }
            tags.push(tag.to_string());
        }
    }

    // Sort tags to find latest (ascending: oldest first, latest last)
    tags.sort_by(|a, b| {
        let a_ver = a.trim_start_matches('v');
        let b_ver = b.trim_start_matches('v');
        let a_parts: Vec<&str> = a_ver.split('.').collect();
        let b_parts: Vec<&str> = b_ver.split('.').collect();

        for (ap, bp) in a_parts.iter().zip(b_parts.iter()) {
            match (ap.parse::<u64>(), bp.parse::<u64>()) {
                (Ok(an), Ok(bn)) if an != bn => return an.cmp(&bn),
                _ => {}
            }
        }
        a_parts.len().cmp(&b_parts.len()).then_with(|| a.cmp(b))
    });

    // Return latest tag (last after ascending sort), or fallback to HEAD commit
    let latest = tags.last().cloned().or_else(|| {
        // Fallback: get HEAD commit hash
        let head_output = std::process::Command::new("git")
            .args(["ls-remote", repo, "HEAD"])
            .output()
            .ok()?;
        if !head_output.status.success() { return None; }
        let stdout = String::from_utf8_lossy(&head_output.stdout);
        stdout.split_whitespace().next().map(|s| s.to_string())
    }).ok_or_else(|| anyhow::anyhow!("No tags or refs found in remote {}", repo))?;

    Ok(latest)
}

fn run_local_diff(src_path: &Path, from_version: Option<&str>, to_version: &str) -> anyhow::Result<String> {
    let from = from_version.unwrap_or("HEAD");
    let range = format!("{}..{}", from, to_version);
    let output = std::process::Command::new("git")
        .args(["diff", "-U9999", &range])
        .current_dir(src_path)
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff failed in {}: {}", src_path.display(), stderr);
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn fetch_remote_diff(repo: &str, from: &str, to: &str) -> anyhow::Result<String> {
    let tmp_dir = create_temp_dir()?;
    let result = fetch_diff_internal(&tmp_dir, repo, from, to);
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

fn fetch_diff_internal(tmp_dir: &Path, repo: &str, from: &str, to: &str) -> anyhow::Result<String> {
    let init = std::process::Command::new("git")
        .args(["init"]).current_dir(tmp_dir).output()?;
    if !init.status.success() { anyhow::bail!("git init failed in temp dir"); }

    let add_remote = std::process::Command::new("git")
        .args(["remote", "add", "origin", repo]).current_dir(tmp_dir).output()?;
    if !add_remote.status.success() {
        let stderr = String::from_utf8_lossy(&add_remote.stderr);
        anyhow::bail!("git remote add failed: {}", stderr);
    }

    // Fetch the branch that contains both commits (default: main)
    let branch = "main";
    let fetch = std::process::Command::new("git")
        .args(["fetch", "origin", branch, "--depth", "10"]).current_dir(tmp_dir).output()?;
    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        // Try HEAD as fallback
        let fetch2 = std::process::Command::new("git")
            .args(["fetch", "origin", "HEAD", "--depth", "10"]).current_dir(tmp_dir).output()?;
        if !fetch2.status.success() {
            let stderr2 = String::from_utf8_lossy(&fetch2.stderr);
            anyhow::bail!("git fetch failed: {} / {}", stderr, stderr2);
        }
    }

    // Now both commits should be in the local history
    let range = format!("{}..{}", from, to);
    let diff = std::process::Command::new("git")
        .args(["diff", "-U9999", &range]).current_dir(tmp_dir).output()?;
    if !diff.status.success() {
        let stderr = String::from_utf8_lossy(&diff.stderr);
        anyhow::bail!("git diff failed: {}", stderr);
    }
    Ok(String::from_utf8_lossy(&diff.stdout).to_string())
}

fn create_temp_dir() -> anyhow::Result<PathBuf> {
    let base = std::env::temp_dir().join("_mig_diff");
    let mut i = 0u64;
    loop {
        let dir = base.join(&i.to_string());
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
            return Ok(dir);
        }
        i += 1;
    }
}

/// ── Step 2: Parse raw unified diff into per-file sections ─────────────────

fn parse_file_diffs(raw: &str) -> Vec<(String, String, Vec<DiffLine>)> {
    let lines: Vec<&str> = raw.lines().collect();
    let mut sections: Vec<Vec<&str>> = Vec::new();
    let mut current: Vec<&str> = Vec::new();

    for line in &lines {
        if line.starts_with("diff --git") && !current.is_empty() {
            sections.push(std::mem::take(&mut current));
        }
        current.push(line);
    }
    if !current.is_empty() { sections.push(current); }

    let mut results: Vec<(String, String, Vec<DiffLine>)> = Vec::new();
    for section in &sections {
        if let Some((status, file_path, diff_lines)) = parse_one_file_diff(section) {
            results.push((status, file_path, diff_lines));
        }
    }
    results
}

fn parse_one_file_diff(lines: &[&str]) -> Option<(String, String, Vec<DiffLine>)> {
    let diff_header = lines.iter().find(|l| l.starts_with("diff --git"))?;
    let path_part = diff_header
        .strip_prefix("diff --git a/")?
        .split_whitespace()
        .next()?;
    let file_path = path_part.to_string();

    let status = if lines.iter().any(|l| l.starts_with("--- /dev/null")) {
        "A"
    } else if lines.iter().any(|l| l.starts_with("+++ /dev/null")) {
        "D"
    } else {
        "M"
    };

    let hunk_start = lines.iter().position(|l| l.starts_with("@@"))?;

    let mut diff_lines: Vec<DiffLine> = Vec::new();
    for line in &lines[hunk_start..] {
        if line.starts_with("diff --git") { break; }
        if let Some(hdr) = line.strip_prefix("@@") {
            // Parse old-file starting line number from @@ -M,N +P,Q @@
            let old_start = hdr.split_whitespace().next().and_then(|s| {
                let s = s.trim_start_matches('-');
                s.split(',').next().and_then(|n| n.parse::<usize>().ok())
            }).unwrap_or(1);
            diff_lines.push(DiffLine { kind: '@', content: old_start.to_string() });
            continue;
        }
        if line.is_empty() {
            diff_lines.push(DiffLine { kind: ' ', content: String::new() });
            continue;
        }
        let kind = line.chars().next().unwrap_or(' ');
        let content = &line[1..];
        diff_lines.push(DiffLine { kind, content: content.to_string() });
    }

    Some((status.to_string(), file_path, diff_lines))
}

/// ── Helpers ───────────────────────────────────────────────────────────────

fn is_analyzable_file(path: &str) -> bool {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    matches!(ext, "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" | "rs")
}

fn load_symbol_index(file_path: &str, symbols_dir: &Path) -> Option<SymbolIndexFile> {
    let index_path = symbols_dir.join(format!("{}.index.json", file_path));
    if !index_path.exists() { return None; }
    let content = std::fs::read_to_string(&index_path).ok()?;
    serde_json::from_str(&content).ok()
}

fn flatten_symbols(symbols: &[StoredSymbol]) -> Vec<StoredSymbol> {
    let mut result = Vec::new();
    for sym in symbols {
        result.push(sym.clone());
        if !sym.children.is_empty() {
            result.extend(flatten_symbols(&sym.children));
        }
    }
    result
}

/// Reconstruct full new file content from diff lines.
fn reconstruct_full_file(diff_lines: &[DiffLine]) -> Option<String> {
    let mut content = String::new();
    let mut has_any = false;
    for dl in diff_lines {
        if dl.kind == ' ' || dl.kind == '+' {
            content.push_str(&dl.content);
            content.push('\n');
            has_any = true;
        }
        // '@' lines (hunk headers) are skipped
    }
    if has_any { Some(content) } else { None }
}

/// Find the migration folder (<repo>-migration/) in the project root.
fn detect_migration_folder(project_root: &Path) -> anyhow::Result<PathBuf> {
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


