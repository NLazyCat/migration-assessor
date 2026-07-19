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
- Compatibility knowledge lives in `core/src/compatibility_data.toml` (edit data, not just code).
- Config is `migration.toml` in the user's project dir; `analyze`/`diff`/`boundaries`/`summary`/`check-updates` are the CLI subcommands (see `main.rs`).
- No README yet — `main.rs`'s `print_usage_guide()` is the closest thing to user docs.
