pub mod compatibility;
pub mod config;
pub mod deps;
pub mod diff;
pub mod discovery;
pub mod git;
pub mod error;
pub mod graph;
pub mod language;
pub mod output;
pub mod output_paths;
pub mod parser;
pub mod project;
pub mod recommendation;
pub mod references;
pub mod scores;
pub mod symbols;
pub mod util;

pub mod align;
pub mod verify;

pub use config::Config;
pub use project::Project;
