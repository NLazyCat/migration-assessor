# Plan 005: Add unit tests for core/src/diff/ modules

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat d596a21..HEAD -- core/src/diff/`
> If any in-scope file changed, compare excerpts against live code; on mismatch, STOP.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: LOW
- **Depends on**: plan 001 (git utils dedup)
- **Category**: tests
- **Planned at**: commit `d596a21`, 2026-07-20

## Why this matters

The `core/src/diff/` module has 7 source files and zero `#[test]` functions. The signature comparison logic (`signature.rs`), the LCS-based symbol mapping (`mapping.rs`), the version comparison (`dependency.rs`), and the value diffing (`logic.rs`) are all untested. Bugs in these modules would silently produce incorrect diff results in the migration report.

## Current state

All files in `core/src/diff/`:
- `mod.rs` — types (SymbolChange, FileDiffResult, DiffReport, etc.)
- `mapping.rs` — `build_symbol_mapping()` with LCS algorithm + `structural_similarity()`
- `signature.rs` — `diff()` comparing two Symbol structs
- `logic.rs` — `diff_value()` for constant/value changes
- `doc.rs` — `diff()` for documentation changes
- `dependency.rs` — `compare_versions()`, `is_major_bump()`, dep change analysis
- `engine.rs` — `DiffEngine::diff_project()` orchestrator

**Test pattern to follow**: Look at `core/src/scores.rs` for the existing test style — `#[cfg(test)] mod tests { use super::*; #[test] fn ... }`.

## Commands you will need

| Purpose | Command | Expected |
|---------|---------|----------|
| Tests | `cargo test --workspace` | all pass |

## Scope

**In scope**: Only test additions within `core/src/diff/` files. Add `#[cfg(test)] mod tests { ... }` blocks to each file.

**Out of scope**: Do NOT modify any production logic. Do NOT add tests for `engine.rs` (it depends on git/parsing; testing it requires integration tests). Do NOT modify `migration-analyze` tests.

## Steps

### Step 1: Add tests to `diff/mapping.rs`

Add a `#[cfg(test)] mod tests` block at the end. Cover:

1. **`build_symbol_mapping` with identical symbols** — create two identical `SymbolIndex` instances (same symbols, same IDs). Verify `stable` contains all symbols, `added`/`removed`/`renamed` are empty.

2. **`build_symbol_mapping` with renamed symbol** — two indices where one symbol's name differs but content is the same. Verify `renamed` has 1 entry.

3. **`build_symbol_mapping` with added/removed** — one index has extra symbol. Verify `added` or `removed` is non-empty.

4. **`structural_similarity`** — test with identical, similar, and different parameter lists.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbols::Symbol;

    fn make_symbol(id: &str, name: &str) -> Symbol {
        Symbol {
            id: id.to_string(),
            name: name.to_string(),
            kind: "function".to_string(),
            line_range: [1, 10],
            children: vec![],
            partial_analysis: false,
            partial_reason: None,
            visibility: Some(crate::symbols::Visibility::Public),
            value: None,
            signature: None,
            doc_comment: None,
            attributes: vec![],
            is_async: None,
            return_type: None,
            params: None,
        }
    }

    #[test]
    fn test_identical_symbols() {
        let mut old_idx = SymbolIndex::default();
        let mut new_idx = SymbolIndex::default();
        old_idx.symbols.push(make_symbol("a", "foo"));
        new_idx.symbols.push(make_symbol("a", "foo"));
        let result = build_symbol_mapping(&old_idx, &new_idx);
        assert_eq!(result.stable.len(), 1);
        assert!(result.added.is_empty());
        assert!(result.removed.is_empty());
        assert!(result.renamed.is_empty());
    }

    #[test]
    fn test_added_symbol() {
        let mut old_idx = SymbolIndex::default();
        let mut new_idx = SymbolIndex::default();
        old_idx.symbols.push(make_symbol("a", "foo"));
        new_idx.symbols.push(make_symbol("a", "foo"));
        new_idx.symbols.push(make_symbol("b", "bar"));
        let result = build_symbol_mapping(&old_idx, &new_idx);
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].name, "bar");
    }

    #[test]
    fn test_removed_symbol() {
        let mut old_idx = SymbolIndex::default();
        let mut new_idx = SymbolIndex::default();
        old_idx.symbols.push(make_symbol("a", "foo"));
        old_idx.symbols.push(make_symbol("b", "bar"));
        new_idx.symbols.push(make_symbol("a", "foo"));
        let result = build_symbol_mapping(&old_idx, &new_idx);
        assert_eq!(result.removed.len(), 1);
        assert_eq!(result.removed[0].name, "bar");
    }
}
```

**Verify**: `cargo test -p migration-core mapping` → the new tests pass.

### Step 2: Add tests to `diff/signature.rs`

Test `diff()` with:
1. Two identical symbols → `None`
2. Symbol with changed parameters (different count) → `Some` with appropriate detail
3. Symbol with changed return type → `Some`
4. Symbol with changed async flag → `Some`

### Step 3: Add tests to `diff/logic.rs`

Test `diff_value()` with:
1. Same value → `None`
2. Different value → `Some(SymbolChange)` with change_type "value_changed"

### Step 4: Add tests to `diff/doc.rs`

Test `diff()` with:
1. Same doc comment → `None`
2. Different doc comment → `Some(DocChange)` with change_type "changed"
3. Doc added (None → Some) → change_type "added"
4. Doc removed (Some → None) → change_type "removed"
5. Deprecation attribute added → `is_deprecated: true`

### Step 5: Add tests to `diff/dependency.rs`

Test `compare_versions()`:
1. `"1.2.3"` vs `"1.2.3"` → `None` (same)
2. `"1.2.3"` vs `"2.0.0"` → `Some(DepChangeInfo)` with level "major"
3. `"1.2.3"` vs `"1.3.0"` → level "minor"
4. `"1.2.3"` vs `"1.2.4"` → level "patch"

Test `is_major_bump()`:
1. `"1.0.0"` to `"2.0.0"` → `true`
2. `"1.0.0"` to `"1.1.0"` → `false`

**Verify**: `cargo test --workspace` → all pass, including the new tests.

## Test plan

The tests themselves are the deliverable. Each test follows the pattern in `core/src/scores.rs`:
- `#[cfg(test)] mod tests { use super::*; }`
- Simple Arrange-Act-Assert with minimal boilerplate
- Test functions named `test_<what>_<scenario>`

## Done criteria

- [ ] `cargo test --workspace` exits 0; at least 15 new test functions pass across `mapping.rs`, `signature.rs`, `logic.rs`, `doc.rs`, `dependency.rs`
- [ ] Every function exported from these modules has at least one test covering the happy path
- [ ] No changes to production logic

## STOP conditions

- Code excerpts don't match live code (drift).
- `SymbolIndex` or `Symbol` struct fields differ from what's shown (they may have been extended).
- Any existing test breaks — investigate before adding new tests.
