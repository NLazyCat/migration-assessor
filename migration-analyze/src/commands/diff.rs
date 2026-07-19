use clap::Args;
use migration_core::output_paths;
use migration_core::recommendation::{DependencyRecommendation, RecommendationReport};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::commands::context::ProjectContext;
use crate::commands::resolve_project_path;

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
    /// Dependency recommendations relevant to this file, loaded from the
    /// stored `external/recommendations.json`.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    recommendations: Vec<DependencyRecommendation>,
}

#[derive(Debug, Clone, Serialize)]
struct SymbolChangeDetail {
    symbol: String,
    kind: String,
    change_type: String,
    full_body: String,
    /// First few lines before the symbol definition from the new file.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    context_before: Vec<String>,
    /// First few lines after the symbol definition from the new file.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    context_after: Vec<String>,
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
    let project_root = resolve_project_path(&args.path);
    let config_path = project_root.join("migration.toml");

    if !config_path.exists() {
        anyhow::bail!(
            "No migration.toml found in {}.\n\
             Run 'migration-analyze analyze' to analyze the project first.",
            project_root.display()
        );
    }

    let ctx = ProjectContext::load(&project_root)?;
    let config = &ctx.config;

    // Detect migration folder: find <repo>-migration/ in project root
    let migration_dir = ctx.migration_folder.clone();
    let report_dir = ctx.report_dir.clone();

    if !report_dir.exists() {
        anyhow::bail!(
            "Report folder not found at {}. Run 'migration-analyze analyze' first.",
            report_dir.display()
        );
    }

    let source_repo = config.project.source_repo.clone();
    let from_version = config.project.source_version.clone();
    // Determine target version
    let new_version = if args.auto {
        let repo = source_repo
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("--auto requires source_repo in migration.toml"))?;
        let latest = fetch_latest_version(repo)?;
        println!("  Auto-detected latest version: {}", latest);
        latest
    } else {
        args.new_version
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Either --new-version or --auto is required"))?
    };
    let source_path = config.project.source.clone();

    println!("Running incremental diff analysis...");
    if let Some(r) = &source_repo {
        println!("  Source repo: {}", r);
    }
    if let Some(f) = &from_version {
        println!("  From: {}", f);
    }
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
    let symbols_dir = ctx.report_path("symbols");
    let reverse_index = load_reverse_index(&ctx);

    // Step 4: Analyze each file for symbol changes with full body extraction
    let diff_dir = migration_dir.join("diffs");
    let changed_dir = diff_dir.join("changed");
    std::fs::create_dir_all(&changed_dir)?;

    let mut all_file_changes: Vec<FileChangeGroup> = Vec::new();
    let mut all_files: Vec<String> = Vec::new();
    let mut all_triggered_symbols: Vec<String> = Vec::new();

    // Load dependency recommendations for attaching to changed files
    let file_recs = load_file_recommendations(&report_dir);

    for (status, file_path, diff_lines) in &file_diffs {
        all_files.push(file_path.clone());

        if !is_analyzable_file(file_path) {
            continue;
        }

        // Reconstruct new file content for body extraction
        let new_content = reconstruct_full_file(diff_lines);

        // Get symbol changes with full body extraction
        let symbol_index = load_symbol_index(file_path, &symbols_dir);
        let file_changes =
            analyze_file_changes(status, file_path, diff_lines, &new_content, &symbol_index);

        if let Some(mut fc) = file_changes
            && !fc.changes.is_empty()
        {
            // Write aggregated change file
            let changed_path = changed_dir.join(file_path);
            if let Some(parent) = changed_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let change_content = serde_json::to_string_pretty(&fc.changes)?;
            std::fs::write(&changed_path, change_content)?;

            fc.source_attached = true;
            if let Some(recs) = file_recs.get(file_path) {
                fc.recommendations = recs.clone();
            }
            for ch in &fc.changes {
                let symbol_id = format!("{}:{}", file_path, ch.symbol);
                all_triggered_symbols.push(symbol_id);
            }
            all_file_changes.push(fc);
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
    let dated_name = format!("diff-{}.json", timestamp);
    let report_path = diff_dir.join(&dated_name);
    let report_json = serde_json::to_string_pretty(&report)?;
    std::fs::write(&report_path, &report_json)?;

    // Keep a stable "latest" copy for consumers.
    let latest_path = diff_dir.join(output_paths::diffs::LATEST.trim_start_matches("diffs/"));
    std::fs::write(&latest_path, report_json)?;

    // Write affected files summary
    let affected_path = diff_dir.join("affected-files.json");
    let affected_summary = serde_json::json!({
        "triggered_by": report.propagation.triggered_by,
        "affected_files": report.propagation.affected_files,
        "total_affected": report.propagation.affected_files.len(),
    });
    std::fs::write(
        &affected_path,
        serde_json::to_string_pretty(&affected_summary)?,
    )?;

    println!(
        "  Affected files: {}",
        report.propagation.affected_files.len()
    );

    Ok(())
}

/// ── Step 3 helper: Load reverse index ─────────────────────────────────────
fn load_reverse_index(ctx: &ProjectContext) -> ReverseIndex {
    ctx.load_reverse_index().unwrap_or_default()
}

/// Up to this many lines of surrounding context to attach to each changed symbol.
const CONTEXT_WINDOW: usize = 3;

/// ── Analyze changes for one file ──────────────────────────────────────────
fn analyze_file_changes(
    status: &str,
    file_path: &str,
    diff_lines: &[DiffLine],
    new_content: &Option<String>,
    symbol_index: &Option<SymbolIndexFile>,
) -> Option<FileChangeGroup> {
    let index = symbol_index.as_ref()?;

    let all_symbols = flatten_symbols(&index.symbols);
    if all_symbols.is_empty() {
        return None;
    }

    // Build old-line-number to symbol mapping
    let line_to_symbol: HashMap<usize, &StoredSymbol> = all_symbols
        .iter()
        .flat_map(|sym| (sym.line_range[0]..=sym.line_range[1]).map(move |l| (l, sym)))
        .collect();

    // ── Pass 1: Find changed EXISTING symbols ──────────────────────────────
    let mut changed_names: Vec<String> = Vec::new();
    let mut old_lineno: usize = 1;
    let mut inside_symbol: Option<String> = None;
    let max_old_line = all_symbols
        .iter()
        .map(|s| s.line_range[1])
        .max()
        .unwrap_or(0);

    for dl in diff_lines {
        match dl.kind {
            '@' => {
                old_lineno = dl.content.parse::<usize>().unwrap_or(1);
                inside_symbol = line_to_symbol.get(&old_lineno).map(|s| s.name.clone());
            }
            ' ' | '-' => {
                if dl.kind == '-'
                    && let Some(sym) = line_to_symbol.get(&old_lineno)
                    && !changed_names.contains(&sym.name)
                {
                    changed_names.push(sym.name.clone());
                }
                inside_symbol = line_to_symbol.get(&old_lineno).map(|s| s.name.clone());
                old_lineno += 1;
            }
            '+' => {
                if let Some(ref sym_name) = inside_symbol
                    && let Some(sym) = all_symbols.iter().find(|s| s.name == *sym_name)
                    && !changed_names.contains(sym_name)
                    && old_lineno <= max_old_line
                    && old_lineno <= sym.line_range[1]
                {
                    changed_names.push(sym_name.clone());
                }
            }
            _ => {}
        }
    }

    // Also handle added files (all symbols)
    if status == "A" {
        changed_names = all_symbols.iter().map(|s| s.name.clone()).collect();
    }

    // ── Pass 2: Detect new symbols added post-analysis ─────────────────────
    let source = new_content.as_deref().unwrap_or("");
    let existing_names: HashSet<&str> = all_symbols.iter().map(|s| s.name.as_str()).collect();
    let new_symbols = detect_new_symbols(diff_lines, source, &existing_names);

    // ── Build change list ──────────────────────────────────────────────────
    let mut changes: Vec<SymbolChangeDetail> = Vec::new();
    let ordered_names: Vec<&str> = index.symbols.iter().map(|s| s.name.as_str()).collect();

    // Existing symbols that changed
    for sym_name in &changed_names {
        let orig = all_symbols.iter().find(|s| s.name == *sym_name)?;
        let stable_change_type = if status == "A" {
            "added"
        } else if status == "D" {
            "removed"
        } else {
            "modified"
        };

        let full_body = match status {
            "D" => String::new(),
            _ => extract_full_body(sym_name, &orig.kind, source),
        };

        let (context_before, context_after) = match status {
            "D" => (vec![], vec![]),
            _ => extract_context(source, sym_name, &orig.kind, CONTEXT_WINDOW),
        };

        let position = compute_relative_position(sym_name, &ordered_names, index);

        changes.push(SymbolChangeDetail {
            symbol: sym_name.clone(),
            kind: orig.kind.clone(),
            change_type: stable_change_type.to_string(),
            full_body,
            context_before,
            context_after,
            position,
        });
    }

    // New symbols added post-analysis
    for (new_name, new_kind) in &new_symbols {
        let full_body = extract_full_body(new_name, new_kind, source);
        let (context_before, context_after) =
            extract_context(source, new_name, new_kind, CONTEXT_WINDOW);
        changes.push(SymbolChangeDetail {
            symbol: new_name.clone(),
            kind: new_kind.clone(),
            change_type: "added".to_string(),
            full_body,
            context_before,
            context_after,
            position: RelativePosition {
                parent: None,
                above: None,
                below: None,
            },
        });
    }

    if changes.is_empty() {
        return None;
    }

    Some(FileChangeGroup {
        file: file_path.to_string(),
        source_attached: false,
        changes,
        recommendations: vec![],
    })
}

/// Scan diff lines for newly added symbol declarations that do NOT exist in the
/// stored symbol index (i.e., were added post-analysis).
fn detect_new_symbols(
    diff_lines: &[DiffLine],
    _source: &str,
    existing_names: &HashSet<&str>,
) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for dl in diff_lines {
        if dl.kind != '+' {
            continue;
        }
        if let Some((name, kind)) = extract_declaration_from_line(&dl.content)
            && !existing_names.contains(name.as_str())
            && !found.iter().any(|(n, _)| n == &name)
        {
            found.push((name, kind));
        }
    }
    found
}

/// Try to extract a symbol declaration from a single source line.
/// Returns `(name, kind)` if the line contains a known declaration pattern.
fn extract_declaration_from_line(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();

    // Strip export / export default / pub prefixes
    let body = trimmed
        .strip_prefix("export default ")
        .or_else(|| trimmed.strip_prefix("export "))
        .or_else(|| trimmed.strip_prefix("pub("))
        .and_then(|s| {
            // For `pub(crate) fn` etc, strip the visibility part
            if s.starts_with("crate) ") || s.starts_with("super) ") || s.starts_with("self) ") {
                s.split_once(' ').map(|(_, rest)| rest)
            } else {
                Some(s)
            }
        })
        .or_else(|| trimmed.strip_prefix("pub "))
        .unwrap_or(trimmed);

    let body = body.trim();

    // function name / async function name
    if let Some(rest) = body.strip_prefix("async function ") {
        let name = rest.trim_start().split([' ', '(', '<']).next()?;
        if !name.is_empty() {
            return Some((name.to_string(), "function".to_string()));
        }
    }
    if let Some(rest) = body.strip_prefix("function ") {
        let name = rest.trim_start().split([' ', '(', '<']).next()?;
        if !name.is_empty() {
            return Some((name.to_string(), "function".to_string()));
        }
    }
    // Rust: fn name
    if let Some(rest) = body.strip_prefix("fn ") {
        let name = rest.trim_start().split([' ', '(', '<']).next()?;
        if !name.is_empty() {
            return Some((name.to_string(), "function".to_string()));
        }
    }

    // class name
    if let Some(rest) = body.strip_prefix("class ") {
        let name = rest.trim_start().split([' ', '{', '<', '(', ':']).next()?;
        if !name.is_empty() {
            return Some((name.to_string(), "class".to_string()));
        }
    }

    // interface name
    if let Some(rest) = body.strip_prefix("interface ") {
        let name = rest.trim_start().split([' ', '{', '<']).next()?;
        if !name.is_empty() {
            return Some((name.to_string(), "interface".to_string()));
        }
    }

    // type name
    if let Some(rest) = body.strip_prefix("type ") {
        let name = rest.trim_start().split([' ', '=']).next()?;
        if !name.is_empty() {
            return Some((name.to_string(), "type_alias".to_string()));
        }
    }

    // enum name
    if let Some(rest) = body.strip_prefix("enum ") {
        let name = rest.trim_start().split([' ', '{']).next()?;
        if !name.is_empty() {
            return Some((name.to_string(), "enum".to_string()));
        }
    }

    // const/let/var name = …  (arrow function or variable)
    for prefix in &["const ", "let ", "var "] {
        if let Some(rest) = body.strip_prefix(prefix) {
            // Extract the identifier — split on any of ' ', '=', ':', '<', '(', '`'
            let name = rest
                .trim_start()
                .split([' ', '=', ':', '<', '(', '`'])
                .next()?;
            if name.is_empty() || name == "=" {
                continue;
            }
            // Determine kind: if value contains => or function(, it's an arrow_function
            let after_name = rest[name.len()..].trim();
            if let Some(val) = after_name.strip_prefix('=') {
                let val = val.trim();
                let kind = if val.starts_with("=>")
                    || val.starts_with('(')
                    || val.starts_with("async")
                    || val.starts_with("function")
                {
                    "arrow_function"
                } else if val.starts_with("class") {
                    "class"
                } else {
                    "variable"
                };
                return Some((name.to_string(), kind.to_string()));
            }
            // TypeScript type annotation without assignment — const name: Type;
            if after_name.starts_with(':') || after_name.starts_with(' ') {
                return Some((name.to_string(), "variable".to_string()));
            }
        }
    }

    None
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
        let search: Vec<usize> = source
            .match_indices(kw)
            .filter_map(|(pos, _)| {
                let after_kw = &source[pos + kw.len()..];
                // Check if symbol name follows the keyword
                after_kw
                    .trim_start()
                    .starts_with(symbol_name)
                    .then_some(pos)
            })
            .collect();

        if let Some(&pos) = search.first() {
            start_pos = Some(pos);
            break;
        }
    }

    // Fallback: search for symbol name directly (arrow functions, exports)
    if start_pos.is_none() {
        start_pos = source
            .find(&format!("{} ", symbol_name))
            .or_else(|| source.find(&format!("{}({}", symbol_name, symbol_name)))
            .or_else(|| source.find(&format!("{}.{}", symbol_name, symbol_name)));
    }

    let start = match start_pos {
        Some(p) => p,
        None => return String::new(),
    };

    // Find the first top-level brace (skip braces inside parens/angle brackets
    // — e.g. `function f(param: { type }) { body }` should find the body brace)
    let from_start = &source[start..];
    let brace_start = find_first_top_level_brace(from_start);

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
            rest.find('\n')
                .map(|n| start + n + 1)
                .unwrap_or(source.len())
        }
    };

    source[start..end].to_string()
}

/// Find the first `{` that is not nested inside parentheses `(...)` or angle brackets
/// `<...>`. This avoids matching type annotation braces like `{ body: string }`
/// inside function parameters.
fn find_first_top_level_brace(text: &str) -> Option<usize> {
    let mut paren_depth: u32 = 0;
    let mut angle_depth: u32 = 0;
    for (i, ch) in text.char_indices() {
        match ch {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '{' if paren_depth == 0 && angle_depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

/// Extract a few lines of surrounding context for a changed symbol.
///
/// `source` is the reconstructed new-file content. The function re-uses the
/// same keyword-matching heuristic as [`extract_full_body`] to locate the symbol
/// start position, then returns up to `window` lines before and after it.
fn extract_context(
    source: &str,
    symbol_name: &str,
    kind: &str,
    window: usize,
) -> (Vec<String>, Vec<String>) {
    // Determine the declaration keyword(s) for the given kind (mirrors extract_full_body)
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

    let mut start_pos = None;
    for &kw in keywords {
        let search: Vec<usize> = source
            .match_indices(kw)
            .filter_map(|(pos, _)| {
                let after_kw = &source[pos + kw.len()..];
                after_kw
                    .trim_start()
                    .starts_with(symbol_name)
                    .then_some(pos)
            })
            .collect();
        if let Some(&pos) = search.first() {
            start_pos = Some(pos);
            break;
        }
    }

    if start_pos.is_none() {
        start_pos = source
            .find(&format!("{} ", symbol_name))
            .or_else(|| source.find(&format!("{}({}", symbol_name, symbol_name)))
            .or_else(|| source.find(&format!("{}.{}", symbol_name, symbol_name)));
    }

    let start = match start_pos {
        Some(p) => p,
        None => return (vec![], vec![]),
    };

    // Count newlines before start_pos to get 0-indexed line number
    let line_idx = source[..start].matches('\n').count();
    let lines: Vec<&str> = source.lines().collect();

    let before_start = line_idx.saturating_sub(window);
    let before: Vec<String> = lines[before_start..line_idx]
        .iter()
        .map(|l| l.to_string())
        .collect();

    let after_end = (line_idx + 1 + window).min(lines.len());
    let after: Vec<String> = lines[line_idx + 1..after_end]
        .iter()
        .map(|l| l.to_string())
        .collect();

    (before, after)
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
            if ch == '\n' {
                in_line_comment = false;
            }
            continue;
        }
        if in_block_comment {
            if ch == '*' && text[i + 1..].starts_with('/') {
                in_block_comment = false;
            }
            continue;
        }
        if in_string {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if ch == string_char {
                in_string = false;
            }
            continue;
        }
        match ch {
            '/' if text[i + 1..].starts_with('/') => {
                in_line_comment = true;
            }
            '/' if text[i + 1..].starts_with('*') => {
                in_block_comment = true;
            }
            '"' | '\'' | '`' => {
                in_string = true;
                string_char = ch;
            }
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
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
        if p > 0 {
            Some(ordered_names[p - 1].to_string())
        } else {
            None
        }
    });
    let below = pos.and_then(|p| {
        if p + 1 < ordered_names.len() {
            Some(ordered_names[p + 1].to_string())
        } else {
            None
        }
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
            if let Some(grandparent) =
                find_parent_recursive(name, &sym.children, &sym.name, &sym.kind)
            {
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
        if !child.children.is_empty()
            && let Some(result) =
                find_parent_recursive(name, &child.children, &child.name, &child.kind)
        {
            return Some(result);
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
                    if let Some(file) = dependent_symbol.rsplit_once(':').map(|x| x.0) {
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
    let p = Path::new(src);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        project_root.join(p)
    }
}

/// Fetch diff between two versions. Priority:
///   1. Remote fetch if `source_repo` is configured in migration.toml
///   2. Local git strategies with graceful degradation
///   3. Clear error if all fail
fn fetch_diff(
    source_repo: Option<&str>,
    from_version: Option<&str>,
    to_version: &str,
    source_path: Option<&str>,
    project_root: &Path,
) -> anyhow::Result<String> {
    let from = from_version.unwrap_or("HEAD");

    // Strategy 1: remote fetch (if source_repo is configured)
    if let Some(repo) = source_repo
        && !repo.is_empty()
    {
        println!(
            "  Fetching diff from remote: {} {}..{}",
            repo, from, to_version
        );
        match fetch_remote_diff(repo, from, to_version) {
            Ok(diff) => return Ok(diff),
            Err(e) => {
                eprintln!("  Remote fetch failed: {}", e);
                // fall through to local strategies
            }
        }
    }

    // Strategy 2+: local strategies
    let src = source_path.ok_or_else(|| {
        anyhow::anyhow!(
            "No project.source in migration.toml and remote fetch unavailable.\n\
             Set source = \"path/to/local/repo\" or source_repo = \"https://...\" in migration.toml."
        )
    })?;
    let candidate = resolve_source_path(src, project_root);
    let has_git = candidate.join(".git").exists() || candidate.join("HEAD").exists();
    if !has_git {
        anyhow::bail!(
            "Not a git repository: {}\n\
             Set project.source in migration.toml to the local clone of the source repo.",
            candidate.display()
        );
    }

    println!("  Using local source at: {}", candidate.display());

    // Strategy 2a: normal range diff `from..to`
    let range = format!("{}..{}", from, to_version);
    let output = std::process::Command::new("git")
        .args(["diff", "-U9999", &range])
        .current_dir(&candidate)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run git: {}", e))?;

    if output.status.success() {
        let diff = String::from_utf8_lossy(&output.stdout).to_string();
        if !diff.trim().is_empty() {
            return Ok(diff);
        }
    }

    // Strategy 2b: check which version is missing
    let from_exists = std::process::Command::new("git")
        .args(["cat-file", "-e", from])
        .current_dir(&candidate)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    let to_exists = std::process::Command::new("git")
        .args(["cat-file", "-e", to_version])
        .current_dir(&candidate)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Strategy 2c: if only `to` exists, show just that commit
    if to_exists && !from_exists {
        eprintln!(
            "  Warning: base version {} not found in local history, showing only the {} commit.",
            from, to_version
        );
        let output = std::process::Command::new("git")
            .args([
                "diff",
                "-U9999",
                &format!("{}^..{}", to_version, to_version),
            ])
            .current_dir(&candidate)
            .output()?;
        if output.status.success() {
            let diff = String::from_utf8_lossy(&output.stdout).to_string();
            if !diff.trim().is_empty() {
                return Ok(diff);
            }
        }
    }

    // Strategy 2d: if only `from` exists, show what changed since `from` vs working tree
    if from_exists && !to_exists {
        let output = std::process::Command::new("git")
            .args(["diff", "-U9999", from, "--"])
            .current_dir(&candidate)
            .output()?;
        if output.status.success() {
            let diff = String::from_utf8_lossy(&output.stdout).to_string();
            if !diff.trim().is_empty() {
                eprintln!(
                    "  Warning: target version {} not found, showing changes since {}.",
                    to_version, from
                );
                return Ok(diff);
            }
        }
    }

    // Strategy 2e: show all changes in the working tree relative to `to`
    if to_exists {
        let output = std::process::Command::new("git")
            .args(["diff", "-U9999", to_version, "--stat"])
            .current_dir(&candidate)
            .output()?;
        if output.status.success() {
            let stat = String::from_utf8_lossy(&output.stdout).to_string();
            return Ok(stat);
        }
    }

    anyhow::bail!(
        "Cannot compute diff for {}..{} in {}.\n\
         Nonexistent versions: {}\n\
         Hint: ensure both versions exist in local git history, or use a branch/tag name.\n\
         If you have configured source_repo, check that the remote is reachable.",
        from,
        to_version,
        candidate.display(),
        match (from_exists, to_exists) {
            (false, false) => format!("both {} and {}", from, to_version),
            (false, true) => format!("base version {}", from),
            (true, false) => format!("target version {}", to_version),
            (true, true) => "neither (unexpected)".to_string(),
        },
    );
}

fn fetch_remote_diff(repo: &str, from: &str, to: &str) -> anyhow::Result<String> {
    let tmp_dir = create_temp_dir()?;
    let result = fetch_diff_internal(&tmp_dir, repo, from, to);
    let _ = std::fs::remove_dir_all(&tmp_dir);
    result
}

fn fetch_diff_internal(tmp_dir: &Path, repo: &str, from: &str, to: &str) -> anyhow::Result<String> {
    let init = std::process::Command::new("git")
        .args(["init"])
        .current_dir(tmp_dir)
        .output()?;
    if !init.status.success() {
        anyhow::bail!("git init failed in temp dir");
    }

    let add_remote = std::process::Command::new("git")
        .args(["remote", "add", "origin", repo])
        .current_dir(tmp_dir)
        .output()?;
    if !add_remote.status.success() {
        let stderr = String::from_utf8_lossy(&add_remote.stderr);
        anyhow::bail!("git remote add failed: {}", stderr);
    }

    // Fetch the branch that contains both commits (default: main)
    let branch = "main";
    let fetch = std::process::Command::new("git")
        .args(["fetch", "origin", branch, "--depth", "10"])
        .current_dir(tmp_dir)
        .output()?;
    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        // Try HEAD as fallback
        let fetch2 = std::process::Command::new("git")
            .args(["fetch", "origin", "HEAD", "--depth", "10"])
            .current_dir(tmp_dir)
            .output()?;
        if !fetch2.status.success() {
            let stderr2 = String::from_utf8_lossy(&fetch2.stderr);
            anyhow::bail!("git fetch failed: {} / {}", stderr, stderr2);
        }
    }

    // Now both commits should be in the local history
    let range = format!("{}..{}", from, to);
    let diff = std::process::Command::new("git")
        .args(["diff", "-U9999", &range])
        .current_dir(tmp_dir)
        .output()?;
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
        let dir = base.join(i.to_string());
        if !dir.exists() {
            std::fs::create_dir_all(&dir)?;
            return Ok(dir);
        }
        i += 1;
    }
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
        if parts.len() < 2 {
            continue;
        }
        let ref_str = parts[1];

        // Extract tag name from refs/tags/<name>
        if let Some(tag) = ref_str.strip_prefix("refs/tags/") {
            // Skip ^{} peeled tags
            if tag.ends_with("^{}") {
                continue;
            }
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
    let latest = tags
        .last()
        .cloned()
        .or_else(|| {
            // Fallback: get HEAD commit hash
            let head_output = std::process::Command::new("git")
                .args(["ls-remote", repo, "HEAD"])
                .output()
                .ok()?;
            if !head_output.status.success() {
                return None;
            }
            let stdout = String::from_utf8_lossy(&head_output.stdout);
            stdout.split_whitespace().next().map(|s| s.to_string())
        })
        .ok_or_else(|| anyhow::anyhow!("No tags or refs found in remote {}", repo))?;

    Ok(latest)
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
    if !current.is_empty() {
        sections.push(current);
    }

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
        if line.starts_with("diff --git") {
            break;
        }
        if let Some(hdr) = line.strip_prefix("@@") {
            // Parse old-file starting line number from @@ -M,N +P,Q @@
            let old_start = hdr
                .split_whitespace()
                .next()
                .and_then(|s| {
                    let s = s.trim_start_matches('-');
                    s.split(',').next().and_then(|n| n.parse::<usize>().ok())
                })
                .unwrap_or(1);
            diff_lines.push(DiffLine {
                kind: '@',
                content: old_start.to_string(),
            });
            continue;
        }
        if line.is_empty() {
            diff_lines.push(DiffLine {
                kind: ' ',
                content: String::new(),
            });
            continue;
        }
        let kind = line.chars().next().unwrap_or(' ');
        let content = &line[1..];
        diff_lines.push(DiffLine {
            kind,
            content: content.to_string(),
        });
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
    let index_path = symbols_dir.join(file_path).join("symbols.json");
    if !index_path.exists() {
        return None;
    }
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

/// Load the stored `recommendations.json` and build a file → recommendations map.
fn load_file_recommendations(report_dir: &Path) -> HashMap<String, Vec<DependencyRecommendation>> {
    let rec_path = report_dir.join(output_paths::external::RECOMMENDATIONS);
    if !rec_path.exists() {
        return HashMap::new();
    }
    let content = match std::fs::read_to_string(&rec_path) {
        Ok(c) => c,
        Err(_) => return HashMap::new(),
    };
    let report: RecommendationReport = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(_) => return HashMap::new(),
    };

    let mut map: HashMap<String, Vec<DependencyRecommendation>> = HashMap::new();
    for dep in &report.dependencies {
        for module in &dep.affected_modules {
            map.entry(module.clone()).or_default().push(dep.clone());
        }
    }
    map
}
