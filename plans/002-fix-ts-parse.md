# Plan 002: Fix TypeScript extract_imports / extract_call_graph to work without AST from parse()

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat d596a21..HEAD -- core/src/language/typescript.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: MED
- **Depends on**: none
- **Category**: bugs
- **Planned at**: commit `d596a21`, 2026-07-20

## Why this matters

`TypeScriptLanguage::parse()` returns `AstNode::Other(json!({}))` — a dummy AST (line 26). This is because `AstNode::TypeScript(oxc_ast::ast::Program<'a>)` borrows from an `oxc_allocator::Allocator` that can't outlive `parse()`.

The consequence: `TypeScriptDiffAnalyzer::extract_imports()` and `extract_call_graph()` match `AstNode::TypeScript(program)` and produce empty results when called through the trait path (e.g., `DiffEngine::diff_project()`). The fix: rewrite these two methods to re-parse `parsed.source` directly, removing the dependency on `parsed.ast`.

## Current state

**`core/src/language/typescript.rs:228-262`** — `extract_imports` matches on `AstNode::TypeScript`:
```rust
fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String> {
    if let AstNode::TypeScript(program) = &parsed.ast {
        // ... body ...
    } else {
        Vec::new()
    }
}
```

**`core/src/language/typescript.rs:264-276`** — `extract_call_graph` same pattern:
```rust
fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)> {
    if let AstNode::TypeScript(program) = &parsed.ast {
        // ... body ...
    } else {
        Vec::new()
    }
}
```

Both always return empty because `parse()` stores `AstNode::Other(json!({}))`.

**Existing pattern for parsing**: `core/src/parser/typescript.rs` re-parses source using oxc — model the fix on that. Also `core/src/util::detect_source_type` provides `SourceType` from a file path.

## Commands you will need

| Purpose | Command | Expected |
|---------|---------|----------|
| Build | `cargo build --workspace` | exit 0 |
| Clippy | `cargo clippy --workspace -- -D warnings` | clean |
| Tests | `cargo test --workspace` | all pass |

## Scope

**In scope**: `core/src/language/typescript.rs` only.

**Out of scope**: `core/src/parser/typescript.rs`, `core/src/symbols/typescript.rs`, `core/src/references/typescript.rs` — they already parse correctly independently.

## Steps

### Step 1: Rewrite `extract_imports` to re-parse from source

Replace the body of `extract_imports` (currently matching on `AstNode::TypeScript`) to re-parse `parsed.source` using oxc directly:

```rust
fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String> {
    let source_type = crate::util::detect_source_type(Some(Path::new(&parsed.file_path)));
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, &parsed.source, source_type).parse();
    let mut imports = Vec::new();
    for stmt in &ret.program.body {
        // same import-extraction logic as current lines 231-254
        match stmt {
            Statement::ImportDeclaration(import) => {
                let src = import.source.value.to_string();
                if !src.starts_with('.') && !src.starts_with('/') {
                    imports.push(src);
                }
            }
            Statement::ExportNamedDeclaration(export) => {
                if let Some(source) = &export.source {
                    let src = source.value.to_string();
                    if !src.starts_with('.') && !src.starts_with('/') {
                        imports.push(src);
                    }
                }
            }
            Statement::ExportAllDeclaration(export) => {
                let src = export.source.value.to_string();
                if !src.starts_with('.') && !src.starts_with('/') {
                    imports.push(src);
                }
            }
            _ => {}
        }
    }
    imports.sort();
    imports.dedup();
    imports
}
```

Add the required imports (`Allocator`, `Parser`) if they aren't already imported. Remove `use serde_json;` if it was only used for `json!({})` and is no longer needed.

**Verify**: `cargo build --workspace` → compiles.

### Step 2: Rewrite `extract_call_graph` to re-parse from source

Same approach — replace the `if let AstNode::TypeScript(program) = &parsed.ast { ... } else { Vec::new() }` with a standalone parse:

```rust
fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)> {
    let source_type = crate::util::detect_source_type(Some(Path::new(&parsed.file_path)));
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, &parsed.source, source_type).parse();
    let mut calls = Vec::new();
    for stmt in &ret.program.body {
        self.visit_statement(stmt, &mut calls);
    }
    calls.sort();
    calls.dedup();
    calls
}
```

**Verify**: `cargo build --workspace` → compiles.

### Step 3: Clean up and verify

Remove imports that are no longer needed (e.g., `use serde_json;`). Remove `AstNode::TypeScript` from the import if no other code in this file uses it.

**Verify**: `cargo clippy --workspace -- -D warnings` → clean. `cargo test --workspace` → all 15 pass.

## Test plan

No new tests. Existing e2e tests still pass. The `extract_imports`/`extract_call_graph` methods are currently dead code (never produce results), so the fix adds working functionality without breaking anything.

## Done criteria

- [ ] `cargo clippy --workspace -- -D warnings` exits 0
- [ ] `cargo test --workspace` exits 0; all 15 tests pass
- [ ] No `AstNode::TypeScript` matching in `extract_imports` or `extract_call_graph` — both re-parse from `parsed.source` directly

## STOP conditions

- The code at `typescript.rs:228-276` doesn't match the excerpts (codebase drifted).
- Existing tests break — investigate and report before fixing.
- The `Program` body iteration in oxc returns different AST types than expected — report the actual types.
