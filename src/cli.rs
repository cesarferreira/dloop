//! CLI argument parsing.
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Generate `.byedroid.toml` from detected Gradle project settings
    Init,
}

#[derive(Parser, Debug)]
#[command(name = "bye")]
#[command(author, version, about = "ByeDroid TUI — Android dev cockpit", long_about = None)]
pub struct Cli {
    /// Project directory (defaults to current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Commands>,
}
