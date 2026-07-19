use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(
    name = "migration-analyze",
    version,
    about = "Migration assessment and analysis tool"
)]
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
    /// Show a summary of the latest analysis results
    Summary(commands::summary::SummaryArgs),
    /// Check source repo for updates since last analysis
    CheckUpdates(commands::check_updates::CheckUpdatesArgs),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Some(Commands::Init(args)) => commands::init::run(args)?,
        Some(Commands::Analyze(args)) => commands::analyze::run(args)?,
        Some(Commands::Diff(args)) => commands::diff::run(args)?,
        Some(Commands::Boundaries(args)) => commands::boundaries::run(args)?,
        Some(Commands::CheckUpdates(args)) => commands::check_updates::run(args)?,
        Some(Commands::Summary(args)) => commands::summary::run(args)?,
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
    println!("  init           Create a new migration project directory");
    println!("  analyze        Analyze source repo and create migration folder");
    println!("  diff           Run incremental git diff analysis");
    println!("  boundaries     Generate interface boundary report (layering + cut planes)");
    println!("  check-updates  Check source repo for updates since last analysis");
    println!("  summary        Show a summary of the latest analysis results");
    println!();
    println!("QUICK START:");
    println!("  1. migration-analyze init <project-name>");
    println!("  2. cd <project-name> && git clone <source-repo>");
    println!("  3. migration-analyze analyze");
    println!("  4. migration-analyze summary  (view results in terminal)");
    println!();
    println!("  For help on a specific command:");
    println!("    migration-analyze <COMMAND> --help");
}
