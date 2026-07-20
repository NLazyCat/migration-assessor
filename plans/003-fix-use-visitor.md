# Plan 003: Fix Rust UseVisitor empty implementation

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat d596a21..HEAD -- core/src/language/rust.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bugs
- **Planned at**: commit `d596a21`, 2026-07-20

## Why this matters

`RustLanguage::extract_references()` uses a `UseVisitor` that implements `Visit<'ast>` with an empty `visit_item_use` body. The visitor's `imports` vector is never populated, so the forward/reverse index maps are always empty. External crate dependencies are never discovered through this path, making the reference index useless for Rust files when called through the `Language` trait.

## Current state

**`core/src/language/rust.rs:72-78`** — the empty UseVisitor:
```rust
struct UseVisitor {
    imports: Vec<String>,
}

impl<'ast> Visit<'ast> for UseVisitor {
    fn visit_item_use(&mut self, _node: &'ast syn::ItemUse) {
    }
}
```

The `syn::visit::Visit` trait is already imported. The `extract_references` method (lines 38-57) creates the visitor and calls `visitor.visit_file(file)`, then iterates `visitor.imports` — which is always empty.

**Pattern to follow**: The `syn::ItemUse` node has a `tree` field of type `syn::UseTree` with variants `Path`, `Name`, `Glob`, `Group`. Extract path segments from `UseTree` to reconstruct the full import path string.

## Commands you will need

| Purpose | Command | Expected |
|---------|---------|----------|
| Build | `cargo build --workspace` | exit 0 |
| Clippy | `cargo clippy --workspace -- -D warnings` | clean |
| Tests | `cargo test --workspace` | all pass |

## Scope

**In scope**: `core/src/language/rust.rs` — only the `UseVisitor` impl block (lines 75-78).

**Out of scope**: `core/src/references/rust.rs` — the existing full reference extractor. Do not duplicate its logic.

## Steps

### Step 1: Implement `visit_item_use`

Replace the empty body with logic to extract the full use path as a string:

```rust
impl<'ast> Visit<'ast> for UseVisitor {
    fn visit_item_use(&mut self, node: &'ast syn::ItemUse) {
        let path_str = use_tree_to_string(&node.tree);
        if !path_str.is_empty() {
            self.imports.push(path_str);
        }
    }
}
```

Add a helper function `use_tree_to_string`:
```rust
fn use_tree_to_string(tree: &syn::UseTree) -> String {
    match tree {
        syn::UseTree::Path(path) => {
            let prefix = path.ident.to_string();
            let rest = use_tree_to_string(&path.tree);
            if rest.is_empty() { prefix } else { format!("{}::{}", prefix, rest) }
        }
        syn::UseTree::Name(name) => name.ident.to_string(),
        syn::UseTree::Rename(rename) => rename.ident.to_string(),
        syn::UseTree::Glob(_) => "*".to_string(),
        syn::UseTree::Group(group) => {
            let items: Vec<String> = group.items.iter().map(|t| use_tree_to_string(t)).collect();
            items.join(", ")
        }
    }
}
```

Place the helper function before `UseVisitor` in the file.

**Verify**: `cargo build --workspace` → compiles.

### Step 2: Verify clippy and tests

**Verify**: `cargo clippy --workspace -- -D warnings` → clean. `cargo test --workspace` → all 15 pass.

## Test plan

No new tests. The `extract_references` return values are only used downstream by the diff engine. Existing e2e tests verify the overall pipeline doesn't break.

## Done criteria

- [ ] `cargo clippy --workspace -- -D warnings` exits 0
- [ ] `cargo test --workspace` exits 0; all 15+ tests pass
- [ ] `visit_item_use` body is non-empty and pushes to `self.imports`

## STOP conditions

- The code at `rust.rs:75-78` doesn't match the excerpt (codebase drifted).
- syn API changes between versions — if `UseTree` variants differ, report the actual variants.
