# Plan 005: Add incremental analysis cache

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- migration-analyze/src/commands/analyze.rs core/src/output.rs core/src/`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: 003
- **Category**: perf
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

`OutputWriter::init` currently deletes the entire report directory and `analyze` re-parses every file on every run. For large repositories this wastes time when only a few files changed. A content-addressed incremental cache lets repeated analyses skip unchanged files while still invalidating results when source content or parser versions change.

## Current state

- `core/src/output.rs:7-10`:
  ```rust
  pub fn init(output_dir: &Path) -> anyhow::Result<Self> {
      if output_dir.exists() {
          fs::remove_dir_all(output_dir)?;
      }
      fs::create_dir_all(output_dir)?;
      Ok(Self)
  }
  ```
- `migration-analyze/src/commands/analyze.rs:75-207` runs project detection, file discovery, dependency resolution, compatibility evaluation, graph construction, symbol extraction, and reference extraction sequentially every time.
- There is no cache directory concept yet.

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |
| E2E       | Run `analyze` twice on same repo    | second run is faster and output is identical |

## Scope

**In scope**:
- `core/src/output.rs` (cache directory helpers)
- `core/src/cache.rs` (new module, create)
- `migration-analyze/src/commands/analyze.rs`
- `core/src/lib.rs` (export new module if needed)

**Out of scope**:
- Caching dependency resolution results from external package managers (Cargo/npm metadata changes are hard to invalidate).
- Caching the final HTML report generation.
- Changing output directory names (plan 006).

## Steps

### Step 1: Design cache key schema

Create `core/src/cache.rs` with a cache key type:

```rust
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheKey {
    pub file_hash: String,
    pub parser_version: String,
    pub tool_version: String,
}

impl CacheKey {
    pub fn for_file(path: &Path, parser_version: &str, tool_version: &str) -> anyhow::Result<Self> {
        let content = std::fs::read(path)?;
        let file_hash = format!("{:x}", sha2::Sha256::digest(&content));
        Ok(Self {
            file_hash,
            parser_version: parser_version.to_string(),
            tool_version: tool_version.to_string(),
        })
    }

    pub fn digest(&self) -> String {
        format!("{:x}", sha2::Sha256::digest(serde_json::to_string(self).unwrap().as_bytes()))
    }
}
```

You may use `sha2` or a simpler hasher. If adding `sha2`, add it to `core/Cargo.toml` workspace dependencies. Alternatively, use a built-in hasher via `std::collections::hash_map::DefaultHasher` and `std::hash::Hasher` to avoid new deps.

**Verify**: `cargo check -p migration-core` compiles the new module.

### Step 2: Implement cache storage

Add to `core/src/cache.rs`:

```rust
pub struct AnalysisCache {
    root: PathBuf,
}

impl AnalysisCache {
    pub fn new(project_root: &Path) -> anyhow::Result<Self> {
        let root = project_root.join(".migration-cache");
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    pub fn get(&self, key: &CacheKey) -> Option<serde_json::Value> {
        let path = self.entry_path(key);
        if !path.exists() { return None; }
        let text = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn put(&self, key: &CacheKey, value: &serde_json::Value) -> anyhow::Result<()> {
        let path = self.entry_path(key);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(value)?)?;
        Ok(())
    }

    fn entry_path(&self, key: &CacheKey) -> PathBuf {
        let digest = key.digest();
        let prefix = &digest[..2];
        let suffix = &digest[2..];
        self.root.join(prefix).join(format!("{}.json", suffix))
    }
}
```

**Verify**: `cargo check -p migration-core` succeeds.

### Step 3: Wire cache into symbol and reference extraction

Modify `analyze.rs` to create an `AnalysisCache` and pass it to `SymbolExtractor::extract_all` and the reference extractors.

1. Create the cache before file discovery:
   ```rust
   let cache = migration_core::cache::AnalysisCache::new(&project_root)?;
   ```
2. Update `symbols::SymbolExtractor::extract_all` signature to accept an optional `&AnalysisCache`. If a cached result exists for a file, return it instead of parsing. After parsing, store the result.
3. Do the same for `references::typescript::extract_all` and `references::rust::extract_all`.

If changing signatures is too invasive, you may instead wrap the extractors at the call site: iterate files, check cache, call extractor only on cache misses, and aggregate results.

**Verify**: `cargo check --workspace` succeeds.

### Step 4: Preserve report outputs without full deletion

Change `OutputWriter::init` to stop deleting the report directory. Instead, create it if missing and leave existing contents in place (writes will overwrite individual files).

```rust
pub fn init(output_dir: &Path) -> anyhow::Result<Self> {
    fs::create_dir_all(output_dir)?;
    Ok(Self)
}
```

**Verify**: `cargo check -p migration-core` succeeds.

### Step 5: Version/cache invalidation

Define a `CACHE_VERSION` constant in `cache.rs`. Bump it whenever the parser output schema changes. Include it in `CacheKey` so old cache entries are naturally missed.

**Verify**: A changed `CACHE_VERSION` causes previously cached entries to be re-parsed.

### Step 6: Verification

Run:

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Then run a manual benchmark:

```bash
# First run (cold cache)
cargo run -p migration-analyze -- analyze <fixture>
# Second run (warm cache)
cargo run -p migration-analyze -- analyze <fixture>
```

**Verify**: The second run completes successfully and produces identical output.

## Test plan

- Add unit tests in `core/src/cache.rs`:
  - `CacheKey::for_file` returns the same key for identical content and different keys for different content.
  - `AnalysisCache::put` then `get` round-trips a value.
  - Changing `parser_version` in the key results in a cache miss.
- Add an integration test that runs `analyze` twice and asserts the second run reuses cached entries (e.g., by checking that cache files exist and the report is regenerated).

## Done criteria

- [ ] `core/src/cache.rs` exists and provides `AnalysisCache` with content-addressed keys.
- [ ] `analyze` uses the cache for symbol and reference extraction.
- [ ] `OutputWriter::init` no longer deletes the entire report directory.
- [ ] `.migration-cache/` is created and populated during analysis.
- [ ] `cargo test --workspace` passes.
- [ ] Manual two-run test produces identical output.
- [ ] `plans/README.md` status row for plan 005 updated to DONE.

## STOP conditions

Stop and report if:
- Extractor signatures are too entangled to accept a cache parameter without massive refactoring.
- The cache produces stale results after a source file change (invalidation bug).
- Adding a cache dependency causes licensing or build issues.

## Maintenance notes

- Any change to the JSON schema produced by symbol/reference extractors must bump `CACHE_VERSION`.
- Reviewers should verify that cache hits do not skip necessary re-analysis when global state (like dependency resolution) changes.
- Consider adding a `--no-cache` flag to `AnalyzeArgs` for debugging.
