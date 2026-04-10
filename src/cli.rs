//! CLI argument parsing.
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Generate `.loopcat.toml` from detected Gradle project settings
    Init,
}

#[derive(Parser, Debug)]
#[command(name = "dloop")]
#[command(author, version, about = "Droid Loop TUI — Android dev cockpit", long_about = None)]
pub struct Cli {
    /// Project directory (defaults to current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Option<Commands>,
}
