# Plan 001: Replace symbol registry with real-time target-project alignment

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 92c212b..HEAD -- core/src/ migration-analyze/src/`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: L
- **Risk**: MED
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `92c212b`, 2026-07-20

## Why this matters

Currently the tool uses an SQLite-based symbol registry that is never auto-populated,
requiring manual `registry add` commands for every symbol. The patch module depends on
this registry but is unusable in practice. Instead of maintaining a stale database,
we should parse the target project directly during `diff` and perform real-time
fuzzy matching to determine where each source change maps in the target codebase.
This eliminates the registry database, the patch module, and ~800 lines of dead code,
replacing them with a ~400-line alignment engine that produces useful target references
in every diff report with zero user setup.

## Current state

### Source tree structure

```
core/src/
  registry/            ← to DELETE (~350 lines)
    mod.rs             — SymbolMapping struct + SymbolRegistry trait
    sqlite.rs          — SqliteRegistry impl (rusqlite-based)
    schema.sql         — SQLite DDL
    enrich.rs          — recently added enrichment helpers
  patch/               ← to DELETE (~500 lines)
    mod.rs             — Patch struct
    generator.rs       — PatchGenerator (reads registry to produce patches)
    validator.rs       — ContextValidator
    format.rs          — Patch formatting
    types.rs           — PatchAction enum
  diff/
    engine.rs          — DiffEngine
    logic.rs           — diff_value()
    signature.rs       — diff_children(), param/return-type diff
    mapping.rs         — name-based symbol matching (KEEP)
    mod.rs             — DiffReport, SymbolChange, FileDiffResult
  align/               ← to CREATE
    (empty for now)
  lib.rs               — exports both `registry` and `patch` (line 17, 14)
  Cargo.toml           — depends on `rusqlite` (line 28)

migration-analyze/src/
  commands/
    mod.rs             — exports registry + patch (lines 7-8)
    registry.rs        — CLI subcommand (~150 lines)
    patch.rs           — CLI subcommand (~80 lines)
    diff.rs            — currently imports registry::enrich
  main.rs              — registers Registry + Patch subcommands (lines 32-39)
```

### Key data structures

```rust
// core/src/diff/mod.rs — SymbolChange, the struct every change flows through
pub struct SymbolChange {
    pub symbol: String,
    pub kind: String,
    pub change_type: String,
    pub severity: String,
    pub old_name: Option<String>,
    pub rename_confidence: Option<f64>,
    pub details: Vec<ChangeDetail>,
    pub old_line_range: Option<[usize; 2]>,
    pub new_line_range: Option<[usize; 2]>,
    pub old_source: Option<String>,
    pub new_source: Option<String>,
    pub target_file: Option<String>,      // already added
    pub target_symbol: Option<String>,    // already added
}
```

```rust
// core/src/diff/types.rs — CLI output format
pub(crate) struct SymbolChangeDetail {
    // ... same fields as SymbolChange ...
    pub(crate) target_file: Option<String>,    // already added
    pub(crate) target_symbol: Option<String>,  // already added
}
```

### How `diff` currently uses registry enrichment

`migration-analyze/src/commands/diff.rs:115-128`:
```rust
// After run_ast_diff(), currently tries to load registry.db and enrich
let registry_path = ctx.migration_folder.join("registry.db");
if registry_path.exists() {
    if let Ok(reg) = SqliteRegistry::new(&registry_path) {
        enrich::enrich_target_info(&mut diff_result.file_changes, Some(&reg));
    }
} else {
    enrich::enrich_target_info(&mut diff_result.file_changes, None::<...>);
}
```

### Compatibility data pattern (the model for `naming_data/`)

```toml
# core/compatibility_data/ts_libraries/database.toml
[library.prisma]
type = "orm"
description = "Next-generation ORM..."
tags = ["database", "orm", ...]
```

```rust
// core/src/compatibility/types.rs:190-212
impl MatrixRegistry {
    pub(crate) fn load(language: &str) -> Self {
        let data = match language {
            "typescript" => include_str!(concat!(env!("OUT_DIR"), "/ts_libraries.toml")),
            "rust" => include_str!(concat!(env!("OUT_DIR"), "/rust_libraries.toml")),
            _ => { return Self { libraries: HashMap::new() }; }
        };
        // parse toml::Table, extract [library.*] sections...
    }
}
```

```rust
// core/build.rs — merges *.toml files in compatibility_data/*/ into single files
fn merge_toml_dir(dir: &Path) -> String {
    // collects all .toml files, concatenates with # Source: comments
}
```

### `SymbolExtractor` — already parses target-language projects

`core/src/symbols/mod.rs:117-167` — `SymbolExtractor::extract_all()` takes root path
and file list, returns `Vec<(SymbolIndex, ApiContract)>` with per-file symbol info
(name, kind, line_range, params, return_type, children).

### Diff flow entry point

`migration-analyze/src/commands/diff.rs:115-137` — `run()` calls `logic::run_ast_diff()`,
then converts to output format, runs propagation, and writes JSON report. This is where
the alignment step will be inserted.

### Config already has the target field

```rust
// core/src/config.rs:35-36
pub struct ProjectConfig {
    pub target: Option<String>,    // already exists, never used ← will become required
    // ...
}
```

## Commands you will need

| Purpose   | Command (from repo root)   | Expected on success |
|-----------|----------------------------|---------------------|
| Build     | `cargo build`              | exit 0              |
| Test      | `cargo test`               | exit 0, all pass    |
| Unit only | `cargo test -p migration-core` | exit 0, ~240 pass |

## Scope

**In scope** (the only files you should modify):
- `core/src/registry/` — entire directory (DELETE)
- `core/src/patch/` — entire directory (DELETE)
- `core/src/align/` — entire directory (CREATE)
- `core/naming_data/` — entire directory (CREATE)
- `core/build.rs` — add naming_data merging
- `core/src/lib.rs` — remove registry/patch, add align
- `core/Cargo.toml` — remove rusqlite dependency
- `core/src/diff/mod.rs` — keep SymbolChange as-is (target fields already exist)
- `migration-analyze/src/commands/mod.rs` — remove registry/patch exports
- `migration-analyze/src/commands/registry.rs` — DELETE
- `migration-analyze/src/commands/patch.rs` — DELETE
- `migration-analyze/src/commands/diff.rs` — replace registry enrich with align::resolve_all
- `migration-analyze/src/main.rs` — remove Registry/Patch subcommands
- `plans/README.md` — update status

**Out of scope** (do NOT touch):
- `core/src/diff/engine.rs` — diff engine logic unchanged
- `core/src/diff/signature.rs` — signature diff logic unchanged
- `core/src/diff/mapping.rs` — name-based symbol matching, keep as-is
- `core/src/diff/logic.rs` — value diff logic unchanged
- `core/src/language/` — language parsers unchanged
- `core/src/symbols/` — symbol extractor already works for target project
- `core/src/config.rs` — `project.target` already exists, just reference it

## Git workflow

- Branch: `advisor/001-real-time-alignment`
- Commit per step below (7 commits). Message style: `refactor(align): <description>`
- Do NOT push or open a PR.

## Steps

### Step 1: Remove `registry/` module

Delete these files entirely:
- `core/src/registry/mod.rs`
- `core/src/registry/sqlite.rs`
- `core/src/registry/schema.sql`
- `core/src/registry/enrich.rs`

In `core/src/lib.rs`, remove line `pub mod registry;`.

**Verify**: `cargo build` → errors about missing `registry` in `patch/generator.rs` and `diff.rs` (expected — we'll fix those in later steps). Confirm the files are gone.

### Step 2: Remove `patch/` module

Delete these files entirely:
- `core/src/patch/mod.rs`
- `core/src/patch/generator.rs`
- `core/src/patch/validator.rs`
- `core/src/patch/format.rs`
- `core/src/patch/types.rs`
- `core/src/patch/` directory (all contents)

In `core/src/lib.rs`, remove line `pub mod patch;`.

**Verify**: `cargo build` → errors about missing `registry`/`patch` imports in migration-analyze (expected). The core crate itself should now compile with no errors except in migration-analyze.

### Step 3: Remove `rusqlite` dependency

In `core/Cargo.toml`, delete line 28: `rusqlite.workspace = true`.

In workspace `Cargo.toml`, delete line 37: `rusqlite = { version = "0.32", features = ["bundled"] }`.

**Verify**: `cargo build -p migration-core` → no errors, no warnings.

### Step 4: Create `naming_data/` directory with TOML convention files

Create directory structure:
```
core/naming_data/
  ts_to_rust/
    conventions.toml
    type_map.toml
    api_map.toml
  rust_to_ts/
    conventions.toml
    type_map.toml
    api_map.toml
```

#### `core/naming_data/ts_to_rust/conventions.toml`

```toml
# Naming conventions: TypeScript → Rust
# Follows the same pattern as compatibility_data/ (build.rs merges these)

[convention]
source_lang = "typescript"
target_lang = "rust"

# Prefixes to strip from type/interface names
strip_prefixes = ["I", "IBase", "T"]

# Case transformation for field names and function names
case = "CamelToSnake"
```

#### `core/naming_data/ts_to_rust/type_map.toml`

```toml
[type_mapping]
# Generic parameter placeholder: {T} in "from" captures the inner type
"string" = "String"
"number" = "i32"
"bigint" = "i64"
"boolean" = "bool"
"any" = "Any"
"void" = "()"
"null" = "None"
"undefined" = "()"
"never" = "!"
"unknown" = "Box<dyn Any>"
"object" = "serde_json::Value"
"Error" = "Box<dyn std::error::Error>"
"Promise<{T}>" = "Result<{T}>"
"Array<{T}>" = "Vec<{T}>"
"Record<string, {T}>" = "HashMap<String, {T}>"
"Map<{K}, {V}>" = "HashMap<{K}, {V}>"
"Set<{T}>" = "HashSet<{T}>"
"Date" = "chrono::DateTime<Utc>"
"Buffer" = "Vec<u8>"
"RegExp" = "regex::Regex"
```

#### `core/naming_data/ts_to_rust/api_map.toml`

```toml
[api_mapping]
# API call chain translations
# Format: "source_call_pattern" = "target_call_pattern"

# Database
"db.findOne" = "db.find_one"
"db.findMany" = "db.find_many"
"db.create" = "db.create"
"db.update" = "db.update"
"db.delete" = "db.delete"
"db.save" = "db.insert"
"db.aggregate" = "db.aggregate"

# Promise/async utilities
"Promise.all" = "futures::future::join_all"
"Promise.race" = "tokio::select"
"setTimeout" = "tokio::time::sleep"
"setInterval" = "tokio::time::interval"

# HTTP
"fetch" = "reqwest::get"
"axios.get" = "reqwest::Client::get"
"axios.post" = "reqwest::Client::post"

# Error handling
"try/catch" = "match/Result"
"throw" = "return Err"
"new Error" = "anyhow::anyhow"

# Common utilities
"console.log" = "println"
"console.error" = "eprintln"
"JSON.stringify" = "serde_json::to_string"
"JSON.parse" = "serde_json::from_str"
"Object.keys" = "map.keys"
"Object.values" = "map.values"
"Array.map" = ".iter().map"
"Array.filter" = ".iter().filter"
"Array.reduce" = ".iter().fold"
"Array.find" = ".iter().find"
"Array.includes" = ".contains"
"String.split" = ".split"
"String.trim" = ".trim"
"String.toLowerCase" = ".to_lowercase"
"String.toUpperCase" = ".to_uppercase"
```

#### `core/naming_data/rust_to_ts/conventions.toml`

```toml
[convention]
source_lang = "rust"
target_lang = "typescript"

# Rust types get no prefix stripping by default (no I-prefix convention in Rust)
strip_prefixes = []

# Case transformation for reverse direction
case = "SnakeToCamel"
```

#### `core/naming_data/rust_to_ts/type_map.toml`

```toml
[type_mapping]
"String" = "string"
"i32" = "number"
"i64" = "bigint"
"bool" = "boolean"
"()" = "void"
"Option<{T}>" = "{T} | null"
"Result<{T}>" = "Promise<{T}>"
"Vec<{T}>" = "Array<{T}>"
"HashMap<String, {T}>" = "Record<string, {T}>"
"HashSet<{T}>" = "Set<{T}>"
```

#### `core/naming_data/rust_to_ts/api_map.toml`

```toml
[api_mapping]
"db.find_one" = "db.findOne"
"tokio::time::sleep" = "setTimeout"
"println" = "console.log"
```

**Verify**: Directory tree exists with all 8 files populated. Run `Get-ChildItem -Recurse core/naming_data/` to confirm.

### Step 5: Update `core/build.rs` to merge naming_data

Add after the existing compatibility_data merge logic:

```rust
// In core/build.rs, after the compatibility_data loop (line 19):
for naming_dir in &["ts_to_rust", "rust_to_ts"] {
    let dir = data_dir.parent().unwrap().join("naming_data").join(naming_dir);
    if !dir.is_dir() {
        continue;
    }
    let merged = merge_toml_dir(&dir);
    fs::write(out_path.join(format!("{naming_dir}.toml")), &merged).unwrap();
}
```

Also update the `cargo:rerun-if-changed` to include naming_data:
```rust
println!("cargo:rerun-if-changed=compatibility_data/");
println!("cargo:rerun-if-changed=naming_data/");  // ADD THIS LINE
```

**Verify**: `cargo build -p migration-core` → succeeds. Check that `OUT_DIR` contains the merged files (run `cargo build -p migration-core -vv 2>&1 | Select-String "naming"`).

### Step 6: Create `core/src/align/` module

Create these 5 files:

#### `core/src/align/mod.rs`

Module declaration:
```rust
pub mod matcher;
pub mod naming;
pub mod api_map;
pub mod signature;

use crate::diff::FileDiffResult;
use crate::project::SourceLanguage;
use std::path::Path;

/// Main entry: enrich all FileDiffResult symbol changes with target locations.
///
/// Reads the target project (if configured), extracts its symbols, then runs
/// three-level matching for every source change.
pub fn resolve_all(
    file_changes: &mut [FileDiffResult],
    target_root: Option<&Path>,
    source_lang: &str,
    target_lang: &str,
) {
    let Some(root) = target_root else { return };
    if !root.exists() { return; }

    // Parse target project symbols
    let target_symbols = match super::symbols::SymbolExtractor::extract_all_from_dir(
        root, target_lang,
    ) {
        Ok(syms) => syms,
        Err(_) => return,
    };

    let naming_registry = naming::NamingRegistry::new(source_lang, target_lang);
    let api_registry = api_map::ApiMapRegistry::new(source_lang, target_lang);

    for fc in file_changes.iter_mut() {
        for sc in fc.symbol_changes.iter_mut() {
            let result = matcher::match_symbol(
                &fc.file,
                &sc.symbol,
                &sc.kind,
                &sc.details,
                &target_symbols,
                &naming_registry,
                &api_registry,
            );
            sc.target_file = result.target_file;
            sc.target_symbol = result.target_symbol;
        }
    }
}
```

#### `core/src/align/naming.rs`

Follow the compatibility matrix data-loading pattern:

```rust
use std::collections::HashMap;

pub struct NamingRegistry {
    strip_prefixes: Vec<String>,
    case: CaseTransform,
    type_map: Vec<(String, String)>,  // (source_pattern, target_pattern)
}

enum CaseTransform {
    None,
    CamelToSnake,
    SnakeToCamel,
}

impl NamingRegistry {
    pub fn new(source_lang: &str, target_lang: &str) -> Self {
        let dir_key = format!("{}_to_{}", source_lang, target_lang);
        let data = match dir_key.as_str() {
            "typescript_to_rust" => include_str!(concat!(env!("OUT_DIR"), "/ts_to_rust.toml")),
            "rust_to_typescript" => include_str!(concat!(env!("OUT_DIR"), "/rust_to_ts.toml")),
            _ => return Self::default(),
        };
        // Parse toml — extract [convention] and [type_mapping] sections
        // conventions: strip_prefixes (string array), case (string)
        // type_mapping: key-value pairs with {T} template support
        Self { strip_prefixes, case, type_map }
    }

    /// Translate a source symbol name to target convention
    /// e.g. "IUser" → "User", "displayName" → "display_name"
    pub fn translate_name(&self, name: &str) -> String { /* ... */ }

    /// Translate a type string using the type_map
    /// e.g. "Promise<User>" → "Result<User>"
    pub fn translate_type(&self, ty: &str) -> String { /* ... */ }

    /// Generate candidate names (one source → multiple possible target conventions)
    pub fn candidates(&self, name: &str) -> Vec<String> { /* ... */ }
    
    fn default() -> Self {
        Self { strip_prefixes: vec![], case: CaseTransform::None, type_map: vec![] }
    }
}
```

Key logic for `translate_name`:
1. Try stripping each prefix from `strip_prefixes`
2. Apply case transform on the remaining name
3. Return the transformed name

Key logic for `candidates`:
1. Try with and without prefix stripping
2. Try original name and transformed case
3. Return all variants (deduplicated), original first

Key logic for `translate_type`:
1. For each `(from_pattern, to_pattern)` in `type_map`
2. If `from_pattern` contains `{T}` (or `{K}`, `{V}`), strip the outer type wrapper, recursively translate the inner type, wrap in target pattern
3. Return first match, or original unchanged

#### `core/src/align/api_map.rs`

```rust
use std::collections::HashMap;

pub struct ApiMapRegistry {
    /// Source API call pattern → target API call pattern
    /// e.g. "db.findOne" → "db.find_one"
    mapping: HashMap<String, String>,
}

impl ApiMapRegistry {
    pub fn new(source_lang: &str, target_lang: &str) -> Self {
        // Load from embedded data, same pattern as NamingRegistry
    }

    /// Translate a single API call
    pub fn translate_call(&self, call: &str) -> Option<&str> { /* ... */ }

    /// Translate a call chain (e.g. "db.users.findOne" → "db.users().find_one")
    /// Strips intermediate segments, matches the deepest known pattern
    pub fn translate_call_chain(&self, calls: &[String]) -> Vec<String> {
        calls.iter().map(|c| self.translate_call(c).unwrap_or(c).to_string()).collect()
    }
}
```

#### `core/src/align/signature.rs`

```rust
/// Compare parameter counts and types between source and target symbol.
/// Returns a similarity score 0.0–1.0.
pub fn compare_signatures(
    source_name: &str,
    source_params: &[(String, String)],    // (name, type)
    source_return: Option<&str>,
    target_name: &str,
    target_params: &[(String, String)],
    target_return: Option<&str>,
    naming: &super::naming::NamingRegistry,
) -> f64 {
    // 1. Name match after translation (0–0.4)
    let name_score = if naming.translate_name(source_name) == target_name { 0.4 }
                     else { 0.0 };

    // 2. Parameter count compatibility (0–0.3)
    let param_count_score = {
        let diff = (source_params.len() as i32 - target_params.len() as i32).abs();
        if diff == 0 { 0.3 }
        else if diff <= 2 { 0.15 }
        else { 0.0 }
    };

    // 3. Parameter type compatibility (0–0.2)
    let type_score = {
        // For each source param, find best-match target param by type translation
        // score = matched_types / max(source.len, target.len) * 0.2
        0.0 // simplified
    };

    // 4. Return type compatibility (0–0.1)
    let return_score = match (source_return, target_return) {
        (Some(s), Some(t)) => {
            if naming.translate_type(s) == t { 0.1 } else { 0.0 }
        }
        _ => 0.05,
    };

    name_score + param_count_score + type_score + return_score
}

/// Check if the source symbol's children (fields/members) align with the target's children.
pub fn compare_children(
    source_children: &[(&str, Option<&str>)],   // (name, type)
    target_children: &[(&str, Option<&str>)],
    naming: &super::naming::NamingRegistry,
) -> f64 {
    if source_children.is_empty() || target_children.is_empty() {
        return 0.0;
    }
    let translated_source: Vec<String> = source_children.iter()
        .map(|(name, _)| naming.translate_name(name))
        .collect();
    let target_names: Vec<&str> = target_children.iter().map(|(n, _)| *n).collect();

    let matched = translated_source.iter()
        .filter(|sn| target_names.iter().any(|tn| tn == sn))
        .count();
    matched as f64 / source_children.len() as f64
}
```

#### `core/src/align/matcher.rs`

The three-level matching engine:

```rust
use super::naming::NamingRegistry;
use super::api_map::ApiMapRegistry;
use crate::symbols::SymbolIndex;

pub struct MatchResult {
    pub target_file: Option<String>,
    pub target_symbol: Option<String>,
    pub confidence: f64,
}

/// Three-level matching for a single symbol change.
///
/// Level 1 (name + file): Try same-file match with translated name.
/// Level 2 (signature): If ambiguous, compare parameter/return types.
/// Level 3 (ast): Only if level 2 still has multiple candidates — structural comparison.
pub fn match_symbol(
    source_file: &str,
    source_symbol: &str,
    source_kind: &str,
    details: &[crate::diff::ChangeDetail],  // contains field-level changes
    target_symbols: &[SymbolIndex],
    naming: &NamingRegistry,
    api_map: &ApiMapRegistry,
) -> MatchResult { /* ... */ }
```

Implementation notes for `match_symbol`:

```
1. File-level narrowing:
   - Default target = extension swap (same code as current enrich::default_target_file)
   - Find SymbolIndex entries where module == default_target_file
   - If none, search all files

2. Level 1 — name match:
   - Generate name candidates from source_symbol using naming.candidates()
   - For each candidate, search target symbols in narrowed files
   - Unique match → return with confidence 0.85
   - No match → try children (field names) matching for interface/struct

3. Level 2 — signature match (if level 1 ambiguous or no match):
   - For each remaining candidate, compute signature::compare_signatures()
   - For interface/class, compute compare_children()
   - Pick best (if best > threshold 0.5) → confidence based on score
   - Remove candidates below threshold

4. Level 3 — AST match (if still multiple candidates):
   - Only runs when level 2 produced 2+ candidates above threshold
   - Parse source file and each candidate target file
   - Compare structural properties: statement count, call count, control flow depth
   - Boost best match by 0.05
   - If still ambiguous, return first candidate with lowest confidence
```

**Verify**: `cargo build -p migration-core` → no errors.

### Step 7: Remove CLI commands and wire diff flow

#### Remove `migration-analyze/src/commands/registry.rs` and `patch.rs`

Delete both files.

#### Update `migration-analyze/src/commands/mod.rs`

Remove lines:
```rust
pub mod patch;    // remove
pub mod registry; // remove
```

#### Update `migration-analyze/src/main.rs`

Remove the `Registry` and `Patch` enum variants (lines 31-39) and their match arms (lines 52-53).

Keep `CheckUpdates` subcommand.

#### Update `migration-analyze/src/commands/diff.rs`

Replace the current registry enrich block (around lines 115-128):

```rust
    // BEFORE (current code):
    let registry_path = ctx.migration_folder.join("registry.db");
    if registry_path.exists() {
        if let Ok(reg) = SqliteRegistry::new(&registry_path) {
            enrich::enrich_target_info(&mut diff_result.file_changes, Some(&reg));
        }
    } else {
        enrich::enrich_target_info(&mut diff_result.file_changes, None::<...>);
    }

    // AFTER (new code):
    // Align symbol changes against target project (if configured)
    let target_root = config.project.target.as_ref().map(PathBuf::from);
    migration_core::align::resolve_all(
        &mut diff_result.file_changes,
        target_root.as_deref(),
        config.project.source_lang.as_deref().unwrap_or("typescript"),
        &config.project.target_lang,
    );
```

Remove the import lines for `SqliteRegistry` and `enrich` at the top of the file.
Add `use migration_core::align;`.

**Verify**: `cargo build` → no errors, no warnings. `cargo test` → all tests pass (246 unit + 5 e2e).

### Step 8: Fix tests broken by removal

The following test files reference deleted modules and need fixing:

1. **`core/src/patch/generator.rs` tests** (lines ~170-248) — file is deleted in step 2, so no fix needed.
2. **`core/src/patch/validator.rs` tests** (lines ~80-166) — file deleted, no fix needed.
3. **`core/src/registry/sqlite.rs` tests** (lines ~128-226) — file deleted, no fix needed.

Tests that reference `SymbolChange` constructors should not be affected since the struct still exists.

Potential remaining issue: If any test in `core/src/diff/engine.rs` or `core/src/diff/mod.rs` test files references `patch` or `registry` modules, update the import. Search for `use crate::patch` or `use crate::registry` in test files and remove those lines.

**Verify**: `cargo test` → same pass count as before (246 unit + 5 e2e).

### Step 9: Update `core/src/symbols/mod.rs` to expose `extract_all_from_dir`

The current `SymbolExtractor::extract_all` takes a `root: &Path` and `files: &[PathBuf]`. We need a convenience wrapper that discovers files and extracts. Either add:

```rust
impl SymbolExtractor {
    /// Extract symbols from an entire project directory (auto-discover files).
    pub fn extract_all_from_dir(root: &Path, language: &str) -> anyhow::Result<Vec<(SymbolIndex, ApiContract)>> {
        let source_lang = match language {
            "typescript" | "ts" => SourceLanguage::TypeScript,
            "rust" | "rs" => SourceLanguage::Rust,
            _ => anyhow::bail!("Unsupported target language: {}", language),
        };
        let (files, _) = crate::discovery::discover_source_files(root, &source_lang)?;
        Self::extract_all(root, &files, source_lang)
    }
}
```

**Verify**: `cargo build -p migration-core` → no errors.

## Test plan

- All existing tests must continue to pass (`cargo test` exit 0).
- New tests in `core/src/align/naming.rs`:
  - `test_translate_name_strips_prefix` — `IUser` → `User`
  - `test_translate_name_camel_to_snake` — `displayName` → `display_name`
  - `test_translate_name_no_change` — `login` → `login`
  - `test_translate_type_promise` — `Promise<User>` → `Result<User>`
  - `test_translate_type_array` — `Array<string>` → `Vec<String>`
  - `test_translate_type_no_match` — `CustomType` → `CustomType`
- New tests in `core/src/align/api_map.rs`:
  - `test_translate_db_call` — `db.findOne` → `db.find_one`
  - `test_translate_unknown_call` — `foo.bar` → None
- New tests in `core/src/align/matcher.rs`:
  - `test_match_symbol_by_name` — exact name match after translation
  - `test_match_symbol_fallback_default` — no match, gets default extension swap
- New tests in `core/src/align/signature.rs`:
  - `test_compare_signatures_exact` — identical params → 1.0
  - `test_compare_signatures_different` — different params → < 1.0

Pattern to follow for tests: `core/src/registry/enrich.rs` had a similar test structure (see `test_default_target_file`)

## Done criteria

- [ ] `cargo build` exits 0 with no warnings
- [ ] `cargo test` exits 0, all 246+ unit tests pass + 5 e2e tests
- [ ] `core/src/registry/` directory no longer exists
- [ ] `core/src/patch/` directory no longer exists
- [ ] `core/src/align/` exists with mod.rs, naming.rs, api_map.rs, signature.rs, matcher.rs
- [ ] `core/naming_data/` exists with 8 TOML files
- [ ] `migration-analyze/src/commands/registry.rs` and `patch.rs` no longer exist
- [ ] `migration-analyze/src/main.rs` has no Registry/Patch subcommands
- [ ] `grep -rn "rusqlite" Cargo.toml core/Cargo.toml` returns no matches
- [ ] `grep -rn "registry" core/src/lib.rs` returns no matches (except unrelated uses)
- [ ] `grep -rn "patch" core/src/lib.rs` returns no matches (except unrelated uses)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The code at the locations in "Current state" doesn't match the excerpts (codebase has drifted).
- A step's verification fails twice after a reasonable fix attempt.
- The fix requires touching an out-of-scope file.
- Removing `registry` or `patch` causes compilation errors that can't be resolved by simply removing imports — report the exact error and file.
- `cargo test` fails after step 9 and it's not caused by the removed modules — report which test and the error.

## Maintenance notes

- The `align` module reads both source and target project symbols at diff time. If the target project is large (>1000 files), consider lazy-loading or caching the target symbol index.
- Adding a new language pair (e.g. typescript → python) requires:
  1. Creating a new directory `core/naming_data/ts_to_python/` with conventions.toml, type_map.toml, api_map.toml
  2. Updating `core/build.rs` to include the new directory
  3. Updating `NamingRegistry::new()` to handle the new language pair
- The API map uses exact string matching. If pattern-based matching is needed later (wildcards, regex), refactor `api_map.rs` accordingly.
- The `[mapping].override_list` in `migration.toml` can be used by the align module as a pre-check before the three-level matching. This is deferred from this plan.
