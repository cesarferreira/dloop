//! Global + per-project TOML configuration.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalConfig {
    pub preferred_device_serial: Option<String>,
    pub default_log_level: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectConfig {
    pub package: Option<String>,
    /// Override inferred application IDs (multiple packages / flavors).
    #[serde(default)]
    pub packages: Option<Vec<String>>,
    pub variant: Option<String>,
    #[serde(default)]
    pub gradle_tasks: Vec<String>,
    #[serde(default)]
    pub log_filters: Vec<String>,
    pub log_level: Option<String>,
    pub assemble_task: Option<String>,
    pub install_task: Option<String>,
    #[serde(default)]
    pub scrcpy_args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct MergedConfig {
    pub global: GlobalConfig,
    pub project: ProjectConfig,
    #[allow(dead_code)]
    pub project_root: PathBuf,
}

impl MergedConfig {
    pub fn load(project_root: PathBuf) -> Result<Self> {
        let global = load_global_config().unwrap_or_default();

        let project_path = project_config_path(&project_root);
        let project = if let Some(ref path) = project_path {
            let s = fs::read_to_string(path)
                .with_context(|| format!("read {}", path.display()))?;
            toml::from_str(&s).with_context(|| "parse project config")?
        } else {
            ProjectConfig::default()
        };

        Ok(Self {
            global,
            project,
            project_root,
        })
    }
}

/// Prefer `.loopcat.toml`, fall back to `.droid-loop.toml` (PRD name).
fn project_config_path(project_root: &Path) -> Option<PathBuf> {
    let a = project_root.join(".loopcat.toml");
    if a.exists() {
        return Some(a);
    }
    let b = project_root.join(".droid-loop.toml");
    if b.exists() {
        return Some(b);
    }
    None
}

fn global_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("droid-loop").join("config.toml"))
}

pub fn load_global_config() -> Result<GlobalConfig> {
    let path = global_config_path().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    if !path.exists() {
        return Ok(GlobalConfig::default());
    }
    let s = fs::read_to_string(&path)?;
    Ok(toml::from_str(&s)?)
}

pub fn save_global_config(cfg: &GlobalConfig) -> Result<()> {
    let path = global_config_path().ok_or_else(|| anyhow::anyhow!("no config dir"))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let s = toml::to_string_pretty(cfg)?;
    fs::write(&path, s)?;
    Ok(())
}

#[allow(dead_code)]
pub fn save_project_config(project_root: &Path, cfg: &ProjectConfig) -> Result<()> {
    let path = project_root.join(".loopcat.toml");
    let s = toml::to_string_pretty(cfg)?;
    fs::write(&path, s)?;
    Ok(())
}
