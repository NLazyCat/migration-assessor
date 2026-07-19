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
    println!("  migration-analyze <COMMAND> [OPTIONS] [PATH]");
    println!();
    println!("QUICK START (in your source repo):");
    println!("  migration-analyze analyze         Analyze current project");
    println!("  migration-analyze summary         See results in terminal");
    println!("  migration-analyze diff --auto     Check what changed since analysis");
    println!();
    println!("COMMANDS:");
    println!("  analyze        Analyze source repo and generate migration report");
    println!("  summary        Show analysis results as a terminal summary");
    println!("  diff           Incremental diff analysis against a newer version");
    println!("  boundaries     Generate interface boundary report (layering + cut planes)");
    println!("  check-updates  Check source repo for updates since last analysis");
    println!("  init           Create a new migration project scaffold");
    println!();
    println!("EXAMPLES:");
    println!("  migration-analyze analyze                            # analyze current directory");
    println!("  migration-analyze analyze ../my-project               # analyze a project");
    println!("  migration-analyze summary --format json               # output as JSON");
    println!("  migration-analyze diff --new-version v2.0.0           # diff against a tag");
    println!(
        "  migration-analyze diff --auto                         # auto-detect latest version"
    );
    println!("  migration-analyze check-updates                       # check for source changes");
    println!();
    println!("ALL COMMANDS accept a path argument (default = current directory).");
    println!("Run: migration-analyze <COMMAND> --help  for detailed options.");
}
