use clap::{Parser, Subcommand};

mod commands;
mod web;

#[derive(Parser)]
#[command(name = "migration-analyze", version, about = "Migration assessment and analysis tool")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize migration analysis: create project directory and migration.toml
    Init(commands::init::InitArgs),
    /// Run full analysis on the project
    Analyze(commands::analyze::AnalyzeArgs),
    /// Run incremental git diff analysis
    Diff(commands::diff::DiffArgs),
    /// Generate interface boundary report for incremental migration
    Boundaries(commands::boundaries::BoundariesArgs),
    /// Start web UI server to browse analysis results
    Serve(commands::serve::ServeArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Init(args)) => commands::init::run(args)?,
        Some(Commands::Analyze(args)) => commands::analyze::run(args)?,
        Some(Commands::Diff(args)) => commands::diff::run(args)?,
        Some(Commands::Boundaries(args)) => commands::boundaries::run(args)?,
        Some(Commands::Serve(args)) => {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(commands::serve::run(args))?;
        }
        None => {
            print_usage_guide();
        }
    }

    Ok(())
}

fn print_usage_guide() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║           migration-analyze — Migration Tools          ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    println!("USAGE:");
    println!("  migration-analyze <COMMAND>");
    println!();
    println!("COMMANDS:");
    println!("  init       Create a new migration project directory");
    println!("  analyze    Analyze source repo and create migration folder");
    println!("  diff       Run incremental git diff analysis");
    println!("  boundaries Generate interface boundary report (layering + cut planes)");
    println!("  serve      Start web UI to browse analysis results");
    println!();
    println!("QUICK START:");
    println!("  1. migration-analyze init <project-name>");
    println!("  2. cd <project-name> && git clone <source-repo>");
    println!("  3. migration-analyze analyze");
    println!("  4. migration-analyze serve  (browse results at http://localhost:8080)");
    println!();
    println!("  For help on a specific command:");
    println!("    migration-analyze <COMMAND> --help");
}


