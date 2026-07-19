# Plan 009: Add NDJSON optional output format

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- migration-analyze/src/commands/analyze.rs core/src/output.rs migration-analyze/src/main.rs`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: 006
- **Category**: dx
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

Large symbol lists and reference shards are currently written as pretty-printed JSON arrays. NDJSON (one JSON object per line) allows streaming consumption, works well with `jq` and line-oriented shell tools, and is easier to process incrementally in CI pipelines. Making it an optional format preserves the default human-readable JSON output.

## Current state

- `core/src/output.rs:15-27`:
  ```rust
  pub fn write_json<T: serde::Serialize>(
      &self,
      output_dir: &Path,
      relative_path: &str,
      data: &T,
  ) -> anyhow::Result<()> {
      let path = output_dir.join(relative_path);
      if let Some(parent) = path.parent() {
          fs::create_dir_all(parent)?;
      }
      let content = serde_json::to_string_pretty(data)?;
      fs::write(path, content)?;
      Ok(())
  }
  ```
- `analyze.rs` calls `output.write_json` for every artifact.
- There is no output-format option yet.

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |
| E2E       | `cargo run -p migration-analyze -- analyze --format ndjson <fixture>` | produces `.ndjson` files for arrays |

## Scope

**In scope**:
- `core/src/output.rs`
- `migration-analyze/src/commands/analyze.rs`
- `migration-analyze/src/main.rs` (if CLI args are defined there)

**Out of scope**:
- Changing the default JSON format.
- Updating serve API or HTML report to consume NDJSON.

## Steps

### Step 1: Add output format enum

In `core/src/output.rs`, add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Json,
    Ndjson,
}
```

Add CLI argument support in `AnalyzeArgs`:

```rust
#[arg(long, default_value = "json")]
pub format: String,
```

Or use a `clap::ValueEnum` if preferred.

**Verify**: `cargo check -p migration-core` succeeds.

### Step 2: Extend `OutputWriter` with format support

Add a `format` field to `OutputWriter` and a constructor:

```rust
pub struct OutputWriter {
    format: OutputFormat,
}

impl OutputWriter {
    pub fn init(output_dir: &Path, format: OutputFormat) -> anyhow::Result<Self> {
        fs::create_dir_all(output_dir)?;
        Ok(Self { format })
    }

    pub fn write<T: serde::Serialize>(
        &self,
        output_dir: &Path,
        relative_path: &str,
        data: &T,
    ) -> anyhow::Result<()> {
        let path = output_dir.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = if self.format == OutputFormat::Ndjson {
            to_ndjson(data)?
        } else {
            serde_json::to_string_pretty(data)?
        };
        fs::write(path, content)?;
        Ok(())
    }
}

fn to_ndjson<T: serde::Serialize>(data: &T) -> anyhow::Result<String> {
    let value = serde_json::to_value(data)?;
    let array = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("NDJSON output only supports array data"))?;
    let mut lines = String::new();
    for item in array {
        lines.push_str(&serde_json::to_string(item)?);
        lines.push('\n');
    }
    Ok(lines)
}
```

Adjust file extensions: when `format == Ndjson`, replace `.json` with `.ndjson` in the relative path. You can do this inside `write`:

```rust
let path = if self.format == OutputFormat::Ndjson {
    output_dir.join(relative_path.trim_end_matches(".json")).with_extension("ndjson")
} else {
    output_dir.join(relative_path)
};
```

**Verify**: `cargo check -p migration-core` succeeds.

### Step 3: Wire format through analyze

In `analyze.rs`, parse the CLI format string into `OutputFormat` and pass it to `OutputWriter::init`. Update all `output.write_json` calls to `output.write` (or keep the method name if unchanged).

**Verify**: `cargo check -p migration-analyze` succeeds.

### Step 4: Decide which artifacts become NDJSON

Only array-shaped artifacts should become NDJSON:
- `scores.json`
- `external/packages.json` (if it contains an array)
- `graph/nodes.json`, `graph/edges.json`
- per-file symbols/references

Object-shaped artifacts like `project.json`, `manifest.json`, and `graph/cycles.json` should remain regular JSON even in NDJSON mode. You can handle this by keeping `.json` extension for non-array data or by writing them as single-line NDJSON (one object). The plan recommends: non-array data stays JSON; array data becomes `.ndjson`.

**Verify**: A fixture analyzed with `--format ndjson` produces both `.ndjson` and `.json` files appropriately.

### Step 5: Full verification

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
cargo test --workspace
```

Run manual tests:

```bash
# Default JSON
cargo run -p migration-analyze -- analyze <fixture>
# NDJSON mode
cargo run -p migration-analyze -- analyze --format ndjson <fixture>
```

**Verify**: Both modes produce valid output and the default mode is unchanged.

## Test plan

- Add unit tests in `core/src/output.rs`:
  - `to_ndjson` converts a `Vec<Value>` to newline-separated JSON objects.
  - `to_ndjson` returns an error for non-array data (or the writer falls back to JSON).
- Add an integration test that runs `analyze --format ndjson` and verifies at least one `.ndjson` file exists and each line is valid JSON.

## Done criteria

- [ ] `OutputFormat` enum exists with `Json` and `Ndjson` variants.
- [ ] `AnalyzeArgs` accepts `--format json|ndjson`.
- [ ] Array-shaped artifacts are written as `.ndjson` in NDJSON mode.
- [ ] Object-shaped artifacts remain `.json` in NDJSON mode.
- [ ] Default mode (`--format json`) is unchanged.
- [ ] `cargo clippy --workspace -- -D warnings` passes.
- [ ] `cargo test --workspace` passes.
- [ ] `plans/README.md` status row for plan 009 updated to DONE.

## STOP conditions

Stop and report if:
- The output data structures cannot be easily classified as array vs object at write time.
- Changing file extensions breaks consumers that expect `.json` for every artifact.

## Maintenance notes

- If serve API or HTML report is updated to read NDJSON, add a content-negotiation path.
- New array-shaped artifacts should automatically become NDJSON-capable if they use the shared writer.
