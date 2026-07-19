use std::path::PathBuf;
use tempfile::TempDir;

/// Sets up a temporary project directory containing a self-contained
/// TypeScript fixture and a minimal migration.toml.
///
/// Returns a TempDir containing:
///   - migration.toml (minimal config)
///   - test/ (source TypeScript repo)
///
/// The caller can then run `migration-analyze` commands inside this directory.
#[allow(dead_code)]
pub fn setup_project() -> (TempDir, PathBuf) {
    let tmp_dir = TempDir::new().expect("failed to create temp dir");
    let project_root = tmp_dir.path().to_path_buf();
    let source_dir = project_root.join("test");
    let src_dir = source_dir.join("src");
    std::fs::create_dir_all(&src_dir).expect("failed to create src dir");

    // Package metadata so the analyzer detects a TypeScript project.
    std::fs::write(
        source_dir.join("package.json"),
        r#"{
  "name": "calc-test",
  "version": "1.0.0",
  "main": "src/index.ts"
}
"#,
    )
    .expect("failed to write package.json");

    std::fs::write(
        source_dir.join("tsconfig.json"),
        r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "moduleResolution": "node",
    "strict": true
  },
  "include": ["src/**/*"]
}
"#,
    )
    .expect("failed to write tsconfig.json");

    std::fs::write(
        src_dir.join("types.ts"),
        r#"export interface CalcOptions {
  precision: number;
}

export type Operation = 'add' | 'subtract' | 'multiply' | 'divide';
"#,
    )
    .expect("failed to write types.ts");

    std::fs::write(
        src_dir.join("utils.ts"),
        r#"import { CalcOptions, Operation } from './types';

export function calculate(
  a: number,
  b: number,
  operation: Operation,
  options?: CalcOptions,
): number {
  const result = performOperation(a, b, operation);
  return formatResult(result, options);
}

function performOperation(a: number, b: number, operation: Operation): number {
  switch (operation) {
    case 'add':
      return a + b;
    case 'subtract':
      return a - b;
    case 'multiply':
      return a * b;
    case 'divide':
      return a / b;
  }
}

function formatResult(value: number, options?: CalcOptions): number {
  if (!options) {
    return value;
  }
  const factor = 10 ** options.precision;
  return Math.round(value * factor) / factor;
}
"#,
    )
    .expect("failed to write utils.ts");

    std::fs::write(
        src_dir.join("index.ts"),
        r#"import { CalcOptions, Operation } from './types';
import { calculate } from './utils';

export { calculate, CalcOptions, Operation };

export function runCalc(
  a: number,
  b: number,
  operation: Operation,
  options?: CalcOptions,
): number {
  return calculate(a, b, operation, options);
}

// This line intentionally left blank for diff testing
"#,
    )
    .expect("failed to write index.ts");

    // Create a minimal migration.toml
    let config = r#"# Migration Assessor Configuration
[project]
source = "test"
target_language = "rust"
source_language = "typescript"

[skip]
framework = false
"#;
    std::fs::write(project_root.join("migration.toml"), config)
        .expect("failed to write migration.toml");

    (tmp_dir, project_root)
}

/// Returns the path to the compiled `migration-analyze` binary.
/// Uses `cargo` to find the binary location.
pub fn binary_path() -> PathBuf {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();

    let output = std::process::Command::new("cargo")
        .args(["build", "-p", "migration-analyze", "--message-format=json"])
        .current_dir(&workspace_root)
        .output()
        .ok();

    if let Some(output) = output {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Ok(serde_json::Value::Object(obj)) =
                serde_json::from_str::<serde_json::Value>(line)
            {
                if obj.get("reason").and_then(|v| v.as_str()) == Some("compiler-artifact") {
                    if let Some(executable) = obj.get("executable").and_then(|v| v.as_str()) {
                        if executable.contains("migration-analyze") {
                            return PathBuf::from(executable);
                        }
                    }
                }
            }
        }
    }

    // Fallback: use the known debug build path
    let fallback = workspace_root
        .join("target")
        .join("debug")
        .join("migration-analyze.exe");
    assert!(fallback.exists(), "Binary not found at fallback path");
    fallback
}
