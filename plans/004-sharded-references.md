# Plan 004: Shard reference indexes and drop monolithic forward/reverse JSON

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- migration-analyze/src/commands/analyze.rs migration-analyze/src/web/routes.rs core/src/output.rs core/src/references/`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: 003
- **Category**: perf
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

`analyze` currently writes both full `references/forward.json` and `references/reverse.json` plus per-file shards under `references/by-dir/`. On large projects the monolithic files can reach hundreds of megabytes, causing high memory usage during serialization and slow loading in the web UI. Since the serve API already exposes per-file references, the monolithic files are redundant.

## Current state

- `migration-analyze/src/commands/analyze.rs:199-224`:
  ```rust
  let (forward, reverse): (references::ForwardIndex, references::ReverseIndex) =
      match project.source_language { ... };
  output.write_json(&report_dir, "references/forward.json", &forward)?;
  output.write_json(&report_dir, "references/reverse.json", &reverse)?;

  // Per-file references
  let file_refs = group_references_by_file(&forward, &reverse);
  for (file, refs) in &file_refs {
      let fwd_path = format!("references/by-dir/{}.forward.json", file);
      let rev_path = format!("references/by-dir/{}.reverse.json", file);
      output.write_json(&report_dir, &fwd_path, &refs.forward)?;
      output.write_json(&report_dir, &rev_path, &refs.reverse)?;
  }
  ```
- `migration-analyze/src/web/routes.rs:83-85`:
  ```rust
  pub async fn api_references(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
      Json(read_json(&state.report_dir, "references/reverse.json"))
  }
  ```
- `page_report_ref` and `api_file_references` already consume the per-file shards.

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |
| E2E       | `cargo run -p migration-analyze -- analyze <fixture>` | produces expected shards, no forward.json/reverse.json |

## Scope

**In scope**:
- `migration-analyze/src/commands/analyze.rs`
- `migration-analyze/src/web/routes.rs`
- Any tests that assert the existence of `references/forward.json` or `references/reverse.json`

**Out of scope**:
- Renaming `references/by-dir/` (covered in plan 006).
- Changing the schema of per-file reference JSON.

## Steps

### Step 1: Remove monolithic reference writes in `analyze.rs`

Delete these two lines from `analyze.rs`:
```rust
output.write_json(&report_dir, "references/forward.json", &forward)?;
output.write_json(&report_dir, "references/reverse.json", &reverse)?;
```

Keep the per-file shard generation and writes.

**Verify**: `grep -n "references/forward.json\|references/reverse.json" migration-analyze/src/commands/analyze.rs` returns no matches.

### Step 2: Replace `api_references` with a lightweight listing endpoint

The old endpoint loaded the full reverse index. Change it to return a manifest of available per-file reference shards so the UI can fetch them on demand.

In `migration-analyze/src/web/routes.rs`, replace:
```rust
pub async fn api_references(State(state): State<Arc<AppState>>) -> Json<Option<Value>> {
    Json(read_json(&state.report_dir, "references/reverse.json"))
}
```

With:
```rust
pub async fn api_references(State(state): State<Arc<AppState>>) -> Json<Value> {
    let mut files = Vec::new();
    let dir = state.report_dir.join("references").join("by-dir");
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".forward.json") {
                let base = name.trim_end_matches(".forward.json").to_string();
                files.push(base);
            }
        }
    }
    files.sort();
    files.dedup();
    Json(serde_json::json!({ "files": files }))
}
```

**Verify**: `cargo check -p migration-analyze` succeeds.

### Step 3: Update consumers of `/api/references`

Find any frontend code in `templates.rs` or static files that calls `/api/references` expecting the full reverse index. Update it to:
1. Call `/api/references` to get the file list.
2. Call `/api/references/<file>` for each file the user selects.

If no frontend code consumes `/api/references` directly, document that the endpoint contract has changed.

**Verify**: `grep -rn "api/references" migration-analyze/src/web/` lists all usages and they match the new contract.

### Step 4: Update tests

Search for tests that check for `references/forward.json` or `references/reverse.json`:

```bash
grep -rn "references/forward.json\|references/reverse.json" migration-analyze/ core/
```

Remove or update those assertions. Add an assertion that at least one per-file shard exists after analyzing a fixture.

**Verify**: `cargo test --workspace` passes.

### Step 5: Run full verification

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

**Verify**: All three exit 0.

## Test plan

- Add or extend an integration test that runs `analyze` on a small fixture and asserts:
  - `references/forward.json` does NOT exist.
  - `references/reverse.json` does NOT exist.
  - `references/by-dir/<module>.forward.json` and `references/by-dir/<module>.reverse.json` exist for at least one module.
- Add a unit-style test for `api_references` if the project has HTTP handler tests; otherwise rely on integration tests.

## Done criteria

- [ ] `analyze` no longer writes `references/forward.json` or `references/reverse.json`.
- [ ] `/api/references` returns a file list instead of the full reverse index.
- [ ] Per-file reference shards continue to be written and served via `/api/references/<file>`.
- [ ] `cargo clippy --workspace -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `plans/README.md` status row for plan 004 updated to DONE.

## STOP conditions

Stop and report if:
- The frontend relies on the old `/api/references` full-index contract and cannot be updated within this plan's scope.
- Tests assert the existence of the monolithic files as a critical invariant without an alternative.

## Maintenance notes

- Future work on reference queries should target per-file shards, not full indexes.
- If a future feature genuinely needs a full in-memory reverse index, derive it by walking the shards at load time rather than writing a redundant file.
