//! `bd init` — scaffold `.byedroid.toml` from Gradle inference.
use anyhow::Result;
use std::path::Path;

use crate::modules::config::{save_project_config, ProjectConfig};
use crate::modules::project::infer_project;

pub fn run_init(project_root: &Path) -> Result<()> {
    let inference = infer_project(project_root)?;
    let mut cfg = ProjectConfig::default();

    if !inference.application_ids.is_empty() {
        cfg.packages = Some(inference.application_ids.clone());
    }
    cfg.assemble_task = inference.assemble_task.clone();
    cfg.install_task = inference.install_task.clone();
    cfg.variant = Some(inference.variant_summary.clone());
    cfg.log_level = Some("D,I,W,E,V".to_string());

    save_project_config(project_root, &cfg)?;

    println!("Wrote .byedroid.toml in {}", project_root.display());
    if let Some(ref a) = cfg.assemble_task {
        println!("  assemble_task = {a:?}");
    }
    if let Some(ref i) = cfg.install_task {
        println!("  install_task  = {i:?}");
    }
    if let Some(pkgs) = &cfg.packages {
        println!("  packages      = {pkgs:?}");
    }
    Ok(())
}
