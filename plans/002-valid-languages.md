# Plan 002: Align VALID_LANGUAGES with supported SourceLanguage variants

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- core/src/config.rs core/src/project.rs`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

`Config::validate` accepts `python`, `go`, `java`, and `kotlin` as valid languages, but `Project::detect` only handles TypeScript and Rust. A user who configures `source_lang = "python"` will pass validation and then hit an unhandled runtime path or a misleading auto-detection error. Restricting validation to implemented languages prevents this mismatch.

## Current state

- `core/src/config.rs:191` declares:
  ```rust
  const VALID_LANGUAGES: &[&str] = &["typescript", "rust", "python", "go", "java", "kotlin"];
  ```
- `core/src/project.rs:4-7` defines:
  ```rust
  pub enum SourceLanguage {
      TypeScript,
      Rust,
  }
  ```
- `core/src/project.rs:25-37` maps only `typescript`/`ts` and `rust`/`rs`; any other hint falls through to file-system auto-detection based on `package.json`/`Cargo.toml`.
- `core/src/config.rs:208-225` validates `source_lang` and `target_lang` against `VALID_LANGUAGES`.

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |

## Scope

**In scope**:
- `core/src/config.rs`

**Out of scope**:
- Adding support for Python/Go/Java/Kotlin (a much larger feature).
- Changing `SourceLanguage` enum or `Project::detect`.
- CLI help text changes.

## Steps

### Step 1: Reduce VALID_LANGUAGES to implemented languages

Edit `core/src/config.rs` line 191:

```rust
const VALID_LANGUAGES: &[&str] = &["typescript", "rust"];
```

**Verify**: `grep -n "VALID_LANGUAGES" core/src/config.rs` shows only the updated line and its usages in `validate()`.

### Step 2: Ensure tests still pass

Run the workspace test suite:

```bash
cargo test --workspace
```

**Verify**: Exit 0. If there are tests that assert the old six-language list, they will fail; stop and report if so.

### Step 3: Run clippy and fmt

```bash
cargo fmt
cargo clippy --workspace -- -D warnings
```

**Verify**: Both exit 0.

## Test plan

- If no existing test covers config validation, add a unit test in `core/src/config.rs` (inside a `#[cfg(test)] mod tests` block or extend an existing one):
  - Valid: `source_lang = "typescript"` and `target_lang = "rust"` pass validation.
  - Invalid: `source_lang = "python"` fails with an error mentioning valid values.
- Model the test after any existing unit test in `core/src/config.rs`. If none exists, create a minimal `mod tests` at the bottom of the file.

## Done criteria

- [ ] `core/src/config.rs` contains `const VALID_LANGUAGES: &[&str] = &["typescript", "rust"];`.
- [ ] `cargo test --workspace` passes (including any new validation tests).
- [ ] `cargo clippy --workspace -- -D warnings` passes.
- [ ] `cargo fmt` produces no diff.
- [ ] `plans/README.md` status row for plan 002 updated to DONE.

## STOP conditions

Stop and report if:
- Tests exist that depend on the six-language list and fail after the change.
- The `validate()` function structure differs from the excerpt and the constant cannot be safely narrowed.

## Maintenance notes

- When future work adds a new source language, extend both `SourceLanguage` in `core/src/project.rs` and `VALID_LANGUAGES` in `core/src/config.rs` together. Consider adding a compile-time assertion or a single source of truth to keep them in sync.
