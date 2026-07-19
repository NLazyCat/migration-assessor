# AGENTS.md

Rust workspace (Cargo resolver 2, edition 2024). Two crates:
- `core/` → `migration-core` library: parsers (Rust via `syn`, TS via `oxc`), symbol/reference/deps analysis, scoring, output.
- `migration-analyze/` → `migration-analyze` binary (clap CLI). The only shippable artifact.

## Commands
- `cargo test --workspace` — run all tests.
- `cargo test -p migration-analyze` — e2e tests live in `migration-analyze/tests/` (use `assert_cmd`, need a temp git repo).
- `cargo clippy --workspace -- -D warnings` — CI treats warnings as errors.
- `cargo fmt --check` — CI checks formatting; run `cargo fmt` to fix.
- `cargo build -p migration-analyze` — build just the CLI.

CI gate order (`.github/workflows/ci.yml`): `fmt --check` → `clippy -D warnings` → `test` → `build`.

## Release
- Triggered by `v*` tags. Release builds ONLY `-p migration-analyze` across linux/macos/windows targets and publishes zips/tarballs via GitHub Releases. `core` is not published.

## Gotchas
- `package.json` / `tsconfig.json` reference an `eve` package (`link:../eve/...`) and an `agent/**` dir that do NOT exist in this repo. They are external/link deps; don't assume a JS/TS build runs here. The Rust workspace is the real codebase.
- Path handling: never use `Path::canonicalize()` on Windows inputs — it yields `\\?\` extended paths that break TOML strings and path-prefix logic. `migration-analyze/src/commands/mod.rs` has `resolve_project_path()` / `normalize_path_components()` that avoid it. Reuse those instead of `canonicalize`.
- `git2` is a dependency: `diff`/`check-updates` commands shell out to a real git repo, so e2e tests clone/commit into temp dirs.
- Edition 2024 + `noUncheckedIndexedAccess`-style strictness in core logic; respect `.gitignore` (`/target`, `Cargo.lock`, `/scripts`, `.eve/`).

## Conventions
- Compatibility knowledge lives in `core/src/compatibility_data.toml` (edit data, not just code). There are pre-existing TOML duplicate keys (e.g., `rust->typescript.rand` in both `auth_security.toml` and `utility.toml`) that cause 4 test failures. User said "不要碰兼容性矩阵" (don't touch compatibility matrix).
- Config is `migration.toml` in the user's project dir; `analyze`/`diff`/`boundaries`/`summary`/`check-updates` are the CLI subcommands (see `main.rs`).
- No README yet — `main.rs`'s `print_usage_guide()` is the closest thing to user docs.

## Session Summary (2026-07-19)

### Bugs fixed this session
1. **`context.rs:21`** — `canonicalize()` → `normalize_path()` to avoid `\\?\` Windows extended-length paths that break TOML/path logic.
2. **`analyze.rs` config generation** — was writing `source = <project_root>` instead of actual source repo, and dropped `source_lang`. Fixed both.
3. **`detect_source_repo` priority reorder** — now scans subdirs FIRST, falls back to `config.project.source` only for disambiguation (0 or multiple candidates). Also handles absolute paths in config.
4. **`typescript.rs` extractor** — checks `ret.diagnostics` after parsing, logs parser errors as warnings, returns partial AST instead of empty.
5. **`compatibility.rs`** — 3 clippy cleanups: gated unused `parse_language_pair_key` with `#[cfg(test)]`, removed dead `if { None } else { None }`, replaced `or_else(|| ...)` with `or(...)`.

### Known issues
- `test_compatibility_matrix_loads_rust_to_ts_entries` fails (TOML duplicate keys in `compatibility_data/`). 17/18 pass.
- `diff` has pre-existing limitation: only reports changes for symbols IN the index, new symbols added post-analyze are silently skipped.
- `diff` fetches from remote when local comparison fails (e.g., `--new-version BASE~1` not in local history), no graceful fallback.
