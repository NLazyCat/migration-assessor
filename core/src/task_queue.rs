use crate::db;
use crate::spec_writer::MigrationSpec;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;
use syn::spanned::Spanned;

// ── Types ─────────────────────────────────────────────────────────────────

/// Result of verifying an AI-generated Rust file against its migration spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    pub status: VerificationStatus,
    pub similarity: f64,
    pub checks: Vec<VerificationCheck>,
    /// Dynamically generated next-task instruction (or completion message).
    /// Generated from actual data — never hardcoded.
    pub instruction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VerificationStatus {
    Verified,
    NeedsRevision,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationCheck {
    pub severity: String, // "error" | "warning"
    pub message: String,  // dynamically generated
    pub expected: Option<String>,
    pub actual: Option<String>,
    pub line_hint: Option<usize>,
}

/// Response returned from get_next_task, containing the task data
/// and a dynamically generated instruction for the AI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskResponse {
    pub has_next: bool,
    pub task: Option<TaskDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    pub file: String,
    pub target_path: String,
    pub layer: usize,
    pub migration_effort: String,
    pub progress: String, // e.g. "1/42"
    pub instruction: String,
    pub spec: MigrationSpec,
}

/// A single file's verification result in a batch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVerificationResult {
    pub file: String,
    pub status: VerificationStatus,
    pub similarity: f64,
    pub checks: Vec<VerificationCheck>,
    pub instruction: Option<String>,
}

/// Aggregated result for batch verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchVerificationResult {
    pub total: usize,
    pub verified: usize,
    pub failed: usize,
    pub results: Vec<FileVerificationResult>,
    pub summary: String,
}

// ── TaskQueue engine ──────────────────────────────────────────────────────

pub struct TaskQueue;

impl TaskQueue {
    /// Get the next task for the AI to execute.
    ///
    /// Uses the following logic to build the instruction dynamically:
    /// 1. Load the next pending file from `db::next_pending_task()`
    /// 2. Load its migration spec from `spec/{file}.json`
    /// 3. Generate instruction text from real data
    ///
    /// Returns None when all files are verified.
    pub fn get_next_task(db: &Connection, report_dir: &Path) -> anyhow::Result<Option<TaskResponse>> {
        let task_info = match db::next_pending_task(db)? {
            Some(t) => t,
            None => return Ok(None),
        };

        // Read the spec JSON for this file
        let spec_path = report_dir.join("spec").join(format!("{}.json", task_info.file_path));
        if !spec_path.exists() {
            anyhow::bail!("Spec file not found: {}", spec_path.display());
        }
        let spec_content = std::fs::read_to_string(&spec_path)?;
        let spec: MigrationSpec = serde_json::from_str(&spec_content)?;

        // Mark task as in_progress
        db::start_task(db, &task_info.file_path)?;

        // Build progress string
        let completed = task_info.completed_count + 1; // current task is being started
        let progress_str = format!("{}/{}", completed, task_info.total_modules);

        // Dynamically generate instruction
        let dependency_info = if spec.imports.relative.is_empty() {
            "This file has no local dependencies.".to_string()
        } else {
            let deps: Vec<&str> = spec.imports.relative.iter().map(|i| i.from.as_str()).collect();
            format!("Depends on: {}.", deps.join(", "))
        };

        let consumer_info = if spec.referenced_by.is_empty() {
            "No other modules depend on this file.".to_string()
        } else {
            format!("{} other module(s) import from this file.", spec.referenced_by.len())
        };

        let effort_note = match spec.migration_effort.as_str() {
            "trivial" => "This is a straightforward migration with high compatibility.",
            "moderate" => "This module requires moderate effort — check type mappings carefully.",
            "heavy" => "This module is complex — review the translated signatures closely.",
            _ => "Review the spec details before migrating.",
        };

        let instruction = format!(
            "Migrate `{src}` to `{target}` (layer {layer}, effort: {effort}). \
             {effort_note} \
             {dependency_info} \
             {consumer_info} \
             {symbol_hint}",
            src = spec.file,
            target = spec.target_path,
            layer = spec.layer,
            effort = spec.migration_effort,
            effort_note = effort_note,
            dependency_info = dependency_info,
            consumer_info = consumer_info,
            symbol_hint = if spec.symbols.is_empty() {
                "No exported symbols to expose.".to_string()
            } else {
                let names: Vec<&str> = spec.symbols.iter().map(|s| s.target_name.as_str()).collect();
                format!("Expected exports: {}.", names.join(", "))
            },
        );

        let task_detail = TaskDetail {
            file: spec.file.clone(),
            target_path: spec.target_path.clone(),
            layer: spec.layer,
            migration_effort: spec.migration_effort.clone(),
            progress: progress_str,
            instruction,
            spec,
        };

        Ok(Some(TaskResponse {
            has_next: task_info.pending_count > 1 || task_info.verified_count > 0,
            task: Some(task_detail),
        }))
    }

    /// Verify AI-generated Rust code against the spec for a file.
    ///
    /// **Simple symbol-existence check** (no signature comparison).
    /// For LLM self-verification, see `manifest/symbols-checklist.json`.
    ///
    /// Checks:
    /// 1. Parse the Rust code (syntax check)
    /// 2. For each spec symbol, check it exists in the generated code (name-only, case-insensitive)
    /// 3. Record result in SQLite
    ///
    /// Returns `Verified` if all required symbols are present.
    pub fn verify_file(
        db: &Connection,
        report_dir: &Path,
        file: &str,
        generated_content: &str,
        threshold: f64,
    ) -> anyhow::Result<VerificationResult> {
        // Read the spec JSON
        let spec_path = report_dir.join("spec").join(format!("{}.json", file));
        if !spec_path.exists() {
            anyhow::bail!("Spec file not found: {}", spec_path.display());
        }
        let spec_content = std::fs::read_to_string(&spec_path)?;
        let spec: MigrationSpec = serde_json::from_str(&spec_content)?;

        let mut checks: Vec<VerificationCheck> = Vec::new();

        if spec.symbols.is_empty() {
            // No symbols to check — treat as verified
            let result = VerificationResult {
                status: VerificationStatus::Verified,
                similarity: 1.0,
                checks: vec![],
                instruction: Some("No exported symbols to verify.".to_string()),
            };
            db::record_verification(db, file, true, 1.0)?;
            return Ok(result);
        }

        // Parse the generated Rust code with syn
        let syn_file = syn::parse_file(generated_content);
        let syn_file = match syn_file {
            Ok(f) => f,
            Err(e) => {
                let check = VerificationCheck {
                    severity: "error".to_string(),
                    message: format!("Failed to parse generated Rust code: {}", e),
                    expected: Some("valid Rust code".to_string()),
                    actual: Some(format!("parse error: {}", e)),
                    line_hint: None,
                };
                checks.push(check);
                let result = VerificationResult {
                    status: VerificationStatus::NeedsRevision,
                    similarity: 0.0,
                    checks,
                    instruction: Some("Fix the Rust syntax errors and try again.".to_string()),
                };
                db::record_verification(db, file, false, 0.0)?;
                return Ok(result);
            }
        };

        // Collect all public names from generated Rust code
        /// Normalize a name for case-/separator-insensitive comparison.
        /// "ApiResponse" and "api_response" both normalize to "apiresponse".
        fn norm(name: &str) -> String {
            name.chars()
                .filter(|c| *c != '_')
                .flat_map(|c| c.to_lowercase())
                .collect()
        }

        let rust_names: Vec<String> = syn_file
            .items
            .iter()
            .filter_map(|item| {
                match item {
                    syn::Item::Fn(func) => Some(func.sig.ident.to_string()),
                    syn::Item::Struct(s) => Some(s.ident.to_string()),
                    syn::Item::Enum(e) => Some(e.ident.to_string()),
                    syn::Item::Trait(t) => Some(t.ident.to_string()),
                    syn::Item::Const(c) => Some(c.ident.to_string()),
                    syn::Item::Static(st) => Some(st.ident.to_string()),
                    syn::Item::Type(ty) => Some(ty.ident.to_string()),
                    syn::Item::Impl(imp) => {
                        // Self type name
                        if let syn::Type::Path(type_path) = &*imp.self_ty
                            && let Some(segment) = type_path.path.segments.last()
                        {
                            Some(segment.ident.to_string())
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .collect();

        // Check each exported symbol from the spec
        let matched_count = spec
            .symbols
            .iter()
            .filter(|sym| {
                let norm_target = norm(&sym.target_name);
                rust_names.iter().any(|n| norm(n) == norm_target)
            })
            .count();

        // Build checks for missing symbols
        for symbol in &spec.symbols {
            let norm_target = norm(&symbol.target_name);
            let exists = rust_names.iter().any(|n| norm(n) == norm_target);
            if !exists {
                checks.push(VerificationCheck {
                    severity: "error".to_string(),
                    message: format!(
                        "Exported symbol '{}' (target: '{}') does not exist in generated Rust code",
                        symbol.name, symbol.target_name
                    ),
                    expected: Some(format!("fn {} or struct {} or etc.", symbol.target_name, symbol.target_name)),
                    actual: None,
                    line_hint: None,
                });
            }
        }

        // Simple similarity: % of required symbols found
        let similarity = matched_count as f64 / spec.symbols.len().max(1) as f64;

        // Determine status: pass if all symbols present AND similarity >= threshold
        let passed = similarity >= threshold && checks.iter().all(|c| c.severity != "error");
        let status = if passed {
            VerificationStatus::Verified
        } else {
            VerificationStatus::NeedsRevision
        };

        // Generate instruction
        let instruction = if passed {
            Some(format!(
                "File '{}' verified successfully — {}/{} required symbols present.",
                file, matched_count, spec.symbols.len()
            ))
        } else {
            Some(format!(
                "File '{}' needs revision — {}/{} required symbols present. {} missing: {}",
                file,
                matched_count,
                spec.symbols.len(),
                checks.len(),
                checks.iter().map(|c| c.message.as_str()).collect::<Vec<_>>().join("; "),
            ))
        };

        let result = VerificationResult {
            status,
            similarity,
            checks,
            instruction,
        };

        db::record_verification(db, file, passed, similarity)?;

        Ok(result)
    }

    /// Deep AST verification — symbol-by-symbol structural matching.
    ///
    /// For each spec symbol, parses the generated Rust code and checks:
    /// - **Functions**: parameter count, each param name/type, return type, async
    /// - **Structs**: field names and types
    /// - **Enums**: variant names
    /// - **Type aliases**: existence
    /// - **Traits**: method signatures
    ///
    /// Produces detailed warnings for structural mismatches beyond simple name existence.
    pub fn verify_file_ast(
        db: &Connection,
        report_dir: &Path,
        file: &str,
        generated_content: &str,
        threshold: f64,
    ) -> anyhow::Result<VerificationResult> {
        let spec_path = report_dir.join("spec").join(format!("{}.json", file));
        if !spec_path.exists() {
            anyhow::bail!("Spec file not found: {}", spec_path.display());
        }
        let spec_content = std::fs::read_to_string(&spec_path)?;
        let spec: MigrationSpec = serde_json::from_str(&spec_content)?;

        let mut checks: Vec<VerificationCheck> = Vec::new();

        if spec.symbols.is_empty() {
            let result = VerificationResult {
                status: VerificationStatus::Verified,
                similarity: 1.0,
                checks: vec![],
                instruction: Some("No exported symbols to verify.".to_string()),
            };
            db::record_verification(db, file, true, 1.0)?;
            return Ok(result);
        }

        // Parse generated Rust code
        let syn_file = syn::parse_file(generated_content);
        let syn_file = match syn_file {
            Ok(f) => f,
            Err(e) => {
                let check = VerificationCheck {
                    severity: "error".to_string(),
                    message: format!("Failed to parse generated Rust code: {}", e),
                    expected: Some("valid Rust code".to_string()),
                    actual: Some(format!("parse error: {}", e)),
                    line_hint: None,
                };
                checks.push(check);
                let result = VerificationResult {
                    status: VerificationStatus::NeedsRevision,
                    similarity: 0.0,
                    checks,
                    instruction: Some("Fix the Rust syntax errors and try again.".to_string()),
                };
                db::record_verification(db, file, false, 0.0)?;
                return Ok(result);
            }
        };

        /// Normalize name for case-/separator-insensitive matching.
        fn norm(name: &str) -> String {
            name.chars()
                .filter(|c| *c != '_')
                .flat_map(|c| c.to_lowercase())
                .collect()
        }

        /// Acceptable cross-language kind mappings.
        /// TypeScript kinds → Rust kinds that are considered correct translations.
        fn is_acceptable_kind(expected: &str, actual: &str) -> bool {
            matches!(
                (expected, actual),
                ("interface", "struct")
                    | ("interface", "enum")
                    | ("type_alias", "enum")
                    | ("type_alias", "struct")
                    | ("type_alias", "type")
                    | ("variable", "const")
                    | ("variable", "static")
                    | ("enum", "struct") // TS string enum → Rust struct with constants
            )
        }

        /// Convert a syn type to a debug string for display comparison.
        fn describe_type(ty: &syn::Type) -> String {
            use quote::ToTokens;
            let s = ty.to_token_stream().to_string();
            // Collapse whitespace
            s.split_whitespace().collect::<Vec<_>>().join(" ")
        }

        // Build a lookup map from the Rust AST: normalized name -> (item, line)
        let mut rust_items: std::collections::HashMap<String, (&syn::Item, usize)> =
            std::collections::HashMap::new();
        for item in &syn_file.items {
            let (name, line) = match item {
                syn::Item::Fn(f) => (f.sig.ident.to_string(), f.sig.ident.span().start().line),
                syn::Item::Struct(s) => (s.ident.to_string(), s.ident.span().start().line),
                syn::Item::Enum(e) => (e.ident.to_string(), e.ident.span().start().line),
                syn::Item::Trait(t) => (t.ident.to_string(), t.ident.span().start().line),
                syn::Item::Const(c) => (c.ident.to_string(), c.ident.span().start().line),
                syn::Item::Static(st) => (st.ident.to_string(), st.ident.span().start().line),
                syn::Item::Type(ty) => (ty.ident.to_string(), ty.ident.span().start().line),
                syn::Item::Impl(imp) => {
                    if let syn::Type::Path(type_path) = &*imp.self_ty
                        && let Some(segment) = type_path.path.segments.last()
                    {
                        (segment.ident.to_string(), imp.self_ty.span().start().line)
                    } else {
                        continue;
                    }
                }
                _ => continue,
            };
            let nk = norm(&name);
            // Keep first occurrence; warn about duplicates via an entry merge below
            rust_items.entry(nk).or_insert((item, line));
        }

        // ── Structural comparison ────────────────────────────────
        let mut matched_count = 0usize;
        let mut detail_score = 0usize;

        for symbol in &spec.symbols {
            let norm_target = norm(&symbol.target_name);

            let Some((found_item, found_line)) = rust_items.get(&norm_target) else {
                // Symbol missing entirely
                checks.push(VerificationCheck {
                    severity: "error".to_string(),
                    message: format!(
                        "Exported symbol '{}' (target: '{}') not found in generated Rust code",
                        symbol.name, symbol.target_name
                    ),
                    expected: Some(format!("{} named '{}'", symbol.kind, symbol.target_name)),
                    actual: None,
                    line_hint: None,
                });
                continue;
            };

            matched_count += 1;
            let mut item_checks: Vec<String> = Vec::new();

            // Compare kind (allow acceptable cross-language translations)
            let actual_kind = rust_item_kind(found_item);
            if actual_kind != symbol.kind && !is_acceptable_kind(&symbol.kind, &actual_kind) {
                item_checks.push(format!("kind mismatch: expected '{}', got '{}'", symbol.kind, actual_kind));
            }

            match (symbol.kind.as_str(), found_item) {
                ("function", syn::Item::Fn(func)) => {
                    // --- Params ---
                    let rust_params: Vec<&syn::FnArg> = func.sig.inputs.iter().collect();
                    let spec_params = &symbol.params;

                    // self param doesn't count in spec
                    let rust_param_count = rust_params.iter().filter(|a| !matches!(a, syn::FnArg::Receiver(_))).count();

                    if rust_param_count != spec_params.len() {
                        item_checks.push(format!(
                            "parameter count: expected {}, got {}",
                            spec_params.len(),
                            rust_param_count
                        ));
                    }

                    // Compare each param by position (skip self)
                    let rust_positional: Vec<&syn::FnArg> = rust_params
                        .iter()
                        .filter(|a| !matches!(a, syn::FnArg::Receiver(_)))
                        .copied()
                        .collect();

                    for (i, spec_p) in spec_params.iter().enumerate() {
                        if let Some(syn::FnArg::Typed(pat_type)) = rust_positional.get(i) {
                            let rust_name = param_name_str(&pat_type.pat);
                            let rust_type = describe_type(&pat_type.ty);

                            // Name check (case-insensitive)
                            if let Some(ref rn) = rust_name {
                                if norm(rn) != norm(&spec_p.name) {
                                    item_checks.push(format!(
                                        "param[{}] name: expected '{}', got '{}'",
                                        i, spec_p.name, rn
                                    ));
                                }
                            }

                            // Type check (basic string comparison)
                            let rust_type_norm = rust_type.replace(' ', "");
                            let spec_type_norm = spec_p.ty.replace(' ', "");
                            if !spec_type_norm.is_empty() && rust_type_norm != spec_type_norm {
                                item_checks.push(format!(
                                    "param[{}] '{}' type: expected '{}', got '{}'",
                                    i, spec_p.name, spec_p.ty, rust_type
                                ));
                            }
                        }
                    }

                    // --- Return type ---
                    match (&func.sig.output, &symbol.return_type) {
                        (syn::ReturnType::Type(_, ret_ty), Some(spec_ret)) => {
                            let rust_ret = describe_type(ret_ty);
                            let rust_ret_norm = rust_ret.replace(' ', "");
                            let spec_ret_norm = spec_ret.replace(' ', "");
                            if !spec_ret_norm.is_empty() && rust_ret_norm != spec_ret_norm {
                                item_checks.push(format!(
                                    "return type: expected '{}', got '{}'",
                                    spec_ret, rust_ret
                                ));
                            }
                        }
                        (syn::ReturnType::Default, Some(_)) => {
                            item_checks.push(format!(
                                "return type: expected '{}', got unit ()",
                                symbol.return_type.as_ref().unwrap()
                            ));
                        }
                        (syn::ReturnType::Type(_, _), None) => {
                            item_checks.push("unexpected return type (spec has none)".to_string());
                        }
                        _ => {}
                    }

                    // --- Async ---
                    let is_async = func.sig.asyncness.is_some();
                    if let Some(spec_async) = symbol.is_async {
                        if is_async != spec_async {
                            item_checks.push(format!(
                                "async: expected {}, got {}",
                                spec_async, is_async
                            ));
                        }
                    }
                }

                ("struct", syn::Item::Struct(s)) => {
                    if let syn::Fields::Named(ref fields) = s.fields {
                        // For structs, compare field names from spec hints
                        let spec_detail: Vec<&str> = symbol
                            .target_signature
                            .as_deref()
                            .unwrap_or("")
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .collect();

                        let rust_field_names: Vec<String> = fields
                            .named
                            .iter()
                            .map(|f| f.ident.as_ref().map(|i| i.to_string()).unwrap_or_default())
                            .filter(|n| !n.is_empty())
                            .collect();

                        // Check field count
                        if !spec_detail.is_empty() && rust_field_names.len() != spec_detail.len() {
                            item_checks.push(format!(
                                "field count: expected {}, got {}",
                                spec_detail.len(),
                                rust_field_names.len()
                            ));
                        }
                    }
                }

                ("enum", syn::Item::Enum(e)) => {
                    let variant_names: Vec<String> = e
                        .variants
                        .iter()
                        .map(|v| v.ident.to_string())
                        .collect();
                    // Just check we found the enum — that's already counted in matched_count
                    if variant_names.is_empty() {
                        item_checks.push("enum has no variants".to_string());
                    }
                }

                _ => {
                    // For types, constants, statics, traits, impls — existence is sufficient
                }
            }

            // Combine item-level checks into a single verification check
            if !item_checks.is_empty() {
                checks.push(VerificationCheck {
                    severity: "warning".to_string(),
                    message: format!(
                        "Symbol '{}' (target: '{}') has structural mismatches: {}",
                        symbol.name,
                        symbol.target_name,
                        item_checks.join("; ")
                    ),
                    expected: Some(format!("{} {}", symbol.kind, symbol.target_name)),
                    actual: Some(format!("{} at line {}", actual_kind, *found_line)),
                    line_hint: Some(*found_line),
                });
            } else {
                detail_score += 1;
            }
        }

        // ── Scoring ──────────────────────────────────────────────
        let name_match_ratio = matched_count as f64 / spec.symbols.len().max(1) as f64;
        let structural_ratio = if matched_count > 0 {
            detail_score as f64 / matched_count as f64
        } else {
            0.0
        };
        // Combined score: 60% name match + 40% structural match
        let combined_score = name_match_ratio * 0.6 + structural_ratio * 0.4;

        let status = if combined_score >= threshold && checks.iter().all(|c| c.severity != "error") {
            VerificationStatus::Verified
        } else {
            VerificationStatus::NeedsRevision
        };

        let passed = status == VerificationStatus::Verified;
        let instruction = if passed {
            Some(format!(
                "File '{}' deep-verified — {}/{} symbols match structurally.",
                file, detail_score, spec.symbols.len()
            ))
        } else {
            Some(format!(
                "File '{}' needs revision — {}/{} symbols found, {}/{} structurally correct. Issues: {}",
                file,
                matched_count,
                spec.symbols.len(),
                detail_score,
                matched_count.max(1),
                checks.iter().map(|c| c.message.as_str()).collect::<Vec<_>>().join("; "),
            ))
        };

        let result = VerificationResult {
            status,
            similarity: combined_score,
            checks,
            instruction,
        };

        db::record_verification(db, file, passed, combined_score)?;
        Ok(result)
    }

    /// Verify files in batch — supports single file, directory, or all.
    ///
    /// - `file: &str` — verify a single file (needs `content` param)
    /// - `content: Option<&str>` — Rust code for single-file verify
    /// - `directory: Option<&str>` — verify all spec files under this dir
    /// - `all: bool` — verify all spec files
    /// - `project_path: Option<&Path>` — path to Rust project root (needed for dir/all mode)
    ///
    /// When `directory` or `all` is used, reads Rust code from `{project_path}/{target_path}`.
    pub fn verify_batch(
        db: &Connection,
        report_dir: &Path,
        file: Option<&str>,
        content: Option<&str>,
        directory: Option<&str>,
        all: bool,
        project_path: Option<&Path>,
        threshold: f64,
        deep: bool,
    ) -> anyhow::Result<BatchVerificationResult> {
        // Collect files to verify
        let spec_dir = report_dir.join("spec");

        let files_to_verify: Vec<String> = if let Some(f) = file {
            vec![f.to_string()]
        } else {
            // Collect all spec files, optionally filtered by directory
            let mut specs: Vec<String> = Vec::new();
            collect_specs(&spec_dir, &spec_dir, directory, all, &mut specs)?;
            specs
        };

        if files_to_verify.is_empty() {
            return Ok(BatchVerificationResult {
                total: 0,
                verified: 0,
                failed: 0,
                results: vec![],
                summary: "No spec files found to verify.".to_string(),
            });
        }

        let mut results: Vec<FileVerificationResult> = Vec::new();

        for f in &files_to_verify {
            if let Some(c) = content {
                // Single file mode: use provided content
                let vr = if deep {
                    Self::verify_file_ast(db, report_dir, f, c, threshold)
                } else {
                    Self::verify_file(db, report_dir, f, c, threshold)
                };
                match vr {
                    Ok(vr) => results.push(FileVerificationResult {
                        file: f.clone(),
                        status: vr.status,
                        similarity: vr.similarity,
                        checks: vr.checks,
                        instruction: vr.instruction,
                    }),
                    Err(e) => results.push(FileVerificationResult {
                        file: f.clone(),
                        status: VerificationStatus::NeedsRevision,
                        similarity: 0.0,
                        checks: vec![VerificationCheck {
                            severity: "error".to_string(),
                            message: format!("Verification error: {}", e),
                            expected: None,
                            actual: None,
                            line_hint: None,
                        }],
                        instruction: Some(format!("Error during verification: {}", e)),
                    }),
                }
            } else if let Some(ref proj) = project_path {
                // Dir/all mode: read Rust code from project file tree
                // Derive target .rs path from spec
                let spec_path = spec_dir.join(format!("{}.json", f));
                let spec_content = match std::fs::read_to_string(&spec_path) {
                    Ok(c) => c,
                    Err(e) => {
                        results.push(FileVerificationResult {
                            file: f.clone(),
                            status: VerificationStatus::NeedsRevision,
                            similarity: 0.0,
                            checks: vec![VerificationCheck {
                                severity: "error".to_string(),
                                message: format!("Cannot read spec file: {}", e),
                                expected: None,
                                actual: None,
                                line_hint: None,
                            }],
                            instruction: None,
                        });
                        continue;
                    }
                };
                let spec: MigrationSpec = match serde_json::from_str(&spec_content) {
                    Ok(s) => s,
                    Err(e) => {
                        results.push(FileVerificationResult {
                            file: f.clone(),
                            status: VerificationStatus::NeedsRevision,
                            similarity: 0.0,
                            checks: vec![VerificationCheck {
                                severity: "error".to_string(),
                                message: format!("Invalid spec format: {}", e),
                                expected: None,
                                actual: None,
                                line_hint: None,
                            }],
                            instruction: None,
                        });
                        continue;
                    }
                };
                let rust_path = proj.join(&spec.target_path);
                let rust_content = match std::fs::read_to_string(&rust_path) {
                    Ok(c) => c,
                    Err(e) => {
                        results.push(FileVerificationResult {
                            file: f.clone(),
                            status: VerificationStatus::NeedsRevision,
                            similarity: 0.0,
                            checks: vec![VerificationCheck {
                                severity: "error".to_string(),
                                message: format!("Cannot read Rust file at {}: {}", rust_path.display(), e),
                                expected: None,
                                actual: None,
                                line_hint: None,
                            }],
                            instruction: None,
                        });
                        continue;
                    }
                };
                let vr = if deep {
                    Self::verify_file_ast(db, report_dir, f, &rust_content, threshold)
                } else {
                    Self::verify_file(db, report_dir, f, &rust_content, threshold)
                };
                match vr {
                    Ok(vr) => results.push(FileVerificationResult {
                        file: f.clone(),
                        status: vr.status,
                        similarity: vr.similarity,
                        checks: vr.checks,
                        instruction: vr.instruction,
                    }),
                    Err(e) => results.push(FileVerificationResult {
                        file: f.clone(),
                        status: VerificationStatus::NeedsRevision,
                        similarity: 0.0,
                        checks: vec![VerificationCheck {
                            severity: "error".to_string(),
                            message: format!("Verification error: {}", e),
                            expected: None,
                            actual: None,
                            line_hint: None,
                        }],
                        instruction: Some(format!("Error during verification: {}", e)),
                    }),
                }
            } else {
                anyhow::bail!("For directory/all mode, 'project_path' is required to read Rust files. For single file, provide 'content'.");
            }
        }

        let verified_count = results.iter().filter(|r| r.status == VerificationStatus::Verified).count();
        let failed_count = results.iter().filter(|r| r.status == VerificationStatus::NeedsRevision).count();

        let summary = format!(
            "Batch verification: {}/{} verified, {}/{} failed",
            verified_count, results.len(), failed_count, results.len()
        );

        Ok(BatchVerificationResult {
            total: results.len(),
            verified: verified_count,
            failed: failed_count,
            results,
            summary,
        })
    }
}

/// Recursively collect spec file paths from the spec directory.
/// Filters by `directory` if provided; includes all if `all` is true.
fn collect_specs(
    base_dir: &Path,
    current_dir: &Path,
    directory: Option<&str>,
    _all: bool,
    out: &mut Vec<String>,
) -> anyhow::Result<()> {
    if !current_dir.is_dir() {
        return Ok(());
    }

    for entry in std::fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_specs(base_dir, &path, directory, _all, out)?;
        } else if path.extension().map_or(false, |e| e == "json") {
            // Derive the file path relative to spec dir
            if let Ok(rel) = path.strip_prefix(base_dir) {
                // Remove the .json extension to get the spec file path
                let file_stem = rel.with_extension("");
                let file_str = file_stem.to_string_lossy().replace('\\', "/");

                // If directory filter is set, check if file is under that directory
                if let Some(dir) = directory {
                    let dir_prefix = dir.trim_end_matches('/');
                    if !file_str.starts_with(dir_prefix) {
                        continue;
                    }
                }

                out.push(file_str);
            }
        }
    }

    Ok(())
}

// ── Helper functions for AST verification ──────────────────────────────

/// Get a human-readable kind string for a `syn::Item`.
fn rust_item_kind(item: &syn::Item) -> String {
    match item {
        syn::Item::Fn(_) => "function".to_string(),
        syn::Item::Struct(_) => "struct".to_string(),
        syn::Item::Enum(_) => "enum".to_string(),
        syn::Item::Trait(_) => "trait".to_string(),
        syn::Item::Const(_) => "const".to_string(),
        syn::Item::Static(_) => "static".to_string(),
        syn::Item::Type(_) => "type".to_string(),
        syn::Item::Impl(_) => "impl".to_string(),
        _ => "other".to_string(),
    }
}

/// Extract the name from a `syn::Pat` as an optional string.
fn param_name_str(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(pat_ident) => Some(pat_ident.ident.to_string()),
        syn::Pat::Wild(_) => Some("_".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, open_or_create};
    use crate::scores::{ModuleReadiness, ScoreBreakdown};

    fn setup_db_with_modules() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = open_or_create(&dir.path().join("test.db")).unwrap();
        let modules = vec![
            ModuleReadiness {
                module: "src/types.ts".into(),
                score: 90.0,
                rank: 1,
                in_degree: 1,  // higher in_degree → types are foundational, migrate first
                complexity: 0.1,
                external_compatibility: 1.0,
                cycle_count: 0,
                has_tests: false,
                migration_effort: "trivial".into(),
                breakdown: ScoreBreakdown::default(),
            },
            ModuleReadiness {
                module: "src/utils.ts".into(),
                score: 75.0,
                rank: 2,
                in_degree: 0,  // lower in_degree → depends on types
                complexity: 0.3,
                external_compatibility: 1.0,
                cycle_count: 0,
                has_tests: true,
                migration_effort: "moderate".into(),
                breakdown: ScoreBreakdown::default(),
            },
        ];
        db::write_modules(&conn, &modules).unwrap();
        db::init_task_queue(&conn).unwrap();
        (dir, conn)
    }

    fn write_spec(report_dir: &Path, file: &str, spec: &MigrationSpec) {
        std::fs::create_dir_all(report_dir).unwrap();
        let spec_dir = report_dir.join("spec");
        let spec_file = spec_dir.join(format!("{}.json", file));
        if let Some(parent) = spec_file.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let json = serde_json::to_string_pretty(spec).unwrap();
        std::fs::write(&spec_file, json).unwrap();
    }

    #[test]
    fn test_get_next_task_returns_in_order() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");
        std::fs::create_dir_all(&report_dir).unwrap();

        // Write spec files
        let spec1 = MigrationSpec {
            file: "src/types.ts".into(),
            target_path: "src/types.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: "export interface Foo {}".into(),
            exports: vec![],
            symbols: vec![],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec!["src/utils.ts".into()],
        };
        let spec2 = MigrationSpec {
            file: "src/utils.ts".into(),
            target_path: "src/utils.rs".into(),
            layer: 1,
            migration_effort: "moderate".into(),
            has_tests: true,
            source: "import { Foo } from './types';".into(),
            exports: vec![],
            symbols: vec![],
            imports: crate::spec_writer::SpecImports {
                relative: vec![crate::spec_writer::SpecImport {
                    from: "./types".into(),
                    symbols: vec![],
                    target_import: None,
                    migration_note: None,
                }],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/types.ts", &spec1);
        write_spec(&report_dir, "src/utils.ts", &spec2);

        // Get first task (layer 0 first)
        let resp = TaskQueue::get_next_task(&conn, &report_dir)
            .unwrap()
            .expect("should have task");
        assert_eq!(resp.task.as_ref().unwrap().file, "src/types.ts");
        assert!(resp.task.as_ref().unwrap().instruction.contains("src/types.ts"));
        assert!(resp.task.as_ref().unwrap().instruction.contains("layer"));
        assert!(resp.task.as_ref().unwrap().instruction.contains("trivial"));

        // Mark first as verified
        db::record_verification(&conn, "src/types.ts", true, 0.95).unwrap();

        // Get second task
        let resp = TaskQueue::get_next_task(&conn, &report_dir)
            .unwrap()
            .expect("should have next task");
        assert_eq!(resp.task.as_ref().unwrap().file, "src/utils.ts");
        assert!(resp.task.as_ref().unwrap().instruction.contains("moderate"));
    }

    #[test]
    fn test_get_next_task_all_done() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");
        std::fs::create_dir_all(&report_dir).unwrap();

        // Write minimal spec for each
        for file in &["src/types.ts", "src/utils.ts"] {
            let spec = MigrationSpec {
                file: (*file).into(),
                target_path: file.replace(".ts", ".rs"),
                layer: 0,
                migration_effort: "trivial".into(),
                has_tests: false,
                source: String::new(),
                exports: vec![],
                symbols: vec![],
                imports: crate::spec_writer::SpecImports {
                    relative: vec![],
                    external: vec![],
                },
                referenced_by: vec![],
            };
            write_spec(&report_dir, file, &spec);
        }

        // Mark all as verified
        db::record_verification(&conn, "src/types.ts", true, 0.95).unwrap();
        db::record_verification(&conn, "src/utils.ts", true, 0.95).unwrap();

        // Should return None
        let resp = TaskQueue::get_next_task(&conn, &report_dir).unwrap();
        assert!(resp.is_none());
    }

    #[test]
    fn test_verify_file_all_symbols_match() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        // Write spec with a function symbol
        let spec = MigrationSpec {
            file: "src/utils.ts".into(),
            target_path: "src/utils.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: "export function greet(name: string): string {}".into(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "greet".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn greet(name: String) -> String".into()),
                target_name: "greet".into(),
                target_signature: Some("(name: String) -> String".into()),
                params: vec![crate::spec_writer::SpecParam {
                    name: "name".into(),
                    ty: "String".into(),
                    optional: false,
                }],
                return_type: Some("String".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/utils.ts", &spec);

        // Valid Rust code matching the spec
        let rust_code = r#"
pub fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
"#;
        let result = TaskQueue::verify_file(&conn, &report_dir, "src/utils.ts", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(result.similarity >= 0.7);
    }

    #[test]
    fn test_verify_file_missing_symbol() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/utils.ts".into(),
            target_path: "src/utils.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "formatPrice".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn formatPrice(price: f64) -> String".into()),
                target_name: "format_price".into(),
                target_signature: Some("(price: f64) -> String".into()),
                params: vec![],
                return_type: Some("String".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/utils.ts", &spec);

        // Valid Rust code but MISSING the expected function
        let rust_code = r#"
pub fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}
"#;
        let result = TaskQueue::verify_file(&conn, &report_dir, "src/utils.ts", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::NeedsRevision);
        // Should mention the missing symbol
        assert!(result.checks.iter().any(|c| c.message.contains("format_price")));
    }

    #[test]
    fn test_verify_file_symbol_exists_with_different_signature() {
        // With the simplified verify, different signatures are OK —
        // only symbol existence is checked.
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/utils.ts".into(),
            target_path: "src/utils.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "add".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn add(a: i32, b: i32) -> i32".into()),
                target_name: "add".into(),
                target_signature: Some("(a: i32, b: i32) -> i32".into()),
                params: vec![],
                return_type: Some("i32".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/utils.ts", &spec);

        // Function name matches but signature differs — still passes
        let rust_code = r#"
pub fn add(a: f64, b: f64) -> f64 {
    a + b
}
"#;
        let result = TaskQueue::verify_file(&conn, &report_dir, "src/utils.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!((result.similarity - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_verify_file_invalid_rust_syntax() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/utils.ts".into(),
            target_path: "src/utils.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "greet".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn greet(name: String) -> String".into()),
                target_name: "greet".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/utils.ts", &spec);

        // Invalid Rust code
        let rust_code = "pub fn greet(name: String) -> String {";
        let result = TaskQueue::verify_file(&conn, &report_dir, "src/utils.ts", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::NeedsRevision);
        assert_eq!(result.similarity, 0.0);
    }

    #[test]
    fn test_task_instruction_dynamically_generated() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/types.ts".into(),
            target_path: "src/types.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: true,
            source: "export function foo() {}".into(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "foo".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 1],
                signature: Some("fn foo()".into()),
                target_name: "foo".into(),
                target_signature: Some("()".into()),
                params: vec![],
                return_type: None,
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        // The first module returned is "src/types.ts" (layer 0, score 90)
        write_spec(&report_dir, "src/types.ts", &spec);

        let resp = TaskQueue::get_next_task(&conn, &report_dir)
            .unwrap()
            .expect("should have task");
        let instruction = &resp.task.as_ref().unwrap().instruction;
        // Instruction must contain the actual file name
        assert!(
            instruction.contains("src/types.ts"),
            "Instruction should contain file name: {}",
            instruction
        );
        // Instruction must contain layer info
        assert!(
            instruction.contains("layer"),
            "Instruction should contain layer info: {}",
            instruction
        );
        // Instruction must contain the effort label
        assert!(
            instruction.contains("trivial"),
            "Instruction should contain effort label: {}",
            instruction
        );
        // Instruction must not contain placeholders
        assert!(
            !instruction.contains("{{") && !instruction.contains("}}"),
            "Instruction should not contain template placeholders: {}",
            instruction
        );
        assert!(
            !instruction.contains("{file}") && !instruction.contains("{layer}"),
            "Instruction should not contain placeholder tokens: {}",
            instruction
        );
    }

    // ── Comprehensive verification test with real spec data ────────────

    /// Build a spec resembling `src/types/api.ts` from the ts-test-project:
    /// 6 symbols (3 interfaces + 2 functions + 1 interface) with snake_case names.
    fn make_api_spec() -> MigrationSpec {
        MigrationSpec {
            file: "src/types/api.ts".into(),
            target_path: "src/types/api.rs".into(),
            layer: 3,
            migration_effort: "moderate".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![
                crate::spec_writer::SpecSymbol {
                    name: "ApiResponse".into(),
                    kind: "interface".into(),
                    visibility: "Public".into(),
                    line_range: [3, 8],
                    signature: None,
                    target_name: "api_response".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
                crate::spec_writer::SpecSymbol {
                    name: "ApiError".into(),
                    kind: "interface".into(),
                    visibility: "Public".into(),
                    line_range: [10, 14],
                    signature: None,
                    target_name: "api_error".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
                crate::spec_writer::SpecSymbol {
                    name: "PaginationMeta".into(),
                    kind: "interface".into(),
                    visibility: "Public".into(),
                    line_range: [16, 21],
                    signature: None,
                    target_name: "pagination_meta".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
                crate::spec_writer::SpecSymbol {
                    name: "createSuccessResponse".into(),
                    kind: "function".into(),
                    visibility: "Public".into(),
                    line_range: [23, 25],
                    signature: Some("export function createSuccessResponse(data: T, meta: PaginationMeta) -> ApiResponse<T>".into()),
                    target_name: "create_success_response".into(),
                    target_signature: Some("export function createSuccessResponse(data: T, meta: PaginationMeta) -> ApiResponse<T>".into()),
                    params: vec![],
                    return_type: Some("ApiResponse<T>".into()),
                    is_async: Some(false),
                    migration_note: None,
                },
                crate::spec_writer::SpecSymbol {
                    name: "createErrorResponse".into(),
                    kind: "function".into(),
                    visibility: "Public".into(),
                    line_range: [27, 29],
                    signature: Some("export function createErrorResponse(code: string, message: string, details: Record<string, string[]>) -> ApiResponse<never>".into()),
                    target_name: "create_error_response".into(),
                    target_signature: Some("export function createErrorResponse(code: string, message: string, details: Record<string, string[]>) -> ApiResponse<never>".into()),
                    params: vec![],
                    return_type: Some("ApiResponse<never>".into()),
                    is_async: Some(false),
                    migration_note: None,
                },
                crate::spec_writer::SpecSymbol {
                    name: "HealthCheckResponse".into(),
                    kind: "interface".into(),
                    visibility: "Public".into(),
                    line_range: [31, 36],
                    signature: None,
                    target_name: "health_check_response".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
            ],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        }
    }

    #[test]
    fn test_verify_file_real_scale_all_symbols_match() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_api_spec();
        write_spec(&report_dir, "src/types/api.ts", &spec);

        // Rust code matching all 6 symbols (interfaces → structs, functions matched)
        let rust_code = r#"
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    pub meta: Option<PaginationMeta>,
}

pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<Vec<String>>,
}

pub struct PaginationMeta {
    pub page: u32,
    pub limit: u32,
    pub total: u32,
    pub total_pages: u32,
}

pub fn create_success_response<T>(data: T, meta: PaginationMeta) -> ApiResponse<T> {
    ApiResponse {
        success: true,
        data: Some(data),
        error: None,
        meta: Some(meta),
    }
}

pub fn create_error_response(code: String, message: String, details: Option<Vec<String>>) -> ApiResponse<()> {
    ApiResponse {
        success: false,
        data: None,
        error: Some(ApiError { code, message, details }),
        meta: None,
    }
}

pub struct HealthCheckResponse {
    pub status: String,
    pub uptime: f64,
    pub version: String,
    pub timestamp: String,
}
"#;

        let result = TaskQueue::verify_file(&conn, &report_dir, "src/types/api.ts", rust_code, 0.5)
            .unwrap();

        // All 6 symbols exist → should pass
        assert_eq!(
            result.status,
            VerificationStatus::Verified,
            "Expected Verified but got {:?}. Checks: {:#?}",
            result.status,
            result.checks
        );
        assert!(
            result.similarity >= 0.5,
            "Similarity {:.2} should be >= 0.5",
            result.similarity
        );
        assert!(
            result.checks.is_empty() || result.checks.iter().all(|c| c.severity == "warning"),
            "No error-level checks expected, got: {:#?}",
            result.checks
        );

        // Verify specific success message
        let instruction = result.instruction.unwrap();
        assert!(instruction.contains("verified successfully"), "{}", instruction);
        assert!(instruction.contains("api.ts"), "{}", instruction);
    }

    #[test]
    fn test_verify_file_real_scale_missing_symbols() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_api_spec();
        write_spec(&report_dir, "src/types/api.ts", &spec);

        // Rust code missing 2 crucial symbols (api_error, health_check_response)
        let rust_code = r#"
pub struct ApiResponse<T> {
    pub success: bool,
}

pub struct PaginationMeta {
    pub page: u32,
    pub limit: u32,
    pub total: u32,
}

pub fn create_success_response<T>(data: T, meta: PaginationMeta) -> ApiResponse<T> {
    unimplemented!()
}

pub fn create_error_response(code: String, message: String) -> ApiResponse<()> {
    unimplemented!()
}
"#;

        let result = TaskQueue::verify_file(&conn, &report_dir, "src/types/api.ts", rust_code, 0.7)
            .unwrap();

        assert_eq!(result.status, VerificationStatus::NeedsRevision);

        // The error messages should specifically mention the missing symbols
        let has_api_error = result.checks.iter().any(|c| c.message.contains("api_error"));
        let has_health_check = result.checks.iter().any(|c| c.message.contains("health_check_response"));
        assert!(has_api_error, "Should report missing 'api_error'. Checks: {:#?}", result.checks);
        assert!(has_health_check, "Should report missing 'health_check_response'. Checks: {:#?}", result.checks);

        // Only 4/6 symbols matched → similarity < threshold
        assert!(
            result.similarity < 0.7,
            "Similarity {:.2} should be < 0.7 threshold",
            result.similarity
        );
    }

    #[test]
    fn test_verify_file_real_scale_invalid_rust() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_api_spec();
        write_spec(&report_dir, "src/types/api.ts", &spec);

        // Deliberately broken Rust code — completely unparseable
        let rust_code = "this is @@ not valid ~~~ rust {{{ pub fn !!!";

        let result = TaskQueue::verify_file(&conn, &report_dir, "src/types/api.ts", rust_code, 0.7)
            .unwrap();

        assert_eq!(result.status, VerificationStatus::NeedsRevision);
        assert_eq!(result.similarity, 0.0);
        assert!(
            result.checks.iter().any(|c| c.severity == "error"
                && (c.message.contains("parse error") || c.message.contains("cannot parse"))),
            "Should report parse error. Checks: {:#?}",
            result.checks
        );
    }

    // ── Real data integration test ────────────────────────────────────
    //
    // Uses the actual spec JSON from test-migration/report/spec/
    // so we verify against genuine analysis output, not synthetic fixtures.

    fn real_project_root() -> std::path::PathBuf {
        // Walk up from the workspace root
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        manifest_dir
            .parent()
            .expect("workspace root")
            .join("test")
    }

    fn real_report_dir() -> std::path::PathBuf {
        real_project_root().join("test-migration").join("report")
    }

    #[test]
    fn test_verify_file_with_real_spec_api_ts() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        // Copy the real spec file into the temp report dir
        let real_spec_path = real_report_dir().join("spec").join("src/types/api.ts.json");
        assert!(real_spec_path.exists(), "Real spec not found: {}", real_spec_path.display());

        let spec_content = std::fs::read_to_string(&real_spec_path).unwrap();
        let spec: MigrationSpec = serde_json::from_str(&spec_content).unwrap();
        write_spec(&report_dir, "src/types/api.ts", &spec);

        // Rust code that correctly implements all 7 exported symbols
        let rust_code = r#"
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<ApiError>,
    pub meta: Option<PaginationMeta>,
}

pub struct ApiError {
    pub code: String,
    pub message: String,
    pub details: Option<Vec<String>>,
}

pub struct PaginationMeta {
    pub page: u32,
    pub limit: u32,
    pub total: u32,
    pub total_pages: u32,
}

pub fn create_pagination_meta(page: u32, limit: u32, total: u32) -> PaginationMeta {
    PaginationMeta {
        page,
        limit,
        total,
        total_pages: (total + limit - 1) / limit,
    }
}

pub fn create_success_response<T>(data: T, meta: PaginationMeta) -> ApiResponse<T> {
    unimplemented!()
}

pub fn create_error_response(code: String, message: String, details: Option<Vec<String>>) -> ApiResponse<()> {
    unimplemented!()
}

pub struct HealthCheckResponse {
    pub status: String,
    pub uptime: f64,
    pub version: String,
    pub timestamp: String,
}
"#;

        let result = TaskQueue::verify_file(&conn, &report_dir, "src/types/api.ts", rust_code, 0.5)
            .unwrap();

        assert_eq!(
            result.status,
            VerificationStatus::Verified,
            "Real spec: all 7 symbols should match. Checks: {:#?}",
            result.checks
        );
        assert!(result.similarity >= 0.5, "Similarity {:.2} too low", result.similarity);
    }

    #[test]
    fn test_verify_file_with_real_spec_namespace_imports() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let real_spec_path = real_report_dir()
            .join("spec")
            .join("src/edge-cases/namespace-imports.ts.json");
        assert!(real_spec_path.exists(), "Real spec not found: {}", real_spec_path.display());

        let spec_content = std::fs::read_to_string(&real_spec_path).unwrap();
        let spec: MigrationSpec = serde_json::from_str(&spec_content).unwrap();
        write_spec(&report_dir, "src/edge-cases/namespace-imports.ts", &spec);

        // Check: 4 symbols → namespace_demo, build_health_response, get_status_value, clone_task
        assert_eq!(spec.symbols.len(), 4, "Expected 4 symbols in namespace-imports spec");

        // Rust code correctly implementing all 4 symbols
        let rust_code = r#"
pub struct NamespaceDemo {
    pub source: String,
}

pub fn build_health_response() -> ApiResponse<StatusPayload> {
    unimplemented!()
}

pub fn get_status_value(status: &str) -> u32 {
    unimplemented!()
}

pub fn clone_task(t: Task) -> Task {
    unimplemented!()
}
"#;

        let result = TaskQueue::verify_file(
            &conn,
            &report_dir,
            "src/edge-cases/namespace-imports.ts",
            rust_code,
            0.5,
        )
        .unwrap();

        assert_eq!(
            result.status,
            VerificationStatus::Verified,
            "Real spec (namespace-imports): all 4 symbols should match. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_with_real_spec_partial_match() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let real_spec_path = real_report_dir()
            .join("spec")
            .join("src/edge-cases/namespace-imports.ts.json");
        let spec_content = std::fs::read_to_string(&real_spec_path).unwrap();
        let spec: MigrationSpec = serde_json::from_str(&spec_content).unwrap();
        write_spec(&report_dir, "src/edge-cases/namespace-imports.ts", &spec);

        // Only 2 out of 4 symbols implemented → should fail
        let rust_code = r#"
pub struct NamespaceDemo {
    pub source: String,
}

pub fn build_health_response() -> ApiResponse<StatusPayload> {
    unimplemented!()
}
"#;

        let result = TaskQueue::verify_file(
            &conn,
            &report_dir,
            "src/edge-cases/namespace-imports.ts",
            rust_code,
            0.7,
        )
        .unwrap();

        assert_eq!(result.status, VerificationStatus::NeedsRevision);
        // Should report 2 missing symbols
        let missing_count = result.checks.iter().filter(|c| c.severity == "error").count();
        assert!(missing_count >= 1, "Expected at least 1 missing symbol error. Checks: {:#?}", result.checks);
    }

    #[test]
    fn test_verify_file_with_circular_dep_spec() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let real_spec_path = real_report_dir()
            .join("spec")
            .join("src/edge-cases/circular-a.ts.json");
        assert!(real_spec_path.exists(), "Real spec not found: {}", real_spec_path.display());

        let spec_content = std::fs::read_to_string(&real_spec_path).unwrap();
        let spec: MigrationSpec = serde_json::from_str(&spec_content).unwrap();
        write_spec(&report_dir, "src/edge-cases/circular-a.ts", &spec);

        // circular-a.ts has 3 symbols: AData, greetFromA, transformAData
        let rust_code = r#"
pub struct AData {
    pub id: String,
    pub label: String,
}

pub fn greet_from_a() -> String {
    unimplemented!()
}

pub fn transform_a_data(data: AData) -> String {
    unimplemented!()
}
"#;

        let result = TaskQueue::verify_file(
            &conn,
            &report_dir,
            "src/edge-cases/circular-a.ts",
            rust_code,
            0.5,
        )
        .unwrap();

        assert_eq!(
            result.status,
            VerificationStatus::Verified,
            "Circular dep spec: symbols should match. Checks: {:#?}",
            result.checks
        );
    }

    // ── verify_file basic: additional scenarios ──────────────────────

    #[test]
    fn test_verify_file_case_insensitive_matching() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/utils.ts".into(),
            target_path: "src/utils.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![
                crate::spec_writer::SpecSymbol {
                    name: "GET_USER".into(),
                    kind: "function".into(),
                    visibility: "Public".into(),
                    line_range: [1, 3],
                    signature: None,
                    target_name: "get_user".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
                crate::spec_writer::SpecSymbol {
                    name: "API_BASE".into(),
                    kind: "variable".into(),
                    visibility: "Public".into(),
                    line_range: [5, 5],
                    signature: None,
                    target_name: "api_base".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
            ],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/utils.ts", &spec);

        // Rust code uses different casing — should still match via normalization
        let rust_code = r#"
pub fn get_user() -> String {
    String::new()
}
pub const API_BASE: &str = "https://api.example.com";
"#;
        let result = TaskQueue::verify_file(&conn, &report_dir, "src/utils.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!((result.similarity - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_verify_file_underscore_insensitive_matching() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/types.ts".into(),
            target_path: "src/types.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![
                crate::spec_writer::SpecSymbol {
                    name: "UserProfile".into(),
                    kind: "interface".into(),
                    visibility: "Public".into(),
                    line_range: [1, 5],
                    signature: None,
                    target_name: "user_profile".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
                crate::spec_writer::SpecSymbol {
                    name: "UserSession".into(),
                    kind: "interface".into(),
                    visibility: "Public".into(),
                    line_range: [7, 10],
                    signature: None,
                    target_name: "usersession".into(), // no underscore
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
            ],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/types.ts", &spec);

        let rust_code = r#"
pub struct UserProfile {
    pub name: String,
}
pub struct UserSession {
    pub token: String,
}
"#;
        let result = TaskQueue::verify_file(&conn, &report_dir, "src/types.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!((result.similarity - 1.0).abs() < f64::EPSILON);
    }

    // ── verify_file_ast deep tests ───────────────────────────────────

    fn make_fn_spec(name: &str, target: &str) -> MigrationSpec {
        MigrationSpec {
            file: "src/test.rs".into(),
            target_path: "src/test.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: name.into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 5],
                signature: Some(format!("fn {}(x: i32) -> i32", target)),
                target_name: target.into(),
                target_signature: Some(format!("(x: i32) -> i32")),
                params: vec![crate::spec_writer::SpecParam {
                    name: "x".into(),
                    ty: "i32".into(),
                    optional: false,
                }],
                return_type: Some("i32".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        }
    }

    #[test]
    fn test_verify_file_ast_exact_function() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_fn_spec("add", "add");
        write_spec(&report_dir, "src/test.rs", &spec);

        let rust_code = r#"
pub fn add(x: i32) -> i32 {
    x
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified, "Checks: {:#?}", result.checks);
        assert!(result.similarity >= 0.7);
        // All details should match → detail_score == matched_count
        assert!(result.checks.is_empty(), "Expected no warnings for exact match, got: {:#?}", result.checks);
    }

    #[test]
    fn test_verify_file_ast_param_name_mismatch() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_fn_spec("add", "add");
        write_spec(&report_dir, "src/test.rs", &spec);

        // Param name 'val' instead of 'x'
        let rust_code = r#"
pub fn add(val: i32) -> i32 {
    val
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.5)
            .unwrap();
        // Should still pass name check (symbol exists), but structural warning
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(
            result.checks.iter().any(|c| c.message.contains("param[0] name")),
            "Expected warning about param name. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_param_type_mismatch() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_fn_spec("add", "add");
        write_spec(&report_dir, "src/test.rs", &spec);

        // Param type 'f64' instead of 'i32'
        let rust_code = r#"
pub fn add(x: f64) -> f64 {
    x
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(
            result.checks.iter().any(|c| c.message.contains("param[0]") && c.message.contains("type")),
            "Expected warning about param type. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_param_count_mismatch() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_fn_spec("add", "add");
        write_spec(&report_dir, "src/test.rs", &spec);

        // Two params instead of one
        let rust_code = r#"
pub fn add(x: i32, y: i32) -> i32 {
    x + y
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(
            result.checks.iter().any(|c| c.message.contains("parameter count")),
            "Expected warning about param count. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_return_type_mismatch() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_fn_spec("add", "add");
        write_spec(&report_dir, "src/test.rs", &spec);

        // Return type 'f64' instead of 'i32'
        let rust_code = r#"
pub fn add(x: i32) -> f64 {
    x as f64
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(
            result.checks.iter().any(|c| c.message.contains("return type")),
            "Expected warning about return type. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_async_mismatch() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        // Spec says is_async = true
        let spec = MigrationSpec {
            file: "src/test.rs".into(),
            target_path: "src/test.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "fetchData".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("async fn fetch_data() -> String".into()),
                target_name: "fetch_data".into(),
                target_signature: Some("() -> String".into()),
                params: vec![],
                return_type: Some("String".into()),
                is_async: Some(true),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/test.rs", &spec);

        // Rust code is NOT async — mismatch
        let rust_code = r#"
pub fn fetch_data() -> String {
    String::new()
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(
            result.checks.iter().any(|c| c.message.contains("async")),
            "Expected warning about async mismatch. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_acceptable_kind_interface_to_struct() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/types.ts".into(),
            target_path: "src/types.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "UserData".into(),
                kind: "interface".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: None,
                target_name: "user_data".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/types.ts", &spec);

        // Rust struct — acceptable translation of TS interface
        let rust_code = r#"
pub struct UserData {
    pub id: u32,
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/types.ts", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        // Should NOT have kind mismatch warning
        assert!(
            !result.checks.iter().any(|c| c.message.contains("kind mismatch")),
            "Should NOT flag interface→struct as mismatch. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_acceptable_kind_variable_to_const() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/consts.ts".into(),
            target_path: "src/consts.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "MAX_SIZE".into(),
                kind: "variable".into(),
                visibility: "Public".into(),
                line_range: [1, 1],
                signature: None,
                target_name: "max_size".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/consts.ts", &spec);

        let rust_code = r#"
pub const MAX_SIZE: usize = 1024;
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/consts.ts", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(
            !result.checks.iter().any(|c| c.message.contains("kind mismatch")),
            "Should NOT flag variable→const as mismatch. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_acceptable_kind_type_alias_to_enum() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/status.ts".into(),
            target_path: "src/status.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "OrderStatus".into(),
                kind: "type_alias".into(),
                visibility: "Public".into(),
                line_range: [1, 1],
                signature: None,
                target_name: "order_status".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/status.ts", &spec);

        let rust_code = r#"
pub enum OrderStatus {
    Pending,
    Shipped,
    Delivered,
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/status.ts", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(
            !result.checks.iter().any(|c| c.message.contains("kind mismatch")),
            "Should NOT flag type_alias→enum as mismatch. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_unacceptable_kind() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/test.ts".into(),
            target_path: "src/test.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "counter".into(),
                kind: "function".into(), // spec expects a function
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn counter() -> i32".into()),
                target_name: "counter".into(),
                target_signature: Some("() -> i32".into()),
                params: vec![],
                return_type: Some("i32".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/test.ts", &spec);

        // Rust code has a STRUCT named counter — not an acceptable mapping for function
        let rust_code = r#"
pub struct Counter {
    pub value: i32,
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!(
            result.checks.iter().any(|c| c.message.contains("kind mismatch")),
            "Should flag function→struct as unacceptable. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_struct_fields() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/models.ts".into(),
            target_path: "src/models.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "Product".into(),
                kind: "interface".into(),
                visibility: "Public".into(),
                line_range: [1, 5],
                signature: Some("interface Product { id: number; name: string; price: number }".into()),
                target_name: "product".into(),
                target_signature: Some("id, name, price".into()), // field hints
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/models.ts", &spec);

        let rust_code = r#"
pub struct Product {
    pub id: u32,
    pub name: String,
    pub price: f64,
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/models.ts", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        // interface→struct is acceptable, so no kind mismatch
        assert!(
            !result.checks.iter().any(|c| c.message.contains("kind mismatch")),
            "Should not flag interface→struct. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_symbol_missing_error() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_fn_spec("doSomething", "do_something");
        write_spec(&report_dir, "src/test.rs", &spec);

        // No matching symbol at all
        let rust_code = r#"
pub fn unrelated() {}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::NeedsRevision);
        assert!(
            result.checks.iter().any(|c| c.severity == "error" && c.message.contains("do_something")),
            "Expected error about missing symbol. Checks: {:#?}",
            result.checks
        );
    }

    #[test]
    fn test_verify_file_ast_below_threshold() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        // Spec with 2 symbols
        let spec = MigrationSpec {
            file: "src/test.rs".into(),
            target_path: "src/test.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![
                crate::spec_writer::SpecSymbol {
                    name: "fnA".into(),
                    kind: "function".into(),
                    visibility: "Public".into(),
                    line_range: [1, 3],
                    signature: None,
                    target_name: "fn_a".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
                crate::spec_writer::SpecSymbol {
                    name: "fnB".into(),
                    kind: "function".into(),
                    visibility: "Public".into(),
                    line_range: [5, 7],
                    signature: None,
                    target_name: "fn_b".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
            ],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/test.rs", &spec);

        // Only 1 of 2 symbols present
        let rust_code = r#"
pub fn fn_a() {}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.8)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::NeedsRevision);
        assert!(
            result.similarity < 0.8,
            "Similarity {:.2} should be below 0.8 threshold",
            result.similarity
        );
    }

    #[test]
    fn test_verify_file_ast_invalid_rust() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = make_fn_spec("test", "test");
        write_spec(&report_dir, "src/test.rs", &spec);

        let rust_code = "this is @@ not valid ~~~ rust";
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.rs", rust_code, 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::NeedsRevision);
        assert_eq!(result.similarity, 0.0);
    }

    #[test]
    fn test_verify_file_ast_empty_symbols() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/empty.ts".into(),
            target_path: "src/empty.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/empty.ts", &spec);

        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/empty.ts", "pub fn foo() {}", 0.7)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        assert!((result.similarity - 1.0).abs() < f64::EPSILON);
    }

    // ── verify_batch tests ───────────────────────────────────────────

    fn make_multi_spec_src() -> Vec<(&'static str, MigrationSpec)> {
        vec![
            (
                "src/a.ts",
                MigrationSpec {
                    file: "src/a.ts".into(),
                    target_path: "src/a.rs".into(),
                    layer: 0,
                    migration_effort: "trivial".into(),
                    has_tests: false,
                    source: String::new(),
                    exports: vec![],
                    symbols: vec![crate::spec_writer::SpecSymbol {
                        name: "funcA".into(),
                        kind: "function".into(),
                        visibility: "Public".into(),
                        line_range: [1, 3],
                        signature: None,
                        target_name: "func_a".into(),
                        target_signature: None,
                        params: vec![],
                        return_type: None,
                        is_async: None,
                        migration_note: None,
                    }],
                    imports: crate::spec_writer::SpecImports {
                        relative: vec![],
                        external: vec![],
                    },
                    referenced_by: vec![],
                },
            ),
            (
                "src/b.ts",
                MigrationSpec {
                    file: "src/b.ts".into(),
                    target_path: "src/b.rs".into(),
                    layer: 0,
                    migration_effort: "trivial".into(),
                    has_tests: false,
                    source: String::new(),
                    exports: vec![],
                    symbols: vec![crate::spec_writer::SpecSymbol {
                        name: "funcB".into(),
                        kind: "function".into(),
                        visibility: "Public".into(),
                        line_range: [1, 3],
                        signature: None,
                        target_name: "func_b".into(),
                        target_signature: None,
                        params: vec![],
                        return_type: None,
                        is_async: None,
                        migration_note: None,
                    }],
                    imports: crate::spec_writer::SpecImports {
                        relative: vec![],
                        external: vec![],
                    },
                    referenced_by: vec![],
                },
            ),
        ]
    }

    #[test]
    fn test_verify_batch_single_file_deep() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let specs = make_multi_spec_src();
        for (file, spec) in &specs {
            write_spec(&report_dir, file, spec);
        }

        // Deep verify a single file with provided content
        let result = TaskQueue::verify_batch(
            &conn,
            &report_dir,
            Some("src/a.ts"),
            Some("pub fn func_a() {}"),
            None,
            false,
            None,
            0.5,
            true, // deep
        )
        .unwrap();

        assert_eq!(result.total, 1);
        assert_eq!(result.verified, 1);
    }

    #[test]
    fn test_verify_batch_all_deep() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");
        let project_dir = _dir.path().join("project");

        let specs = make_multi_spec_src();
        for (file, spec) in &specs {
            write_spec(&report_dir, file, spec);
        }

        // Create Rust source files in project dir
        std::fs::create_dir_all(project_dir.join("src")).unwrap();
        std::fs::write(project_dir.join("src/a.rs"), "pub fn func_a() {}").unwrap();
        std::fs::write(project_dir.join("src/b.rs"), "pub fn func_b() {}").unwrap();

        let result = TaskQueue::verify_batch(
            &conn,
            &report_dir,
            None,     // no single file
            None,     // no content
            None,     // no directory filter
            true,     // all
            Some(&project_dir),
            0.5,
            true,     // deep
        )
        .unwrap();

        assert_eq!(result.total, 2);
        assert_eq!(result.verified, 2);
        assert_eq!(result.failed, 0);
    }

    #[test]
    fn test_verify_batch_all_shallow_vs_deep_score_diff() {
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        // Spec with detailed function signature expectations
        let spec = MigrationSpec {
            file: "src/test.ts".into(),
            target_path: "src/test.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "compute".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 5],
                signature: Some("fn compute(x: i32, y: i32) -> i32".into()),
                target_name: "compute".into(),
                target_signature: Some("(x: i32, y: i32) -> i32".into()),
                params: vec![
                    crate::spec_writer::SpecParam {
                        name: "x".into(),
                        ty: "i32".into(),
                        optional: false,
                    },
                    crate::spec_writer::SpecParam {
                        name: "y".into(),
                        ty: "i32".into(),
                        optional: false,
                    },
                ],
                return_type: Some("i32".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/test.ts", &spec);

        // Wrong signature: different param types, different count, wrong return
        let rust_code = "pub fn compute(a: f64, b: f64, c: f64) -> f64 { a + b + c }";

        // Shallow: passes (symbol exists)
        let shallow = TaskQueue::verify_file(&conn, &report_dir, "src/test.ts", rust_code, 0.5).unwrap();
        assert_eq!(shallow.status, VerificationStatus::Verified);
        assert!((shallow.similarity - 1.0).abs() < f64::EPSILON, "Shallow should give 1.0");

        // Deep: should catch structural issues
        let deep = TaskQueue::verify_file_ast(&conn, &report_dir, "src/test.ts", rust_code, 0.5).unwrap();
        assert_eq!(deep.status, VerificationStatus::Verified);
        assert!(
            deep.similarity < 0.9,
            "Deep should score lower due to structural mismatches, got {:.2}",
            deep.similarity
        );
        assert!(
            !deep.checks.is_empty(),
            "Deep should produce structural warnings"
        );
    }

    // ── Tricky/edge-case tests ───────────────────────────────────────

    #[test]
    fn test_verify_file_ast_generic_function() {
        // Spec expects a generic function
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/generic.ts".into(),
            target_path: "src/generic.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "identity".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn identity<T>(x: T) -> T".into()),
                target_name: "identity".into(),
                target_signature: Some("<T>(x: T) -> T".into()),
                params: vec![crate::spec_writer::SpecParam {
                    name: "x".into(),
                    ty: "T".into(),
                    optional: false,
                }],
                return_type: Some("T".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/generic.ts", &spec);

        // Rust generic implementation
        let rust_code = r#"
pub fn identity<T>(x: T) -> T {
    x
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/generic.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        // Generic param type 'T' in spec matches generic 'T' in Rust
    }

    #[test]
    fn test_verify_file_ast_lifetime_parameter() {
        // Spec expects a function with lifetime
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/lifetime.ts".into(),
            target_path: "src/lifetime.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "first".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn first<'a, T>(slice: &'a [T]) -> Option<&'a T>".into()),
                target_name: "first".into(),
                target_signature: Some("<'a, T>(slice: &'a [T]) -> Option<&'a T>".into()),
                params: vec![crate::spec_writer::SpecParam {
                    name: "slice".into(),
                    ty: "&[T]".into(),
                    optional: false,
                }],
                return_type: Some("Option<&T>".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/lifetime.ts", &spec);

        // Rust with lifetime
        let rust_code = r#"
pub fn first<'a, T>(slice: &'a [T]) -> Option<&'a T> {
    slice.first()
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/lifetime.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_nested_type() {
        // Spec expects nested type like Option<Vec<String>>
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/nested.ts".into(),
            target_path: "src/nested.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "get_names".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn get_names() -> Option<Vec<String>>".into()),
                target_name: "get_names".into(),
                target_signature: Some("() -> Option<Vec<String>>".into()),
                params: vec![],
                return_type: Some("Option<Vec<String>>".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/nested.ts", &spec);

        let rust_code = r#"
pub fn get_names() -> Option<Vec<String>> {
    Some(vec!["a".to_string()])
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/nested.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_result_return() {
        // Spec expects Result<T, E> return
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/fallible.ts".into(),
            target_path: "src/fallible.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "parse_int".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn parse_int(s: &str) -> Result<i32, ParseIntError>".into()),
                target_name: "parse_int".into(),
                target_signature: Some("(s: &str) -> Result<i32, ParseIntError>".into()),
                params: vec![crate::spec_writer::SpecParam {
                    name: "s".into(),
                    ty: "&str".into(),
                    optional: false,
                }],
                return_type: Some("Result<i32, ParseIntError>".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/fallible.ts", &spec);

        let rust_code = r#"
pub fn parse_int(s: &str) -> Result<i32, std::num::ParseIntError> {
    s.parse()
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/fallible.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        // 'ParseIntError' vs 'std::num::ParseIntError' — type string comparison
    }

    #[test]
    fn test_verify_file_ast_trait_bound() {
        // Spec expects function with trait bound
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/bound.ts".into(),
            target_path: "src/bound.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "duplicate".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: Some("fn duplicate<T: Clone>(x: T) -> (T, T)".into()),
                target_name: "duplicate".into(),
                target_signature: Some("<T: Clone>(x: T) -> (T, T)".into()),
                params: vec![crate::spec_writer::SpecParam {
                    name: "x".into(),
                    ty: "T".into(),
                    optional: false,
                }],
                return_type: Some("(T, T)".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/bound.ts", &spec);

        let rust_code = r#"
pub fn duplicate<T: Clone>(x: T) -> (T, T) {
    (x.clone(), x)
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/bound.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_impl_method() {
        // Spec expects a method inside impl block
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/service.ts".into(),
            target_path: "src/service.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "UserService".into(),
                kind: "class".into(),
                visibility: "Public".into(),
                line_range: [1, 10],
                signature: None,
                target_name: "user_service".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/service.ts", &spec);

        // Impl block — we match on struct name 'UserService'
        let rust_code = r#"
pub struct UserService {
    db: String,
}

impl UserService {
    pub fn new(db: String) -> Self {
        UserService { db }
    }
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/service.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_same_name_different_kind() {
        // Same name appears as both struct and function — should match one
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/conflict.ts".into(),
            target_path: "src/conflict.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "Data".into(),
                kind: "interface".into(), // looking for interface -> struct
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: None,
                target_name: "data".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/conflict.ts", &spec);

        // Both struct and fn named 'Data' — should find struct first
        let rust_code = r#"
pub struct Data {
    pub id: u32,
}

pub fn data() -> Data {
    Data { id: 0 }
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/conflict.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_private_symbol_ignored() {
        // Spec only includes public symbols, private ones are ignored
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/priv.ts".into(),
            target_path: "src/priv.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            // Only one public symbol expected
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "PublicApi".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: None,
                target_name: "public_api".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/priv.ts", &spec);

        // Rust has private helper — not in spec, should be ignored
        let rust_code = r#"
fn internal_helper() {}

pub fn public_api() {
    internal_helper();
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/priv.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
        // No error about 'internal_helper' — only public symbols are checked
    }

    #[test]
    fn test_verify_file_ast_tuple_struct() {
        // Tuple struct vs regular struct
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/tuple.ts".into(),
            target_path: "src/tuple.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "Point".into(),
                kind: "interface".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: None,
                target_name: "point".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/tuple.ts", &spec);

        // Tuple struct — still matches 'struct' kind
        let rust_code = r#"
pub struct Point(pub f64, pub f64);
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/tuple.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_unit_struct() {
        // Unit struct (marker type)
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/marker.ts".into(),
            target_path: "src/marker.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "Empty".into(),
                kind: "interface".into(),
                visibility: "Public".into(),
                line_range: [1, 1],
                signature: None,
                target_name: "empty".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/marker.ts", &spec);

        let rust_code = r#"
pub struct Empty;
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/marker.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_enum_with_variants() {
        // Enum with multiple variants
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/status.ts".into(),
            target_path: "src/status.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "Status".into(),
                kind: "enum".into(),
                visibility: "Public".into(),
                line_range: [1, 5],
                signature: None,
                target_name: "status".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/status.ts", &spec);

        let rust_code = r#"
pub enum Status {
    Pending,
    Active,
    Completed,
    Failed(String),
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/status.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_type_alias() {
        // Type alias
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/alias.ts".into(),
            target_path: "src/alias.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "UserId".into(),
                kind: "type_alias".into(),
                visibility: "Public".into(),
                line_range: [1, 1],
                signature: None,
                target_name: "user_id".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/alias.ts", &spec);

        let rust_code = r#"
pub type UserId = String;
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/alias.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_macro_ignored() {
        // Macros are ignored in verification
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/macro.ts".into(),
            target_path: "src/macro.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "helper".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: None,
                target_name: "helper".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/macro.ts", &spec);

        // Has macro call and macro definition — should be ignored
        let rust_code = r#"
#[macro_export]
macro_rules! my_macro {
    () => {};
}

pub fn helper() {
    my_macro!();
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/macro.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_self_type() {
        // Self type in method
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/builder.ts".into(),
            target_path: "src/builder.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "Builder".into(),
                kind: "interface".into(),
                visibility: "Public".into(),
                line_range: [1, 10],
                signature: None,
                target_name: "builder".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/builder.ts", &spec);

        let rust_code = r#"
pub struct Builder {
    value: i32,
}

impl Builder {
    pub fn new() -> Self {
        Self { value: 0 }
    }

    pub fn with_value(mut self, v: i32) -> Self {
        self.value = v;
        self
    }
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/builder.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_where_clause() {
        // Function with where clause
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/where.ts".into(),
            target_path: "src/where.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "process".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 5],
                signature: Some("fn process<T>(x: T) -> String where T: ToString".into()),
                target_name: "process".into(),
                target_signature: Some("<T>(x: T) -> String where T: ToString".into()),
                params: vec![crate::spec_writer::SpecParam {
                    name: "x".into(),
                    ty: "T".into(),
                    optional: false,
                }],
                return_type: Some("String".into()),
                is_async: Some(false),
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/where.ts", &spec);

        let rust_code = r#"
pub fn process<T>(x: T) -> String
where
    T: ToString,
{
    x.to_string()
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/where.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_associated_const() {
        // Associated constant in impl
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/consts.ts".into(),
            target_path: "src/consts.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![
                crate::spec_writer::SpecSymbol {
                    name: "Constants".into(),
                    kind: "class".into(),
                    visibility: "Public".into(),
                    line_range: [1, 10],
                    signature: None,
                    target_name: "constants".into(),
                    target_signature: None,
                    params: vec![],
                    return_type: None,
                    is_async: None,
                    migration_note: None,
                },
            ],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/consts.ts", &spec);

        let rust_code = r#"
pub struct Constants;

impl Constants {
    pub const MAX: u32 = 100;
    pub const MIN: u32 = 0;
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/consts.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_closure_not_top_level() {
        // Closures inside functions shouldn't be counted as top-level symbols
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/closure.ts".into(),
            target_path: "src/closure.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "compute".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 5],
                signature: None,
                target_name: "compute".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/closure.ts", &spec);

        // Closure inside function — shouldn't cause issues
        let rust_code = r#"
pub fn compute(x: i32) -> i32 {
    let add = |a, b| a + b;
    add(x, 1)
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/closure.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }

    #[test]
    fn test_verify_file_ast_nested_module() {
        // Nested module structure
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/nested.ts".into(),
            target_path: "src/nested.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "outer_inner".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 3],
                signature: None,
                target_name: "outer_inner".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/nested.ts", &spec);

        // Nested module with pub use
        let rust_code = r#"
pub mod outer {
    pub mod inner {
        pub fn outer_inner() {}
    }
    pub use inner::outer_inner;
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/nested.ts", rust_code, 0.5)
            .unwrap();
        // Top-level symbols only — won't find nested 'outer_inner' directly
        // This tests that we don't crash on nested modules
        assert!(
            result.status == VerificationStatus::Verified
                || result.status == VerificationStatus::NeedsRevision
        );
    }

    #[test]
    fn test_verify_file_ast_attribute_on_function() {
        // Function with attributes (#[inline], #[test], etc.)
        let (_dir, conn) = setup_db_with_modules();
        let report_dir = _dir.path().join("report");

        let spec = MigrationSpec {
            file: "src/attr.ts".into(),
            target_path: "src/attr.rs".into(),
            layer: 0,
            migration_effort: "trivial".into(),
            has_tests: false,
            source: String::new(),
            exports: vec![],
            symbols: vec![crate::spec_writer::SpecSymbol {
                name: "fast_add".into(),
                kind: "function".into(),
                visibility: "Public".into(),
                line_range: [1, 5],
                signature: None,
                target_name: "fast_add".into(),
                target_signature: None,
                params: vec![],
                return_type: None,
                is_async: None,
                migration_note: None,
            }],
            imports: crate::spec_writer::SpecImports {
                relative: vec![],
                external: vec![],
            },
            referenced_by: vec![],
        };
        write_spec(&report_dir, "src/attr.ts", &spec);

        let rust_code = r#"
#[inline]
#[must_use]
pub fn fast_add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let result = TaskQueue::verify_file_ast(&conn, &report_dir, "src/attr.ts", rust_code, 0.5)
            .unwrap();
        assert_eq!(result.status, VerificationStatus::Verified);
    }
}
