# Plan 001: Add CI/CD workflows and release gates

> **Executor instructions**: Follow this plan step by step. Run every verification command and confirm the expected result before moving to the next step. If anything in the "STOP conditions" section occurs, stop and report — do not improvise. When done, update the status row for this plan in `plans/README.md`.
>
> **Drift check (run first)**: `git diff --stat 80a884a..HEAD -- .github/workflows/ migration-analyze/src/ Cargo.toml core/src/`
> If any in-scope file changed since this plan was written, compare the "Current state" excerpts against the live code before proceeding; on a mismatch, treat it as a STOP condition.

## Status

- **Priority**: P0
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: dx
- **Planned at**: commit `80a884a`, 2026-07-19

## Why this matters

The project currently has no automated checks on pull requests and the release workflow builds binaries without running tests. This means a broken test or clippy warning can ship in a tagged release. Adding CI gates catches regressions before merge and ensures every release artifact passes the test suite.

## Current state

- `.github/workflows/release.yml` exists and triggers on `v*` tags. It checks out the repo, installs Rust, adds a target, runs `cargo build --release --target ...`, packages the binary, and publishes a GitHub release. It does **not** run `cargo test` or `cargo clippy`.
- The workspace root `Cargo.toml` defines members `core` and `migration-analyze` and uses edition 2024.
- There are existing dev-dependencies in `migration-analyze/Cargo.toml`: `assert_cmd`, `predicates`, `reqwest`, `tempfile`, indicating end-to-end tests are intended.
- Verified commands on this repo:
  - `cargo build --workspace`
  - `cargo test --workspace`
  - `cargo clippy --workspace -- -D warnings`
  - `cargo fmt --check`

## Commands you will need

| Purpose   | Command                              | Expected on success |
|-----------|--------------------------------------|---------------------|
| Build     | `cargo build --workspace`            | exit 0              |
| Test      | `cargo test --workspace`             | exit 0, tests pass  |
| Lint      | `cargo clippy --workspace -- -D warnings` | exit 0, no warnings |
| Format    | `cargo fmt --check`                  | exit 0              |

## Scope

**In scope**:
- `.github/workflows/ci.yml` (create)
- `.github/workflows/release.yml` (modify)
- Baseline cleanup required for CI to pass: format all Rust files, fix clippy warnings, make the test suite pass.

**Out of scope**:
- `benchmark.yml` or `nightly.yml`.

## Steps

### Step 0: Baseline cleanup

The repository currently does not pass `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, or `cargo test --workspace`. CI gates cannot be added until the baseline is green. Fix the baseline first.

1. Run `cargo fmt` to apply formatting.
2. Run `cargo clippy --workspace -- -D warnings` and fix every warning. Common expected warning categories include:
   - `clippy::collapsible_if`
   - `clippy::needless_borrow`
   - `clippy::single_element_loop`
   - `clippy::type_complexity`
   - `clippy::manual_split_once`
   - `clippy::single_match`
   Fix them mechanically; do not refactor logic beyond what clippy demands.
3. Run `cargo test --workspace`. The test suite fails because the fixture `C:\Users\16017\Documents\AI\calc-test` does not exist. Investigate `migration-analyze/tests/common/mod.rs` and `migration-analyze/tests/e2e_analyze.rs`. Make the tests self-contained: either create the fixture inside the test setup (preferred) or make the test generate a minimal TypeScript/Rust project in a temp directory and analyze it. Do not hard-code an absolute path to a fixture outside the repo.
4. Re-run `cargo fmt`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace` until all three pass.

**Verify**: `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, and `cargo test --workspace` all exit 0.

### Step 1: Create `ci.yml`

Create `.github/workflows/ci.yml` with the following content. It must trigger on pull requests and pushes to `main`. It runs format, clippy, test, and build on `ubuntu-latest`.

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - name: Format check
        run: cargo fmt --check
      - name: Clippy
        run: cargo clippy --workspace -- -D warnings
      - name: Test
        run: cargo test --workspace
      - name: Build
        run: cargo build --workspace
```

**Verify**: `git add .github/workflows/ci.yml && git status` shows only the new file staged.

### Step 2: Add test and lint gates to `release.yml`

Modify `.github/workflows/release.yml` so the `build` job runs `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` before the cross-compilation step.

Current relevant section (lines 29-32):
```yaml
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: rustup target add ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }} -p migration-analyze
```

Change it to:
```yaml
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo clippy --workspace -- -D warnings
      - run: cargo test --workspace
      - run: rustup target add ${{ matrix.target }}
      - run: cargo build --release --target ${{ matrix.target }} -p migration-analyze
```

**Verify**: Open `.github/workflows/release.yml` and confirm the build job now contains `cargo clippy`, `cargo test`, `rustup target add`, and `cargo build` in that order.

### Step 3: Validate workflows locally

Run the same commands the CI will run:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
```

**Verify**: All four commands exit 0. If any fail, stop and report the failure.

## Test plan

- No new Rust tests are required; this plan changes CI configuration.
- Verify that the YAML is syntactically valid by checking it with a parser or by inspecting the file visually.

## Done criteria

- [ ] `.github/workflows/ci.yml` exists and matches the content in Step 1.
- [ ] `.github/workflows/release.yml` runs `cargo clippy --workspace -- -D warnings` and `cargo test --workspace` before `cargo build --release`.
- [ ] `cargo fmt --check`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, and `cargo build --workspace` all pass locally.
- [ ] `plans/README.md` status row for plan 001 updated to DONE.

## STOP conditions

Stop and report if:
- A clippy warning cannot be fixed mechanically and requires a design decision.
- The existing `.github/workflows/release.yml` structure differs so much from the excerpt that the edit cannot be applied cleanly.

## Maintenance notes

- Once this plan lands, every future PR must keep `cargo fmt`, `cargo clippy`, and `cargo test` green. Reviewers should block PRs that bypass CI.
- If cross-compilation targets need additional dependencies later, add them after the test/lint steps.
