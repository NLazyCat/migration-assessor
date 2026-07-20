# migration-assessor

Cargo workspace (`rust edition 2024`, resolver `2`). Two workspace crates — `core` (lib) and `migration-analyze` (CLI binary) — plus sidecar fixture projects `ts-ecommerce` and `rust-ecommerce` (not workspace members). TypeScript side uses pnpm + locally-linked `eve` package.

## Commands

```powershell
cargo build                  # build everything (workspace)
cargo test --workspace       # all unit + e2e tests (~250 unit, 5 e2e)
cargo test -p migration-core # unit tests only (faster)
cargo test -p migration-analyze  # e2e tests only
cargo clippy --workspace -- -D warnings  # CI release gate
```

E2e tests (`migration-analyze/tests/`) auto-build the binary via `cargo build --message-format=json`.

## Architecture

- **`core/`** — library crate. All analysis logic: symbol extraction (oxc_parser for TS, syn for Rust), dependency graph, reference tracking, compatibility matrix, migration scores, diff analysis, and the `align` module (real-time target-project symbol matching).
- **`migration-analyze/`** — CLI binary using `clap`. 7 subcommands: `init`, `analyze`, `diff`, `boundaries`, `summary`, `check-updates`. Entrypoint: `migration-analyze/src/main.rs`.
- **`e2e-full/`** — a real end-to-end migration project (ts-ecommerce → rust-ecommerce) used for manual testing.
- **`plans/`** — implementation plans executed by agents. Status tracked in `plans/README.md`.

## Workflow

```
migration-analyze init my-project   # scaffold
cd my-project
# edit migration.toml: set source, source_lang, target_language
migration-analyze analyze           # full analysis → {repo}-migration/report/
migration-analyze summary           # terminal summary
migration-analyze diff --auto       # incremental diff vs latest tag
migration-analyze boundaries        # interface boundary report
migration-analyze check-updates     # check source for newer commits
```

## Config (`migration.toml`)

Source language auto-detected from `package.json` (TS) or `Cargo.toml` (Rust). If both exist, require explicit `--source-lang` or `migration.toml` config. Valid languages: `typescript`, `rust`. Target defaults to `rust`.

`[project.target]` is optional — when set, `diff` performs real-time target-project symbol alignment (no DB needed). `[project.ignore]` and `[project.exclude]` control file globs. `[skip.framework] = true` excludes known framework boilerplate (vue, svelte, react, next, shadcn, etc.).

## Key quirks

- **`resolve_project_path`** avoids `std::fs::canonicalize` because Windows produces `\\?\` extended-length paths that break TOML and path operations. Uses `util::normalize_path` instead. Supports `~` for home dir.
- **Build artifacts**: `core/build.rs` merges all `.toml` files from `compatibility_data/*/` and `naming_data/*/` into single files embedded at compile time via `include_str!(concat!(env!("OUT_DIR"), "/..."))`.
- **File discovery** auto-skips `node_modules`, `target`, `.git`, `dist`, `build`, and `*-migration` directories. Skips non-source extensions.
- **Scoring**: 5 weighted factors — in_degree (30%), complexity (25%), external_compatibility (20%), cycle_count (15%), has_tests (10%). Effort labels: trivial ≥70, moderate ≥50, heavy ≥30, rewrite <30.
- **Diff report output**: `{repo}-migration/diffs/` with dated JSON, `latest.json`, and `affected-files.json`.
- **Report structure**: `manifest.json`, `project.json`, `overview.json`, `scores.json`, `errors.json`, `graph/`, `external/`, `references/` (per-file shards), `index.html`.

## Test fixtures

- `ts-ecommerce/` — TypeScript ecommerce project (models/user, models/product, services/cart, services/order, utils/format, utils/validation). Used as source in e2e-full.
- `rust-ecommerce/` — Rust equivalent (same module layout). Used as target in e2e-full. Edition 2021 (not workspace member).
- `migration-analyze/tests/common/` — builds a minimal TS fixture + migration.toml for automated e2e tests.

## CI / Release

- Both workflows trigger on `v*` tags.
- CI: `cargo test --workspace` + `cargo build --workspace` (ubuntu).
- Release: clippy (`-D warnings`), tests, then cross-platform release builds (linux/mac/windows) of `-p migration-analyze` only. Artifacts named `migration-analyze-{target}.{tar.gz|zip}`.
