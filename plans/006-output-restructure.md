# Plan 006: Restructure output directories and add manifest.json

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- migration-analyze/src/commands/analyze.rs migration-analyze/src/web/routes.rs migration-analyze/src/web/templates.rs migration-analyze/src/commands/boundaries.rs migration-analyze/src/commands/diff.rs core/src/output.rs core/src/report.rs`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: L
- **Risk**: HIGH
- **Depends on**: 003, 004
- **Category**: tech-debt
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

The current output layout mixes naming styles (`external-deps`, `internal-deps`, `api-contracts`, `interface-boundaries.json`, `diffs/diff-YYYY-MM-DD.json`) and lacks a central manifest. This makes the serve API and HTML report brittle: every consumer hard-codes paths. A unified layout with `manifest.json` as the entry point lets consumers discover artifacts and allows the output format to evolve without breaking callers.

## Current state

- `analyze.rs:144-224` writes:
  - `project.json`, `errors.json`, `index.json`, `scores.json`
  - `external-deps/resolved.json`, `external-deps/compatibility.json`
  - `internal-deps/dag.json`, `internal-deps/cycles.json`
  - `symbols/by-dir/*.index.json`, `api-contracts/by-dir/*.api.json`
  - `references/forward.json`, `references/reverse.json`, `references/by-dir/*.forward.json`, `references/by-dir/*.reverse.json`
  - `index.html`
- `boundaries.rs` outputs `interface-boundaries.json` in the report root.
- `diff.rs` outputs `diffs/diff-YYYY-MM-DD.json`.
- `web/routes.rs` hard-codes all of these paths.

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |
| E2E       | `cargo run -p migration-analyze -- analyze <fixture>` | report layout matches new structure |

## Scope

**In scope**:
- `migration-analyze/src/commands/analyze.rs`
- `migration-analyze/src/commands/boundaries.rs`
- `migration-analyze/src/commands/diff.rs`
- `migration-analyze/src/web/routes.rs`
- `migration-analyze/src/web/templates.rs`
- `core/src/report.rs`
- `core/src/output.rs`

**Out of scope**:
- Changing JSON schemas inside the files (plan 007 enriches graph data).
- Adding NDJSON serialization (plan 009).

## Steps

### Step 1: Define the new layout constants

Create a new module `core/src/output_paths.rs` (or add to `core/src/output.rs`) that centralizes all output paths:

```rust
pub const MANIFEST: &str = "manifest.json";
pub const PROJECT: &str = "project.json";
pub const OVERVIEW: &str = "overview.json";
pub const SCORES: &str = "scores.json";
pub const ERRORS: &str = "errors.json";
pub const INDEX_HTML: &str = "index.html";

pub mod external {
    pub const PACKAGES: &str = "external/packages.json";
    pub const COMPATIBILITY: &str = "external/compatibility.json";
}

pub mod graph {
    pub const NODES: &str = "graph/nodes.json";
    pub const EDGES: &str = "graph/edges.json";
    pub const CYCLES: &str = "graph/cycles.json";
}

pub mod symbols {
    pub fn for_module(module: &str) -> String {
        format!("symbols/{}/symbols.json", module)
    }
    pub fn api_for_module(module: &str) -> String {
        format!("symbols/{}/api.json", module)
    }
}

pub mod references {
    pub fn forward_for(file: &str) -> String {
        format!("references/forward/{}.json", file)
    }
    pub fn reverse_for(file: &str) -> String {
        format!("references/reverse/{}.json", file)
    }
}

pub mod boundaries {
    pub const LAYERS: &str = "boundaries/layers.json";
    pub const UNCUT_SURFACES: &str = "boundaries/uncut-surfaces.json";
}

pub mod diffs {
    pub fn dated(name: &str) -> String {
        format!("diffs/{}", name)
    }
    pub const LATEST: &str = "diffs/latest.json";
}
```

**Verify**: `cargo check -p migration-core` succeeds.

### Step 2: Update `analyze.rs` to write the new layout and manifest

Change all `output.write_json` calls in `analyze.rs` to use the new paths. Key changes:
- `external-deps/resolved.json` → `external/packages.json`
- `external-deps/compatibility.json` → `external/compatibility.json`
- `internal-deps/dag.json` → split into `graph/nodes.json` and `graph/edges.json`
- `internal-deps/cycles.json` → `graph/cycles.json`
- `symbols/by-dir/<module>.index.json` → `symbols/<module>/symbols.json`
- `api-contracts/by-dir/<module>.api.json` → `symbols/<module>/api.json`
- `references/by-dir/<file>.forward.json` → `references/forward/<file>.json`
- `references/by-dir/<file>.reverse.json` → `references/reverse/<file>.json`

After all writes, write `manifest.json`:

```rust
let manifest = json!({
    "$schema": "https://migration-analyze.dev/schema/v1/manifest.json",
    "schemaVersion": "1.0.0",
    "generatedAt": chrono.to_rfc3339(),
    "toolVersion": env!("CARGO_PKG_VERSION"),
    "files": {
        "project": PROJECT,
        "overview": OVERVIEW,
        "scores": SCORES,
        "errors": ERRORS,
        "externalPackages": external::PACKAGES,
        "externalCompatibility": external::COMPATIBILITY,
        "graphNodes": graph::NODES,
        "graphEdges": graph::EDGES,
        "graphCycles": graph::CYCLES,
    }
});
output.write_json(&report_dir, output_paths::MANIFEST, &manifest)?;
```

**Verify**: `cargo check -p migration-analyze` succeeds.

### Step 3: Split `dag.json` into nodes/edges

In `analyze.rs`, before writing graph output, split `dependency_graph` into nodes and edges. If `DependencyGraph` already has `nodes` and `edges` fields, write them separately:

```rust
output.write_json(&report_dir, graph::NODES, &dependency_graph.nodes)?;
output.write_json(&report_dir, graph::EDGES, &dependency_graph.edges)?;
output.write_json(&report_dir, graph::CYCLES, &cycle_detection)?;
```

Update `ProjectContext::dag` to load both files and merge them into the old `{ nodes, edges }` shape for backward compatibility, OR update all consumers to read nodes/edges separately.

**Verify**: `cargo check --workspace` succeeds.

### Step 4: Update `boundaries.rs` output paths

Change `boundaries.rs` to write:
- `boundaries/layers.json`
- `boundaries/uncut-surfaces.json`

Update any internal reads from `interface-boundaries.json` to read the new files.

**Verify**: `cargo check -p migration-analyze` succeeds.

### Step 5: Update `diff.rs` output paths

Change `diff.rs` to write into `diffs/diff-YYYY-MM-DD.json` (same filename, new directory) and also write/update `diffs/latest.json` as a copy of the latest diff.

**Verify**: `cargo check -p migration-analyze` succeeds.

### Step 6: Update serve API routes

Update `web/routes.rs` to read from the new paths. Replace every `read_json(&state.report_dir, "old/path")` with the corresponding new path. Update `AppState` and `ProjectContext` (from plan 003) to expose helper methods for the new layout.

**Verify**: `cargo check -p migration-analyze` succeeds.

### Step 7: Update HTML report and templates

Update `core/src/report.rs` and `migration-analyze/src/web/templates.rs` to load `graph/nodes.json` and `graph/edges.json` instead of `internal-deps/dag.json`. Update any other hard-coded paths.

**Verify**: `cargo check --workspace` succeeds.

### Step 8: Full verification

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Then run `analyze` on a fixture and list the report directory to confirm the new layout.

## Test plan

- Update integration tests to assert the new file paths exist after `analyze`.
- Add a test that parses `manifest.json` and asserts all listed files exist.
- Add a test that `index.html` is still generated.

## Done criteria

- [ ] New output layout matches the structure defined in Step 1.
- [ ] `manifest.json` is generated with schema version, generatedAt, toolVersion, and file map.
- [ ] All serve routes read from the new paths.
- [ ] `boundaries` and `diff` commands write to the new paths.
- [ ] `cargo clippy --workspace -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `plans/README.md` status row for plan 006 updated to DONE.

## STOP conditions

Stop and report if:
- The `DependencyGraph` type cannot be easily split into nodes/edges for serialization.
- The web UI or templates are too tightly coupled to old paths and require a larger refactor than this plan allows.
- A consumer outside this repo depends on the old paths and cannot be updated.

## Maintenance notes

- All future output-path changes must go through `core/src/output_paths.rs`.
- The `manifest.json` schema version should be bumped whenever the file map changes.
- Reviewers should verify that both the CLI and web UI can read the new layout end-to-end.
