# Plan 001: Dedup git functions from engine.rs / git_utils.rs

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat d596a21..HEAD -- core/src/diff/engine.rs migration-analyze/src/commands/diff/git_utils.rs`
> If any in-scope file changed since this plan was written, compare the
> "Current state" excerpts against the live code before proceeding; on a
> mismatch, treat it as a STOP condition.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: tech-debt
- **Planned at**: commit `d596a21`, 2026-07-20
- **Issue**: —

## Why this matters

Two identical pairs of `get_changed_files` + `get_file_at_version` exist in `core/src/diff/engine.rs` (private) and `migration-analyze/src/commands/diff/git_utils.rs` (public). Any bug fix or behavior change to these git operations must be applied twice, and they will inevitably drift. Extracting to a single shared module removes the duplication risk.

## Current state

**`core/src/diff/engine.rs:80-93`** — private `get_changed_files`:
```rust
fn get_changed_files(project_root: &Path, from_version: &str, to_version: &str) -> anyhow::Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", from_version, to_version])
        .current_dir(project_root)
        .output()?;
    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(files)
}
```

**`core/src/diff/engine.rs:95-106`** — private `get_file_at_version`:
```rust
fn get_file_at_version(project_root: &Path, version: &str, file_path: &str) -> anyhow::Result<String> {
    let output = std::process::Command::new("git")
        .args(["show", &format!("{}:{}", version, file_path)])
        .current_dir(project_root)
        .output()?;
    if !output.status.success() {
        return Err(anyhow::anyhow!("Failed to get file {} at version {}", file_path, version));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
```

**`migration-analyze/src/commands/diff/git_utils.rs:3-29`** — pub `get_changed_files` + `get_file_at_version` — identical bodies.

**Repo conventions**: Follow existing code style — `anyhow::Result`, `std::process::Command`, path handling via `Path`. Use `pub(crate)` visibility for core crate functions. See `core/src/util.rs` for the existing shared-utility pattern.

## Commands you will need

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Build | `cargo build --workspace` | exit 0 |
| Clippy | `cargo clippy --workspace -- -D warnings` | exit 0, no errors |
| Tests | `cargo test --workspace` | all pass |
| Fmt check | `cargo fmt --check` | exit 0 |

## Scope

**In scope** (only files you should modify):
- `core/src/diff/engine.rs` — remove private functions, call shared ones
- `migration-analyze/src/commands/diff/git_utils.rs` — remove duplicated functions, re-export or call from core
- `core/src/util.rs` — only if adding there (but we won't — see step 1)

**Out of scope** (do NOT touch):
- Any test files — plan 005 covers testing
- Any other function in `git_utils.rs` (fetch_latest_version, create_temp_dir, is_analyzable_file)

## Git workflow

- Branch: `advisor/001-dedup-git-utils`
- Commit per step; message style: conventional commits — `refactor: extract shared git functions to core::git`
- Do NOT push or open a PR unless the operator instructed it.

## Steps

### Step 1: Create `core/src/git.rs`

Create a new module `core/src/git.rs` with `pub(crate)` functions:

```rust
use std::path::Path;

/// List files changed between two git revisions.
pub(crate) fn get_changed_files(
    project_root: &Path,
    from_version: &str,
    to_version: &str,
) -> anyhow::Result<Vec<String>> {
    let output = std::process::Command::new("git")
        .args(["diff", "--name-only", from_version, to_version])
        .current_dir(project_root)
        .output()?;
    let files: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(files)
}
```

and `get_file_at_version` with the same body as engine.rs:95-106.

Register the module in `core/src/lib.rs` by adding `pub mod git;`.

**Verify**: `cargo build --workspace` → compiles. The module is pub(crate) so it's only visible within core.

### Step 2: Update `core/src/diff/engine.rs`

Add `use crate::git;` at the top. Remove the two private functions (lines 80-106). Replace the calls inside `DiffEngine::diff_project()`:

- Line 33: `get_changed_files(...)` → `git::get_changed_files(...)`
- Line 37: `get_file_at_version(...)` → `git::get_file_at_version(...)`

**Verify**: `cargo clippy --workspace -- -D warnings` → clean.

### Step 3: Update `migration-analyze/src/commands/diff/git_utils.rs`

Remove the two duplicated functions (lines 3-29). Add re-exports at the top:

```rust
pub use migration_core::git::get_changed_files;
pub use migration_core::git::get_file_at_version;
```

Keep all other functions in the file untouched.

**Verify**: `cargo clippy --workspace -- -D warnings` → clean. `cargo test --workspace` → all 15 pass.

## Test plan

No new tests in this plan (plan 005 covers diff module tests). Existing e2e tests (`e2e_diff`) already exercise the git utils path — verify they still pass:
- `cargo test -p migration-analyze --test e2e_diff` → ok

## Done criteria

ALL must hold:
- [ ] `cargo clippy --workspace -- -D warnings` exits 0
- [ ] `cargo test --workspace` exits 0; all 15+ tests pass
- [ ] `grep -rn "fn get_changed_files" core/src/diff/engine.rs` returns no match (removed from engine.rs)
- [ ] `grep -rn "fn get_changed_files" migration-analyze/src/commands/diff/git_utils.rs` returns no match (removed from git_utils.rs)
- [ ] `grep -rn "pub use migration_core::git::get_changed_files" migration-analyze/src/commands/diff/git_utils.rs` matches (re-export added)
- [ ] No files outside the in-scope list are modified (`git status`)
- [ ] `plans/README.md` status row updated

## STOP conditions

Stop and report back (do not improvise) if:

- The code at `engine.rs:80-106` or `git_utils.rs:3-29` doesn't match the excerpts (codebase drifted).
- A step's verification fails twice after a reasonable fix attempt.
- The fix requires touching an out-of-scope file.
- `cargo test --workspace` discovers that `e2e_diff` tests were using internal state of `git_utils.rs` beyond the two functions.

## Maintenance notes

- If `engine.rs`'s `diff_project` method is ever removed or its git logic changes, `core/src/git.rs` should be updated independently and both consumers will benefit automatically.
- Future git operations should be added to `core/src/git.rs`, not duplicated.
