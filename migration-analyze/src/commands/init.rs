use clap::Args;
use std::path::Path;

#[derive(Args)]
pub struct InitArgs {
    /// Name of the project directory to create
    pub name: String,

    /// Target language for migration (default: rust)
    #[arg(long, default_value = "rust")]
    pub target_lang: String,

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

    // Create minimal project config
    let config_content = format!(
        r#"# Migration Assessor Configuration
#
# 1. Clone your source repository into this directory:
#    git clone <repo-url>
#
# 2. Run analysis:
#    migration-analyze analyze

[project]
target_language = "{target_lang}"
# source_language = "typescript"   # auto-detected if not set

[skip]
framework = true

[output]
# directory = "report"
# split_by_directory = true
"#,
        target_lang = args.target_lang,
    );

    let config_path = project_root.join("migration.toml");
    std::fs::write(&config_path, config_content)?;

    // Create .gitignore with sensible defaults
    let gitignore_path = project_root.join(".gitignore");
    let gitignore_content = r#"# Migration artifacts
*-migration/

# IDE
.idea/
.vscode/
*.swp
"#;
    std::fs::write(&gitignore_path, gitignore_content)?;

    println!("✅ Created project '{}'", args.name);
    println!();
    println!("  Location: {}", project_root.display());
    println!();
    println!("  Next steps:");
    println!("    cd {}", args.name);
    println!("    git clone <your-source-repo>");
    println!("    migration-analyze analyze");

    Ok(())
}
