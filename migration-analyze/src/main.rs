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
    /// Verify target project changes match source diff
    Verify(commands::verify::VerifyArgs),
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
        Some(Commands::Verify(args)) => commands::verify::run(args)?,
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
    println!("  migration-analyze <COMMAND> [OPTIONS]");
    println!();
    println!("QUICK START:");
    println!("  migration-analyze init my-project      Create new migration project");
    println!("  cd my-project");
    println!("  # Edit migration.toml, set source_repo / source_lang");
    println!("  migration-analyze analyze              Analyze source repo");
    println!("  migration-analyze summary              See results in terminal");
    println!();
    println!("COMMANDS:");
    println!("  init <name>    Create a new migration project scaffold");
    println!("  analyze        Analyze source repo and generate migration report");
    println!("  summary        Show analysis results as a terminal summary");
    println!("  diff           Incremental diff analysis against a newer version");
    println!("  verify         Verify target changes match source diff");
    println!("  boundaries     Generate interface boundary report (layering + cut planes)");
    println!("  check-updates  Check source repo for updates since last analysis");
    println!();
    println!("EXAMPLES:");
    println!("  migration-analyze init my-app              # scaffold new project");
    println!("  migration-analyze analyze                   # analyze source repo");
    println!("  migration-analyze summary --format json     # output as JSON");
    println!("  migration-analyze diff --auto               # auto-detect latest version");
    println!("  migration-analyze check-updates             # check for source changes");
    println!();
    println!("Run: migration-analyze <COMMAND> --help  for detailed options.");
}
