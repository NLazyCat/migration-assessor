# Plan 004: Eliminate Rust/TS DiffAnalyzer duplication via trait default methods

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat d596a21..HEAD -- core/src/language/rust.rs core/src/language/typescript.rs core/src/language/mod.rs`
> If any in-scope file changed, compare excerpts against live code; on mismatch, STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: plan 002 (TS extract_imports/extract_call_graph fix)
- **Category**: tech-debt
- **Planned at**: commit `d596a21`, 2026-07-20

## Why this matters

`RustDiffAnalyzer` and `TypeScriptDiffAnalyzer` implement `DiffAnalyzer` with `diff_files()` (~106 lines each) and `diff_symbols()` (~34 lines each) that are structurally identical — only the concrete type passed to `extract_symbols()` differs (RustLanguage vs TypeScriptLanguage). This is ~280 lines of duplicate code that must be kept in sync. Moving the common logic to default trait methods eliminates the duplication.

## Current state

**`core/src/language/mod.rs:42-60`** — the `DiffAnalyzer` trait:
```rust
pub trait DiffAnalyzer: Send + Sync + 'static {
    fn diff_files(&self, old_parsed: &ParsedFile, new_parsed: &ParsedFile) -> anyhow::Result<FileDiffResult>;
    fn diff_symbols(&self, old_sym: &Symbol, new_sym: &Symbol, old_ast: &AstNode, new_ast: &AstNode) -> anyhow::Result<Vec<SymbolChange>>;
    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String>;
    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)>;
}
```

**`language/rust.rs:84-227`** and **`language/typescript.rs:76-226`** — nearly identical `diff_files` (differences: line 90 calls `RustLanguage` vs line 82 calls `TypeScriptLanguage`; the `extract_imports`/`extract_call_graph` bodies are the real per-language logic).

## Scope

**In scope**:
- `core/src/language/mod.rs` — add default methods to `DiffAnalyzer`
- `core/src/language/rust.rs` — remove duplicated `diff_files` and `diff_symbols`
- `core/src/language/typescript.rs` — remove duplicated `diff_files` and `diff_symbols`

**Out of scope**: `engine.rs`, `extract_imports`, `extract_call_graph` — keep those as trait methods.

## Steps

### Step 1: Add default methods to DiffAnalyzer trait

In `core/src/language/mod.rs`, add default implementations for `diff_files` and `diff_symbols`:

```rust
pub trait DiffAnalyzer: Send + Sync + 'static {
    fn diff_files(
        &self,
        old_parsed: &ParsedFile,
        new_parsed: &ParsedFile,
    ) -> anyhow::Result<super::diff::FileDiffResult> {
        // ... shared logic from rust.rs:90-191
        // The key: call Language::extract_symbols via the parsed file's language
        // BUT we need a Language reference to call extract_symbols.
        // Use LanguageRegistry to get it from parsed.language.
        let lang_registry = crate::language::LanguageRegistry::get();
        let lang_name = &old_parsed.language;
        let language = lang_registry.get_language(lang_name)
            .ok_or_else(|| anyhow::anyhow!("Language {} not found", lang_name))?;
        let (old_index, _) = language.extract_symbols(old_parsed)?;
        let (new_index, _) = language.extract_symbols(new_parsed)?;
        // ... rest of the logic identical to current
    }

    fn diff_symbols(
        &self,
        old_sym: &Symbol,
        new_sym: &Symbol,
        _old_ast: &AstNode,
        _new_ast: &AstNode,
    ) -> anyhow::Result<Vec<super::diff::SymbolChange>> {
        // ... shared logic from rust.rs:193-227
    }

    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String>;
    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)>;
}
```

**Key insight**: Instead of hardcoding `RustLanguage::extract_symbols()` vs `TypeScriptLanguage::extract_symbols()`, use `LanguageRegistry::get().get_language(&parsed.language)` to dispatch dynamically. This makes the default method language-agnostic.

Add required imports to `mod.rs`: `use crate::diff::{FileDiffResult, SymbolChange};`.

**Verify**: `cargo build --workspace` → compiles.

### Step 2: Remove duplicated methods from RustDiffAnalyzer

In `core/src/language/rust.rs`, remove the `diff_files` and `diff_symbols` method bodies from the `impl DiffAnalyzer for RustDiffAnalyzer` block. Only keep `extract_imports` and `extract_call_graph`.

```rust
impl DiffAnalyzer for RustDiffAnalyzer {
    // diff_files — inherited from default
    // diff_symbols — inherited from default
    
    fn extract_imports(&self, parsed: &ParsedFile) -> Vec<String> {
        // existing implementation...
    }
    
    fn extract_call_graph(&self, parsed: &ParsedFile) -> Vec<(String, String)> {
        // existing implementation...
    }
}
```

**Verify**: `cargo build --workspace` → compiles.

### Step 3: Remove duplicated methods from TypeScriptDiffAnalyzer

Same as step 2 for `core/src/language/typescript.rs`. Only keep `extract_imports` and `extract_call_graph`.

**Verify**: `cargo clippy --workspace -- -D warnings` → clean. `cargo test --workspace` → all pass.

## Test plan

Existing e2e tests cover the diff pipeline. Verify `cargo test -p migration-analyze` passes all e2e tests.

## Done criteria

- [ ] `cargo clippy --workspace -- -D warnings` exits 0
- [ ] `cargo test --workspace` exits 0; all tests pass
- [ ] No `diff_files` or `diff_symbols` implementation in `rust.rs` or `typescript.rs` (only in `mod.rs` as default methods)

## STOP conditions

- Code excerpts don't match live code (drift).
- `LanguageRegistry::get()` returns no language for a name that was previously working — investigate.
- Any e2e test fails — the dynamic dispatch via `LanguageRegistry` may behave differently from the static dispatch.
