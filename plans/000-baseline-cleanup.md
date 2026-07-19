# Plan 000: Establish CI baseline by fixing pre-existing clippy, fmt, and test fixture issues

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- .`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P0
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

Before CI gates can be enforced, the codebase must already pass `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace`. Currently all three fail: formatting is out of date, clippy reports 34 warnings-as-errors, and the end-to-end tests depend on a missing external fixture (`C:\Users\16017\Documents\AI\calc-test`). This plan cleans up those pre-existing issues so subsequent plans have a green baseline.

## Current state

- `cargo fmt --check` fails because many files are not formatted.
- `cargo clippy --workspace -- -D warnings` fails with 34 errors including:
  - `collapsible_if` in `core/src/deps/typescript.rs`, `core/src/discovery.rs`, `core/src/references/rust.rs`, `core/src/references/typescript.rs`, `core/src/symbols/rust.rs`, `core/src/symbols/typescript.rs`
  - `needless_borrow` in `core/src/graph.rs`, `core/src/references/rust.rs`, `core/src/references/typescript.rs`
  - `single_element_loop` in `core/src/references/rust.rs`
  - `type_complexity` in `core/src/references/rust.rs`, `core/src/references/typescript.rs`
  - `manual_split_once` in `core/src/scores.rs`
  - `single_match` in `core/src/symbols/typescript.rs`
- `cargo test --workspace` fails because `migration-analyze/tests/common/mod.rs` hardcodes fixture path `C:\Users\16017\Documents\AI\calc-test` which does not exist.

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Format    | `cargo fmt`                          | exit 0              |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0              |
| Test      | `cargo test --workspace`             | exit 0              |
| Build     | `cargo build --workspace`            | exit 0              |

## Scope

**In scope**:
- All Rust source files that trigger `cargo fmt` or `cargo clippy` warnings.
- `migration-analyze/tests/common/mod.rs` and related E2E tests.
- Optional: create a self-contained fixture under `migration-analyze/tests/fixtures/test-repo/` so tests do not depend on an external directory.

**Out of scope**:
- Adding CI workflow files (plan 001).
- Any functional changes beyond what is required to satisfy fmt/clippy/test.

## Steps

### Step 1: Run `cargo fmt`

```bash
cargo fmt
```

This will reformat all Rust files. Review the diff to ensure only formatting changed.

**Verify**: `cargo fmt --check` exits 0.

### Step 2: Fix clippy warnings

Run clippy and fix each warning. The 34 errors fall into these categories:

1. **Collapsible `if` statements**: Replace nested `if let`/`if` with let-chains where possible. Example:
   ```rust
   // before
   if let Some(name) = ... {
       if !name.is_empty() && pkg_path.is_empty() {
           return name.to_string();
       }
   }
   // after
   if let Some(name) = ...
       && !name.is_empty() && pkg_path.is_empty()
   {
       return name.to_string();
   }
   ```

2. **Needless borrow**: Remove `&` where the compiler auto-dereferences. Example:
   ```rust
   normalize_path_components(&relative) // -> normalize_path_components(relative)
   ```

3. **Single-element loops**: Replace `for ext in &["rs"] { ... }` with a direct block using `let ext = "rs";`.

4. **Type complexity**: Extract type aliases for `HashMap<String, HashMap<String, (String, String)>>` and the `Vec<(String, anyhow::Result<HashMap<String, (String, String)>>)>` in `references/rust.rs` and `references/typescript.rs`. Example:
   ```rust
   type FileReferences = HashMap<String, (String, String)>;
   type ModuleReferences = HashMap<String, FileReferences>;
   ```

5. **Manual split once**: Replace `rsplitn(2, ':').nth(1)` with `rsplit_once(':').map(|x| x.0)`.

6. **Single match**: Replace `match member { TSSignature::TSPropertySignature(prop) => { ... } _ => {} }` with `if let TSSignature::TSPropertySignature(prop) = member { ... }`.

Fix all warnings one file at a time, re-running clippy after each file to confirm progress.

**Verify**: `cargo clippy --workspace -- -D warnings` exits 0.

### Step 3: Fix the missing test fixture

Option A (recommended): Create a self-contained fixture inside the repo.

1. Create `migration-analyze/tests/fixtures/test-repo/` with a minimal TypeScript project:
   - `package.json` with `{ "name": "test-repo", "version": "1.0.0" }`
   - `tsconfig.json` with basic compiler options
   - At least 3 TypeScript files under `src/` with imports between them (e.g., `src/index.ts`, `src/utils.ts`, `src/math.ts`) so the analyzer finds references and symbols.
2. Update `migration-analyze/tests/common/mod.rs`:
   - Change `FIXTURE_DIR` to `r"C:\Users\16017\Documents\AI\migration-assessor\migration-analyze\tests\fixtures\test-repo"` (or use a relative path computed from `CARGO_MANIFEST_DIR`).
   - Ensure `setup_project` copies from the new fixture.
3. Run `cargo test --workspace` and fix any remaining test assumptions.

Option B: If creating a fixture is not feasible, modify the tests to generate a minimal project programmatically in `setup_project` instead of copying an external fixture.

**Verify**: `cargo test --workspace` exits 0.

### Step 4: Final verification

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
```

**Verify**: All four commands exit 0.

## Test plan

- No new tests are required for the clippy/fmt fixes.
- For the fixture change, ensure the existing E2E tests (`e2e_analyze.rs`, `e2e_diff.rs`, `e2e_init.rs`, `e2e_serve.rs`) all pass.
- If any test asserts old report paths that will change in plan 006, leave those assertions as-is for now; they will be updated in plan 006.

## Done criteria

- [ ] `cargo fmt --check` exits 0.
- [ ] `cargo clippy --workspace -- -D warnings` exits 0.
- [ ] `cargo test --workspace` exits 0.
- [ ] `cargo build --workspace` exits 0.
- [ ] `plans/README.md` status row for plan 000 updated to DONE.

## STOP conditions

Stop and report if:
- A clippy warning cannot be fixed without changing behavior.
- The fixture change causes tests outside `migration-analyze/tests/` to fail.
- Any step fails twice after reasonable fixes.

## Maintenance notes

- Once this plan lands, plan 001 can safely add CI gates.
- Future code should be written clippy-clean from the start; running `cargo clippy` locally before push is now cheap.
