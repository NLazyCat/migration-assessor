# Plan 008: Parallelize symbol and reference extraction

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- migration-analyze/src/commands/analyze.rs core/src/symbols/ core/src/references/ Cargo.toml`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: MED
- **Depends on**: 003
- **Category**: perf
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

`analyze.rs` runs symbol extraction and reference extraction sequentially. Both are CPU-bound and operate on independent file sets. Parallelizing within each extractor (and optionally between the two stages) reduces analysis time on multi-core machines, especially for large repositories.

## Current state

- `migration-analyze/src/commands/analyze.rs:102-103`:
  ```rust
  let symbol_results =
      symbols::SymbolExtractor::extract_all(&project.root, &files, project.source_language)?;
  ```
- `migration-analyze/src/commands/analyze.rs:199-207`:
  ```rust
  let (forward, reverse): (references::ForwardIndex, references::ReverseIndex) =
      match project.source_language {
          project::SourceLanguage::TypeScript => {
              references::typescript::extract_all(&project.root, &files)?
          }
          project::SourceLanguage::Rust => {
              references::rust::extract_all(&project.root, &files)?
          }
      };
  ```
- `rayon` is already listed in workspace dependencies and `core/Cargo.toml`.
- `SymbolExtractor::extract_all` and the reference extractors likely iterate over files in a single thread.

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |
| E2E       | Run `analyze` on a multi-file fixture | output identical to serial run, faster or equal |

## Scope

**In scope**:
- `core/src/symbols/mod.rs` and language-specific symbol extractors
- `core/src/references/mod.rs`, `core/src/references/typescript.rs`, `core/src/references/rust.rs`
- `migration-analyze/src/commands/analyze.rs`

**Out of scope**:
- Parallelizing dependency resolution or graph construction (they have global shared state).
- Changing the output schemas of symbol/reference extraction.

## Steps

### Step 1: Parallelize symbol extraction

Open `core/src/symbols/mod.rs` (or the relevant extractor module) and locate the `extract_all` function. If it iterates over files with a `for` loop, convert it to use `rayon::iter::ParallelIterator`:

```rust
use rayon::prelude::*;

pub fn extract_all(root: &Path, files: &[PathBuf], language: SourceLanguage) -> anyhow::Result<Vec<...>> {
    let results: Vec<_> = files
        .par_iter()
        .map(|file| extract_one(root, file, language))
        .collect();

    // Flatten results and propagate errors
    results.into_iter().collect::<Result<Vec<_>, _>>()
}
```

If the current `extract_all` has mutable shared state, refactor it so each file is processed independently. Return a `Result` per file and aggregate after the parallel loop.

**Verify**: `cargo check -p migration-core` succeeds.

### Step 2: Parallelize reference extraction

Repeat Step 1 for `references::typescript::extract_all` and `references::rust::extract_all`.

**Verify**: `cargo check -p migration-core` succeeds.

### Step 3: Parallelize the two stages (optional)

If symbol extraction and reference extraction are now independent and thread-safe, you can run them in parallel using `rayon::join`:

```rust
let (symbol_results, (forward, reverse)) = rayon::join(
    || symbols::SymbolExtractor::extract_all(&project.root, &files, project.source_language),
    || match project.source_language {
        project::SourceLanguage::TypeScript => references::typescript::extract_all(&project.root, &files),
        project::SourceLanguage::Rust => references::rust::extract_all(&project.root, &files),
    },
);
let symbol_results = symbol_results?;
let (forward, reverse) = (forward?, reverse?);
```

If the signatures or error types make this awkward, skip this step and rely on file-level parallelism from Steps 1-2.

**Verify**: `cargo check -p migration-analyze` succeeds.

### Step 4: Verify determinism

Parallel execution must produce deterministic output. After the change, run `analyze` on a fixture multiple times and compare the JSON outputs (or at least the sorted arrays). Symbols and references should not change order in a way that breaks tests.

If necessary, sort results by module name before writing them.

**Verify**: `cargo test --workspace` passes and repeated runs produce identical output.

### Step 5: Full verification

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Run a manual timing comparison on a fixture with at least 10 files:

```bash
# Before (if you can stash changes) or after
cargo run -p migration-analyze -- analyze <fixture>
```

**Verify**: All checks pass and the output is correct.

## Test plan

- Existing tests should continue to pass; parallelization should not change semantics.
- Add a test that runs extraction on a small multi-file fixture and asserts the results are deterministic (run twice, compare).
- If no such test exists, create one in `core/src/symbols/mod.rs` or `core/src/references/mod.rs` under `#[cfg(test)]`.

## Done criteria

- [ ] Symbol extraction uses `rayon` for file-level parallelism.
- [ ] Reference extraction uses `rayon` for file-level parallelism.
- [ ] `cargo clippy --workspace -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] Output remains deterministic after parallelization.
- [ ] `plans/README.md` status row for plan 008 updated to DONE.

## STOP conditions

Stop and report if:
- The extractors rely on mutable shared state that cannot be easily removed.
- Parallelization introduces non-deterministic output that breaks tests.
- `rayon` is not available or causes compilation errors in the target environment.

## Maintenance notes

- Future extractors should follow the same per-file independent pattern to remain parallelizable.
- If incremental cache (plan 005) is also implemented, ensure cached results are still combined correctly in parallel.
