//! CLI argument parsing.
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "dloop")]
#[command(author, version, about = "Droid Loop TUI — Android dev cockpit", long_about = None)]
pub struct Cli {
    /// Project directory (defaults to current directory)
    #[arg(short, long)]
    pub project: Option<PathBuf>,
}
