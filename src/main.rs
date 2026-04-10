//! Droid Loop TUI — Android build, install, and log workflows.
mod action;
mod adb;
mod app;
mod cli;
mod event;
mod modules;
mod tui;
mod ui;

use anyhow::Result;
use clap::Parser;

use crate::cli::{Cli, Commands};
use crate::modules::config::MergedConfig;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let project_root = cli
        .project
        .clone()
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));

    match cli.command {
        Some(Commands::Init) => {
            modules::init::run_init(&project_root)?;
            return Ok(());
        }
        None => {}
    }

    let config = MergedConfig::load(project_root.clone())?;
    let app = app::App::new(project_root, config)?;
    let terminal = tui::init_terminal()?;
    app::run_app(terminal, app)?;
    Ok(())
}
