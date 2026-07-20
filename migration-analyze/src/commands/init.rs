use clap::Args;
use std::path::Path;

#[derive(Args)]
pub struct InitArgs {
    /// Name of the project directory to create
    pub name: String,

    /// Path to parent directory (default: current directory)
    #[arg(long, default_value = ".")]
    pub dir: String,
}

pub fn run(args: &InitArgs) -> anyhow::Result<()> {
    let parent = Path::new(&args.dir);
    let project_root = parent.join(&args.name);

    if project_root.exists() {
        anyhow::bail!("Directory '{}' already exists.", project_root.display());
    }

    std::fs::create_dir_all(&project_root)?;

    let config_content = r#"# Migration Assessor Configuration
#
# Fill in the [project] section with your source repository information.
# Then run: migration-analyze analyze

[project]
# Source repository URL (git clone URL) or local path (required)
# Examples:
#   source_repo = "https://github.com/user/my-project.git"
#   source_repo = "../local-project"
source_repo = ""

# Source language to analyze (typescript | javascript | rust)
source_lang = "typescript"

# Target language for migration
target_language = "rust"

# Source directory path (auto-detected if left empty)
# source = ""

# Target project path — enables real-time symbol alignment during diff
# target = "../my-rust-project"

# Git branch to analyze (optional)
# source_branch = ""

# Specific version analyzed — auto-managed by analyze, do not edit manually
# source_version = ""

# Fail on first error instead of collecting all errors
# strict = false

# File glob patterns to ignore during analysis
# ignore = ["**/*.test.ts", "**/*.spec.ts"]

# File glob patterns to exclude from analysis
# exclude = ["src/generated/**"]

[skip]
# Skip framework-level dependencies
framework = true

[output]
# Output directory for analysis report
# directory = "report"

# Split output by directory structure
# split_by_directory = true

[compatibility]
# Custom compatibility overrides file
# overrides_file = ".migration-assessor-compat.toml"

[scoring.weights]
in_degree = 30
complexity = 25
compatibility = 20
cycles = 15
tests = 10

[mapping]
# Override file path mappings between source and target
# override_list = [
#     { from = "src/utils.ts", to = "new/src/utils.rs" },
# ]
"#;

    let config_path = project_root.join("migration.toml");
    std::fs::write(&config_path, config_content)?;

    let gitignore_content = "# Migration artifacts\n*-migration/\n\n# IDE\n.idea/\n.vscode/\n*.swp\n";
    std::fs::write(project_root.join(".gitignore"), gitignore_content)?;

    println!("Created project '{}'", args.name);
    println!();
    println!("  Location: {}", project_root.display());
    println!();
    println!("  Next steps:");
    println!("    1. Edit migration.toml and fill in:");
    println!("       - source_repo (git URL or local path)");
    println!("       - source_lang (source language)");
    println!("       - target_language (migration target)");
    println!("    2. Run: migration-analyze analyze");

    Ok(())
}
