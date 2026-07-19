# Plan 003: Introduce ProjectContext for command data loading

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- migration-analyze/src/commands/boundaries.rs migration-analyze/src/commands/diff.rs migration-analyze/src/commands/serve.rs migration-analyze/src/commands/mod.rs core/src/`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: 001, 002
- **Category**: tech-debt
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

`boundaries`, `diff`, and `serve` each re-implement `detect_migration_folder` and independently deserialize report JSON. This duplication causes inconsistent error messages and makes output-path changes risky (a future restructure would require touching every command). A single `ProjectContext` type centralizes migration-folder detection, config loading, and lazy/cached report loading.

## Current state

- `migration-analyze/src/commands/boundaries.rs:530-546` contains:
  ```rust
  fn detect_migration_folder(project_root: &Path) -> anyhow::Result<PathBuf> {
      for entry in std::fs::read_dir(project_root)? {
          let entry = entry?;
          let path = entry.path();
          if !path.is_dir() { continue; }
          let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
          if name.ends_with("-migration") && path.join("report").exists() {
              return Ok(path);
          }
      }
      anyhow::bail!("No migration folder (*-migration/) found in {}", project_root.display())
  }
  ```
- `migration-analyze/src/commands/diff.rs:913-930` contains a near-identical copy with a slightly different error message.
- `migration-analyze/src/commands/serve.rs:78-95` contains a third copy, also with a different error message.
- Each command independently reads `report/project.json`, `report/index.json`, `report/scores.json`, `report/internal-deps/dag.json`, `report/api-contracts/by-dir/*.api.json`, etc.
- `migration-analyze/src/commands/mod.rs` likely exists and is the conventional place for shared command utilities (verify before placing code there).

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |

## Scope

**In scope**:
- `migration-analyze/src/commands/mod.rs` (add `ProjectContext` here, or create `migration-analyze/src/commands/context.rs` if `mod.rs` does not exist)
- `migration-analyze/src/commands/boundaries.rs`
- `migration-analyze/src/commands/diff.rs`
- `migration-analyze/src/commands/serve.rs`

**Out of scope**:
- Changing output directory names or JSON schemas (plan 006).
- Adding incremental caching logic (plan 005).
- Modifying `analyze.rs`.

## Steps

### Step 1: Inspect `commands/mod.rs`

Open `migration-analyze/src/commands/mod.rs`. Determine whether it is a natural place for shared types.

- If it exists and exports command modules, add `pub mod context;` or place `ProjectContext` directly in `mod.rs`.
- If it does not exist, create `migration-analyze/src/commands/context.rs`.

**Verify**: `ls migration-analyze/src/commands/` lists the files and `mod.rs` status is known.

### Step 2: Implement `ProjectContext`

Create the context type. It must expose at least the following API:

```rust
use migration_core::config::Config;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct ProjectContext {
    pub project_root: PathBuf,
    pub migration_folder: PathBuf,
    pub report_dir: PathBuf,
    pub config: Config,
    // caches for lazily-loaded JSON
    project_meta: Mutex<Option<serde_json::Value>>,
    index: Mutex<Option<serde_json::Value>>,
    scores: Mutex<Option<serde_json::Value>>,
    dag: Mutex<Option<serde_json::Value>>,
}

impl ProjectContext {
    pub fn load(project_root: impl AsRef<Path>) -> anyhow::Result<Self> {
        let project_root = project_root.as_ref().canonicalize()?;
        let migration_folder = Self::detect_migration_folder(&project_root)?;
        let report_dir = migration_folder.join("report");
        let config = if let Some(p) = Self::find_config(&project_root) {
            Config::load(&p)?
        } else {
            Config::default()
        };

        Ok(Self {
            project_root,
            migration_folder,
            report_dir,
            config,
            project_meta: Mutex::new(None),
            index: Mutex::new(None),
            scores: Mutex::new(None),
            dag: Mutex::new(None),
        })
    }

    fn detect_migration_folder(project_root: &Path) -> anyhow::Result<PathBuf> {
        if let Ok(entries) = std::fs::read_dir(project_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() { continue; }
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with("-migration") && path.join("report").exists() {
                    return Ok(path);
                }
            }
        }
        anyhow::bail!("No migration folder (*-migration/) found in {}", project_root.display())
    }

    fn find_config(project_root: &Path) -> Option<PathBuf> {
        let p = project_root.join("migration.toml");
        if p.exists() { Some(p) } else { None }
    }

    pub fn project_meta(&self) -> anyhow::Result<serde_json::Value> {
        Self::read_json_cached(&self.report_dir.join("project.json"), &self.project_meta)
    }

    pub fn index(&self) -> anyhow::Result<serde_json::Value> {
        Self::read_json_cached(&self.report_dir.join("index.json"), &self.index)
    }

    pub fn scores(&self) -> anyhow::Result<serde_json::Value> {
        Self::read_json_cached(&self.report_dir.join("scores.json"), &self.scores)
    }

    pub fn dag(&self) -> anyhow::Result<serde_json::Value> {
        Self::read_json_cached(&self.report_dir.join("internal-deps/dag.json"), &self.dag)
    }

    pub fn report_path(&self, relative: &str) -> PathBuf {
        self.report_dir.join(relative)
    }

    pub fn load_json<T: serde::de::DeserializeOwned>(&self, relative: &str) -> anyhow::Result<T> {
        let path = self.report_path(relative);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))
    }

    fn read_json_cached(path: &Path, cache: &Mutex<Option<serde_json::Value>>) -> anyhow::Result<serde_json::Value> {
        if let Some(v) = cache.lock().unwrap().as_ref() {
            return Ok(v.clone());
        }
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read {}: {}", path.display(), e))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| anyhow::anyhow!("Failed to parse {}: {}", path.display(), e))?;
        *cache.lock().unwrap() = Some(value.clone());
        Ok(value)
    }
}
```

Adjust imports and module layout to match the existing project structure. If `migration_core::config::Config` is not public as needed, re-export it or import from `migration_core::config`.

**Verify**: `cargo check -p migration-analyze` compiles the new module.

### Step 3: Replace duplicated detection in `boundaries.rs`

Remove the local `detect_migration_folder` in `boundaries.rs`. Update `run` to construct a `ProjectContext` and use it for loading data.

Current entry pattern (verify the exact variable names):
```rust
let project_root = resolve_project_path(&args.path);
let migration_folder = detect_migration_folder(&project_root)?;
let report_dir = migration_folder.join("report");
```

Replace with:
```rust
use crate::commands::context::ProjectContext;

let ctx = ProjectContext::load(resolve_project_path(&args.path))?;
let report_dir = ctx.report_dir.clone();
```

Then replace direct `std::fs::read_to_string(report_dir.join(...))` calls with `ctx.load_json(...)` or `ctx.report_path(...)` where appropriate.

**Verify**: `cargo check -p migration-analyze` succeeds and there are no remaining `fn detect_migration_folder` definitions in `boundaries.rs`.

### Step 4: Replace duplicated detection in `diff.rs`

Repeat Step 3 for `diff.rs`. Remove its local `detect_migration_folder` and use `ProjectContext`.

**Verify**: `cargo check -p migration-analyze` succeeds and `diff.rs` no longer defines `detect_migration_folder`.

### Step 5: Replace duplicated detection in `serve.rs`

Repeat Step 3 for `serve.rs`. Remove its local `detect_migration_folder` and use `ProjectContext`.

The `AppState` currently stores `report_dir: PathBuf`. Update it to store `context: ProjectContext` (or `Arc<ProjectContext>`) so handlers can use cached loading. Since `ProjectContext` contains `Mutex`, it must be wrapped in `Arc` to be shared across handlers.

Current:
```rust
let state = web::routes::AppState { report_dir };
let state = Arc::new(state);
```

Change to something like:
```rust
let ctx = ProjectContext::load(project_root)?;
let state = web::routes::AppState { ctx: Arc::new(ctx) };
let state = Arc::new(state);
```

Update `web::routes::AppState` in the `web` module to match. You may need to add a field and adjust all handlers that read from `state.report_dir`. If the change is large, stop and report.

**Verify**: `cargo check -p migration-analyze` succeeds and `serve.rs` no longer defines `detect_migration_folder`.

### Step 6: Full verification

Run:

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

**Verify**: All three exit 0.

## Test plan

- Add an integration test in `migration-analyze/tests/` (or extend existing CLI tests) that runs `migration-analyze boundaries`, `diff`, and `serve` against a temporary project. If such tests already exist, update them to still pass after the refactor.
- If no integration tests exist, create a minimal test under `migration-analyze/tests/context.rs` that:
  1. Creates a temp dir with a fake source repo.
  2. Runs `analyze`.
  3. Loads `ProjectContext` and asserts `report_dir` exists.
  4. Reads `project.json` and `index.json` through the context.
- Use `tempfile` and `assert_cmd` patterns from existing tests if present.

## Done criteria

- [ ] `ProjectContext` is defined in a single shared module.
- [ ] `boundaries.rs`, `diff.rs`, and `serve.rs` no longer contain `fn detect_migration_folder`.
- [ ] All three commands load report data through `ProjectContext`.
- [ ] `cargo clippy --workspace -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `plans/README.md` status row for plan 003 updated to DONE.

## STOP conditions

Stop and report if:
- The `web::routes::AppState` refactor in `serve.rs` requires touching more than `serve.rs` and the `web::routes` module (indicates scope creep).
- A command's error message contract is intentionally different and must be preserved; do not silently unify messages without confirmation.

## Maintenance notes

- Future output-path renames (plan 006) should only require updating `ProjectContext`, not every command.
- Adding caching (plan 005) can extend `ProjectContext` with analysis-cache paths.
- Reviewers should reject any new command that re-implements migration-folder detection.
