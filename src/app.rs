//! Application state and main run loop.
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Stdout, Write};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::action::Action;
use crate::adb::{AdbClient, Device};
use crate::event::{poll_event, AppEvent, Modal};
use crate::modules::build::{find_gradlew, spawn_gradle};
use crate::modules::config::save_global_config;
use crate::modules::config::MergedConfig;
use crate::modules::device::scan_devices;
use crate::modules::logcat::{
    clear_buffer, is_crash_continuation, is_crash_start, matches_any_exclude, matches_level_filter,
    parse_log_line, refresh_pids_for_packages, spawn_logcat_reader, CrashEvent, LogEntry,
    LogcatFilter,
};
use crate::modules::mirror::launch_scrcpy;
use crate::modules::project::{infer_project, ProjectInference};

const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_secs(1);
const GIT_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

/// Build a list of selectable (label, assemble_task, install_task) variants.
fn build_variant_list(
    inference: &ProjectInference,
    default_assemble: &str,
    default_install: &str,
) -> Vec<(String, String, String)> {
    // If no flavors, just offer the default debug variant
    if inference.flavor_names.is_empty() {
        return vec![(
            "Debug".to_string(),
            default_assemble.to_string(),
            default_install.to_string(),
        )];
    }

    // Generate all combinations using selected_flavors dimensions
    // For now, enumerate the known flavors grouped by dimension
    let dims = &inference.flavor_dimensions;
    let all_flavors = &inference.flavor_names;

    if dims.is_empty() {
        // Single dimension implied: each flavor gets Debug + Release
        let mut variants = Vec::new();
        for f in all_flavors {
            let seg = capitalize(f);
            variants.push((
                format!("{seg}Debug"),
                format!("assemble{seg}Debug"),
                format!("install{seg}Debug"),
            ));
        }
        // add plain debug/release too
        variants.push((
            "Debug".to_string(),
            "assembleDebug".to_string(),
            "installDebug".to_string(),
        ));
        return variants;
    }

    // With multiple dimensions, we'd need to enumerate all combos.
    // As a practical heuristic: just show one variant per "first-dimension flavor"
    // combined with the selected second-dimension flavor, plus the inferred default.
    let mut variants = Vec::new();

    // Always include the inferred default first
    variants.push((
        inference.variant_summary.clone(),
        default_assemble.to_string(),
        default_install.to_string(),
    ));

    // Also allow the user to pick by the first dimension's flavors
    for f in all_flavors {
        let seg = capitalize(f);
        let a = format!("assemble{seg}Debug");
        let i = format!("install{seg}Debug");
        let label = format!("{seg}Debug");
        if a != default_assemble {
            variants.push((label, a, i));
        }
    }

    variants
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

fn effective_package_list(config: &MergedConfig, inference: &ProjectInference) -> Vec<String> {
    if let Some(ref pkgs) = config.project.packages {
        if !pkgs.is_empty() {
            return pkgs.clone();
        }
    }
    if let Some(ref p) = config.project.package {
        if !p.is_empty() {
            return vec![p.clone()];
        }
    }
    inference.application_ids.clone()
}

fn same_log_entry(a: &LogEntry, b: &LogEntry) -> bool {
    a.raw == b.raw
        && a.timestamp == b.timestamp
        && a.pid == b.pid
        && a.tag == b.tag
        && a.level == b.level
        && a.message == b.message
}

fn preferred_device_index(
    devices: &[Device],
    previous_serial: Option<&str>,
    preferred_serial: Option<&str>,
    previous_selected: usize,
) -> usize {
    if let Some(serial) = previous_serial {
        if let Some(idx) = devices.iter().position(|d| d.serial == serial) {
            return idx;
        }
    }
    if let Some(serial) = preferred_serial {
        if let Some(idx) = devices.iter().position(|d| d.serial == serial) {
            return idx;
        }
    }
    previous_selected.min(devices.len().saturating_sub(1))
}

fn discover_current_branch(project_root: &std::path::Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(project_root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else if branch == "HEAD" {
        let output = Command::new("git")
            .args(["-C"])
            .arg(project_root)
            .args(["rev-parse", "--short", "HEAD"])
            .output()
            .ok()?;
        if !output.status.success() {
            return Some("detached".to_string());
        }
        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if sha.is_empty() {
            Some("detached".to_string())
        } else {
            Some(format!("detached@{sha}"))
        }
    } else {
        Some(branch)
    }
}

fn scroll_offset_to_entry<F>(
    log_lines: &[LogEntry],
    target: &LogEntry,
    mut is_visible: F,
) -> Option<usize>
where
    F: FnMut(&LogEntry) -> bool,
{
    let mut visible_after = 0usize;
    for entry in log_lines.iter().rev() {
        if !is_visible(entry) {
            continue;
        }
        if same_log_entry(entry, target) {
            return Some(visible_after);
        }
        visible_after += 1;
    }
    None
}
use crate::tui::restore_terminal;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Logs,
}

#[derive(Debug, Clone)]
pub struct BuildRecord {
    pub task: String,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    #[allow(dead_code)]
    pub finished_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelFilterMode {
    ConfigDefault,
    All,
    ErrorsOnly,
    WarningsPlus,
    InfoPlus,
    DebugPlus,
    Verbose,
    FatalOnly,
}

impl LevelFilterMode {
    pub fn title(self) -> &'static str {
        match self {
            Self::ConfigDefault => "Config default",
            Self::All => "All levels",
            Self::ErrorsOnly => "Errors only",
            Self::WarningsPlus => "Warnings + Errors",
            Self::InfoPlus => "Info + Warnings + Errors",
            Self::DebugPlus => "Debug +",
            Self::Verbose => "Verbose",
            Self::FatalOnly => "Fatal only",
        }
    }

    fn summary(self) -> &'static str {
        match self {
            Self::ConfigDefault => "config",
            Self::All => "all",
            Self::ErrorsOnly => "errors",
            Self::WarningsPlus => "warn+",
            Self::InfoPlus => "info+",
            Self::DebugPlus => "debug+",
            Self::Verbose => "verbose",
            Self::FatalOnly => "fatal",
        }
    }

    fn levels(self) -> Option<&'static str> {
        match self {
            Self::ConfigDefault => None,
            Self::All => Some(""),
            Self::ErrorsOnly => Some("E,F"),
            Self::WarningsPlus => Some("W,E,F"),
            Self::InfoPlus => Some("I,W,E,F"),
            Self::DebugPlus => Some("D,I,W,E,F"),
            Self::Verbose => Some("V,D,I,W,E,F"),
            Self::FatalOnly => Some("F"),
        }
    }
}

pub const LEVEL_FILTER_OPTIONS: &[LevelFilterMode] = &[
    LevelFilterMode::ConfigDefault,
    LevelFilterMode::All,
    LevelFilterMode::ErrorsOnly,
    LevelFilterMode::WarningsPlus,
    LevelFilterMode::InfoPlus,
    LevelFilterMode::DebugPlus,
    LevelFilterMode::Verbose,
    LevelFilterMode::FatalOnly,
];

pub struct App {
    pub adb: AdbClient,
    pub project_root: PathBuf,
    pub config: MergedConfig,
    pub current_branch: Option<String>,

    pub devices: Vec<Device>,
    pub selected_device: usize,
    #[allow(dead_code)]
    pub active_pane: Pane,

    // ── popup/overlay state ───────────────────────────────────────────────
    pub device_picker_open: bool,
    pub device_picker_cursor: usize,

    pub build_popup_open: bool,
    pub build_popup_scroll: usize,

    pub crash_detail_open: bool,
    pub crash_detail_scroll: usize,
    /// When set, the build popup auto-closes once this instant is reached.
    pub build_popup_auto_close: Option<Instant>,

    pub package_picker_open: bool,
    pub package_picker_input: String,
    pub package_picker_cursor: usize,
    /// `None` = show all; `Some(pkg)` = filter to this package only
    pub active_package_filter: Option<String>,
    pub level_picker_open: bool,
    pub level_picker_cursor: usize,
    pub level_filter_mode: LevelFilterMode,

    pub logcat_child: Option<Child>,
    pub logcat_running: bool,
    pub logcat_paused: bool,
    pub log_lines: Vec<LogEntry>,
    pub max_log_lines: usize,
    pub filter_input: String,
    pub filter_focused: bool,
    pub exclude_input: String,
    pub exclude_focused: bool,
    pub tag_color_cache: HashMap<String, usize>,
    pub package_pids: Vec<String>,

    pub crash_events: Vec<CrashEvent>,
    pub current_crash: Option<CrashEvent>,

    pub device_props: HashMap<String, String>,
    pub device_battery: Option<u8>,
    pub installed_device_packages: Vec<String>,
    pub last_device_info_refresh: Instant,

    pub build_history_open: bool,
    pub build_history_scroll: usize,
    pub help_open: bool,

    pub build_child: Option<Child>,
    pub build_task: Option<String>,
    pub build_start: Option<Instant>,
    pub build_lines: Vec<String>,
    pub build_history: Vec<BuildRecord>,
    #[allow(dead_code)]
    pub build_expanded: bool,

    pub toast: Option<(String, Instant)>,

    rx_log: mpsc::Receiver<String>,
    tx_log: mpsc::Sender<String>,
    rx_build: mpsc::Receiver<String>,
    tx_build: mpsc::Sender<String>,

    pub last_device_refresh: Instant,
    pub last_git_refresh: Instant,
    pub pid_refresh: Instant,

    /// Parsed from `app/build.gradle(.kts)` when possible.
    pub inference: ProjectInference,
    /// Resolved application IDs for logcat (config overrides Gradle inference).
    pub effective_packages: Vec<String>,
    pub effective_assemble: String,
    pub effective_install: String,

    /// How many lines from the bottom the logcat viewport is scrolled (0 = follow tail).
    pub log_scroll: usize,
    /// When true, show ALL logcat lines (no package filter).
    pub show_all_logs: bool,
    /// When true, launch the app after the current build finishes successfully.
    pub launch_after_build: bool,

    // Variant picker state
    pub picker_open: bool,
    pub picker_cursor: usize,
    /// Generated list of build variants (assemble + install pairs).
    pub picker_variants: Vec<(String, String, String)>, // (label, assemble, install)
}

impl App {
    /// Which modal is currently active (for event routing).
    pub fn active_modal(&self) -> Modal {
        if self.filter_focused {
            return Modal::Filter;
        }
        if self.exclude_focused {
            return Modal::ExcludeFilter;
        }
        if self.level_picker_open {
            return Modal::LevelPicker;
        }
        if self.picker_open {
            return Modal::VariantPicker;
        }
        if self.device_picker_open {
            return Modal::DevicePicker;
        }
        if self.crash_detail_open {
            return Modal::CrashDetail;
        }
        if self.build_popup_open {
            return Modal::BuildPopup;
        }
        if self.package_picker_open {
            return Modal::PackagePicker;
        }
        if self.build_history_open {
            return Modal::BuildHistory;
        }
        if self.help_open {
            return Modal::HelpPopup;
        }
        Modal::None
    }

    /// Config + runtime exclude substrings (for pane display consistency).
    pub fn merged_exclude_substrings(&self) -> Vec<String> {
        let mut v = self.config.project.exclude_filters.clone();
        if !self.exclude_input.is_empty() {
            v.push(self.exclude_input.clone());
        }
        v
    }

    /// Whether the log pane should show this line (content + exclude), independent of ingest filter.
    pub fn pane_shows_entry(&self, entry: &LogEntry) -> bool {
        if !matches_level_filter(self.effective_log_levels().as_deref(), &entry.level) {
            return false;
        }
        if !self.filter_input.is_empty() {
            let hay = format!("{} {} {}", entry.tag, entry.level, entry.message).to_lowercase();
            if !hay.contains(&self.filter_input.to_lowercase()) {
                return false;
            }
        }
        !matches_any_exclude(&self.merged_exclude_substrings(), entry)
    }

    /// Filtered package list used for the package picker display.
    pub fn filtered_package_list(&self) -> Vec<String> {
        let all = self.all_known_packages();
        if self.package_picker_input.is_empty() {
            return all;
        }
        let q = self.package_picker_input.to_lowercase();
        let mut matches: Vec<(PackageMatchScore, String)> = all
            .into_iter()
            .filter_map(|pkg| package_match_score(&pkg, &q).map(|score| (score, pkg)))
            .collect();
        matches.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        matches.into_iter().map(|(_, pkg)| pkg).collect()
    }

    pub fn all_known_packages(&self) -> Vec<String> {
        merge_known_packages(
            &self.effective_packages,
            &self.inference.application_ids,
            &self.installed_device_packages,
        )
    }

    pub fn current_level_filter_summary(&self) -> String {
        match self.level_filter_mode {
            LevelFilterMode::ConfigDefault => self
                .effective_log_levels()
                .map(|levels| format!("cfg {levels}"))
                .unwrap_or_else(|| "all".to_string()),
            mode => mode.summary().to_string(),
        }
    }

    pub fn current_level_filter_label(&self) -> String {
        match self.level_filter_mode {
            LevelFilterMode::ConfigDefault => self
                .effective_log_levels()
                .map(|levels| format!("Config default ({levels})"))
                .unwrap_or_else(|| "Config default (all levels)".to_string()),
            mode => match mode.levels() {
                Some(levels) if !levels.is_empty() => format!("{} ({levels})", mode.title()),
                _ => mode.title().to_string(),
            },
        }
    }
}

impl App {
    pub fn new(project_root: PathBuf, config: MergedConfig) -> Result<Self> {
        let current_branch = discover_current_branch(&project_root);
        let adb = AdbClient::new()?;
        let inference = infer_project(&project_root).unwrap_or_else(|_| ProjectInference {
            variant_summary: "Could not read Gradle files".to_string(),
            ..Default::default()
        });
        let effective_packages = effective_package_list(&config, &inference);
        let effective_assemble = config
            .project
            .assemble_task
            .clone()
            .or(inference.assemble_task.clone())
            .unwrap_or_else(|| "assembleDebug".to_string());
        let effective_install = config
            .project
            .install_task
            .clone()
            .or(inference.install_task.clone())
            .unwrap_or_else(|| "installDebug".to_string());
        let show_all_logs = effective_packages.is_empty();

        let picker_variants =
            build_variant_list(&inference, &effective_assemble, &effective_install);

        let (tx_log, rx_log) = mpsc::channel();
        let (tx_build, rx_build) = mpsc::channel();
        let mut app = Self {
            adb,
            project_root,
            config,
            current_branch,
            devices: Vec::new(),
            selected_device: 0,
            active_pane: Pane::Logs,
            device_picker_open: false,
            device_picker_cursor: 0,
            build_popup_open: false,
            build_popup_scroll: 0,
            crash_detail_open: false,
            crash_detail_scroll: 0,
            build_popup_auto_close: None,
            package_picker_open: false,
            package_picker_input: String::new(),
            package_picker_cursor: 0,
            active_package_filter: None,
            level_picker_open: false,
            level_picker_cursor: 0,
            level_filter_mode: LevelFilterMode::ConfigDefault,
            logcat_child: None,
            logcat_running: false,
            logcat_paused: false,
            log_lines: Vec::new(),
            max_log_lines: 10_000,
            filter_input: String::new(),
            filter_focused: false,
            exclude_input: String::new(),
            exclude_focused: false,
            tag_color_cache: HashMap::new(),
            package_pids: Vec::new(),
            crash_events: Vec::new(),
            current_crash: None,
            device_props: HashMap::new(),
            device_battery: None,
            installed_device_packages: Vec::new(),
            last_device_info_refresh: Instant::now() - Duration::from_secs(60),
            build_history_open: false,
            build_history_scroll: 0,
            help_open: false,
            build_child: None,
            build_task: None,
            build_start: None,
            build_lines: Vec::new(),
            build_history: Vec::new(),
            build_expanded: false,
            toast: None,
            rx_log,
            tx_log,
            rx_build,
            tx_build,
            last_device_refresh: Instant::now(),
            last_git_refresh: Instant::now(),
            pid_refresh: Instant::now(),
            inference,
            effective_packages,
            effective_assemble,
            effective_install,
            log_scroll: 0,
            show_all_logs,
            launch_after_build: false,
            picker_open: false,
            picker_cursor: 0,
            picker_variants,
        };
        app.refresh_devices()?;
        app.refresh_device_info();
        if let Some(ref serial) = app.config.global.preferred_device_serial {
            if let Some(idx) = app.devices.iter().position(|d| &d.serial == serial) {
                app.selected_device = idx;
            }
        }
        app.refresh_device_info();
        app.refresh_device_packages();
        // Auto-start logcat if a device is already connected.
        if !app.devices.is_empty() {
            let _ = app.start_logcat();
        }
        Ok(app)
    }

    pub fn selected_serial(&self) -> Option<&str> {
        self.devices
            .get(self.selected_device)
            .map(|d| d.serial.as_str())
    }

    pub fn refresh_devices(&mut self) -> Result<()> {
        let had_devices = !self.devices.is_empty();
        let previous_selected = self.selected_device;
        let prev = self.selected_serial().map(|s| s.to_string());
        self.devices = scan_devices(&self.adb)?;
        if let Some(ref prev_serial) = prev {
            if !self.devices.iter().any(|d| d.serial == *prev_serial) {
                self.show_toast("Device disconnected — stopped logcat");
                self.stop_logcat();
                self.selected_device = 0;
            }
        }

        if !self.devices.is_empty() {
            self.selected_device = preferred_device_index(
                &self.devices,
                prev.as_deref(),
                self.config.global.preferred_device_serial.as_deref(),
                previous_selected,
            );
        }
        self.last_device_refresh = Instant::now();
        self.refresh_device_info();
        self.refresh_device_packages();
        // Auto-start logcat when a device becomes available and it isn't running yet.
        if !self.devices.is_empty() && !self.logcat_running {
            if had_devices {
                self.show_toast("Device connected — resumed logcat");
            }
            let _ = self.start_logcat();
        }
        Ok(())
    }

    fn tick_device_refresh(&mut self) {
        if self.last_device_refresh.elapsed() < DEVICE_REFRESH_INTERVAL {
            return;
        }
        let _ = self.refresh_devices();
    }

    fn tick_git_refresh(&mut self) {
        if self.last_git_refresh.elapsed() < GIT_REFRESH_INTERVAL {
            return;
        }
        self.last_git_refresh = Instant::now();

        let branch = discover_current_branch(&self.project_root);
        if branch != self.current_branch {
            self.current_branch = branch.clone();
            if let Some(branch) = branch {
                self.show_toast(format!("Branch: {branch}"));
            }
        }
    }

    fn refresh_device_info(&mut self) {
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            self.device_props.clear();
            self.device_battery = None;
            return;
        };
        self.device_props = self
            .adb
            .get_device_props(serial.as_str())
            .unwrap_or_default();
        self.device_battery = self.adb.get_battery_level(serial.as_str()).ok().flatten();
        self.last_device_info_refresh = Instant::now();
    }

    fn refresh_device_packages(&mut self) {
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            self.installed_device_packages.clear();
            return;
        };
        self.installed_device_packages = self
            .adb
            .list_installed_packages(serial.as_str())
            .unwrap_or_default();
    }

    fn tick_device_info_refresh(&mut self) {
        if self.last_device_info_refresh.elapsed() < Duration::from_secs(30) {
            return;
        }
        self.refresh_device_info();
    }

    fn finalize_crash(&mut self) {
        if let Some(c) = self.current_crash.take() {
            if !c.lines.is_empty() {
                const MAX_CRASH_EVENTS: usize = 100;
                if self.crash_events.len() >= MAX_CRASH_EVENTS {
                    let drop = self.crash_events.len() - MAX_CRASH_EVENTS + 1;
                    self.crash_events.drain(0..drop);
                }
                self.crash_events.push(c);
            }
        }
    }

    fn apply_crash_fsm(&mut self, entry: &mut LogEntry) {
        if self
            .current_crash
            .as_ref()
            .map(|c| c.lines.len())
            .unwrap_or(0)
            >= 50
        {
            self.finalize_crash();
        }
        if let Some(ref mut crash) = self.current_crash {
            if crash.lines.len() < 50 && is_crash_continuation(entry) {
                crash.lines.push(entry.clone());
                return;
            }
            self.finalize_crash();
            if is_crash_start(entry) {
                entry.crash_start = true;
                self.current_crash = Some(CrashEvent {
                    timestamp: entry.timestamp.clone(),
                    summary: entry.message.clone(),
                    lines: vec![entry.clone()],
                });
            }
        } else if is_crash_start(entry) {
            entry.crash_start = true;
            self.current_crash = Some(CrashEvent {
                timestamp: entry.timestamp.clone(),
                summary: entry.message.clone(),
                lines: vec![entry.clone()],
            });
        }
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn export_logs_to_file(&mut self) -> std::io::Result<()> {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = self.project_root.join(format!("byedroid-{ts}.log"));
        let mut f = fs::File::create(&path)?;
        let mut n = 0usize;
        for entry in &self.log_lines {
            if !self.pane_shows_entry(entry) {
                continue;
            }
            writeln!(f, "{}", entry.raw)?;
            n += 1;
        }
        self.show_toast(format!("Exported {n} lines to {}", path.display()));
        Ok(())
    }

    fn format_crash_text(crash: &CrashEvent) -> String {
        let mut text = format!("--- {} @ {} ---\n", crash.summary, crash.timestamp);
        for line in &crash.lines {
            text.push_str(&line.raw);
            text.push('\n');
        }
        text
    }

    fn yank_last_crash(&mut self) -> Result<(), String> {
        let Some(crash) = self.crash_events.last() else {
            return Err("no crash captured yet".to_string());
        };
        let text = Self::format_crash_text(crash);
        copy_to_clipboard(&text).map_err(|e| e.to_string())?;
        self.show_toast(format!(
            "Copied crash details ({} lines)",
            crash.lines.len()
        ));
        Ok(())
    }

    fn scroll_log_to_last_crash(&mut self) {
        let Some(crash_start) = self
            .crash_events
            .last()
            .and_then(|crash| crash.lines.first())
        else {
            return;
        };

        let scroll = scroll_offset_to_entry(&self.log_lines, crash_start, |entry| {
            self.pane_shows_entry(entry)
        });
        if let Some(scroll) = scroll {
            self.log_scroll = scroll;
        }
    }

    fn open_last_crash_detail(&mut self) {
        self.scroll_log_to_last_crash();
        self.crash_detail_open = true;
        self.crash_detail_scroll = self
            .crash_events
            .last()
            .map(|crash| crash.lines.len())
            .unwrap_or(0);
    }

    fn export_crash_to_file(&mut self) -> std::io::Result<()> {
        let Some(crash) = self.crash_events.last() else {
            return Err(std::io::Error::other("no crash captured yet"));
        };
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let path = self.project_root.join(format!("crash-{ts}.log"));
        let content = Self::format_crash_text(crash);
        fs::write(&path, content)?;
        self.show_toast(format!("Saved crash to {}", path.display()));
        Ok(())
    }

    fn search_crash_online(&mut self) -> std::io::Result<()> {
        let Some(crash) = self.crash_events.last() else {
            return Err(std::io::Error::other("no crash captured yet"));
        };
        let q = format!("{} {}", crash.summary, crash.timestamp);
        let encoded = url_encode_query(&q);
        let url = format!("https://www.google.com/search?q={encoded}");
        open_url(&url)?;
        self.show_toast("Opened search in browser");
        Ok(())
    }

    fn drain_channels(&mut self) {
        let mut got_new = false;
        while let Ok(line) = self.rx_log.try_recv() {
            if self.logcat_paused {
                continue;
            }
            if let Some(mut entry) = parse_log_line(&line) {
                let filter = self.current_log_filter();
                if !filter.allows(&entry, &self.package_pids) {
                    continue;
                }
                self.apply_crash_fsm(&mut entry);
                if self.log_lines.len() >= self.max_log_lines {
                    let drop = self.log_lines.len() - self.max_log_lines + 1;
                    self.log_lines.drain(0..drop);
                }
                self.log_lines.push(entry);
                got_new = true;
            } else if (line.starts_with("[adb logcat stderr]") || line.starts_with("[adb"))
                && self.log_lines.len() < self.max_log_lines
            {
                self.log_lines.push(LogEntry {
                    raw: line.clone(),
                    timestamp: String::new(),
                    pid: String::new(),
                    tag: "adb".into(),
                    level: "E".into(),
                    message: line,
                    crash_start: false,
                });
                got_new = true;
            }
        }
        // When following tail (scroll==0), keep scroll at 0 as lines come in.
        // When scrolled, advance the offset so the viewport stays on the same lines.
        if got_new && self.log_scroll > 0 {
            self.log_scroll += 1; // keep same relative position as buffer grows
        }
        while let Ok(line) = self.rx_build.try_recv() {
            self.build_lines.push(line);
            let max = 50_000usize;
            if self.build_lines.len() > max {
                let drop = self.build_lines.len() - max;
                self.build_lines.drain(0..drop);
            }
        }
    }

    fn current_log_filter(&self) -> LogcatFilter {
        let level = self.effective_log_levels();
        let content = if !self.filter_input.is_empty() {
            Some(self.filter_input.clone())
        } else {
            None
        };
        // show_all_logs=true → no PID filter; active_package_filter overrides effective_packages.
        let filter_by_pkg = !self.show_all_logs
            && (self.active_package_filter.is_some() || !self.effective_packages.is_empty());
        let mut exclude_substrings = self.config.project.exclude_filters.clone();
        if !self.exclude_input.is_empty() {
            exclude_substrings.push(self.exclude_input.clone());
        }
        LogcatFilter {
            filter_by_application_ids: filter_by_pkg,
            tag_substrings: self.config.project.log_filters.clone(),
            levels: level,
            content,
            exclude_substrings,
        }
    }

    fn effective_log_levels(&self) -> Option<String> {
        match self.level_filter_mode {
            LevelFilterMode::ConfigDefault => self
                .config
                .project
                .log_level
                .clone()
                .or_else(|| self.config.global.default_log_level.clone()),
            mode => {
                let levels = mode.levels().unwrap_or("");
                if levels.is_empty() {
                    None
                } else {
                    Some(levels.to_string())
                }
            }
        }
    }

    /// The package(s) currently used for PID filtering.
    pub fn filter_packages(&self) -> Vec<String> {
        if let Some(ref p) = self.active_package_filter {
            vec![p.clone()]
        } else {
            self.effective_packages.clone()
        }
    }

    pub fn stop_logcat(&mut self) {
        if let Some(mut c) = self.logcat_child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.logcat_running = false;
    }

    pub fn start_logcat(&mut self) -> Result<()> {
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            self.show_toast("No device selected");
            return Ok(());
        };
        self.stop_logcat();
        let pkgs = self.filter_packages();
        if !pkgs.is_empty() {
            self.package_pids =
                refresh_pids_for_packages(&self.adb, serial.as_str(), &pkgs).unwrap_or_default();
        }
        let child = spawn_logcat_reader(&self.adb.adb_path, serial.as_str(), self.tx_log.clone())?;
        self.logcat_child = Some(child);
        self.logcat_running = true;
        self.logcat_paused = false;
        Ok(())
    }

    pub fn clear_logs(&mut self) {
        self.log_lines.clear();
        if let Some(serial) = self.selected_serial() {
            let _ = clear_buffer(&self.adb, serial);
        }
    }

    pub fn stop_build(&mut self) {
        if let Some(mut c) = self.build_child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        self.build_task = None;
        self.build_start = None;
    }

    fn finish_build_record(&mut self, code: Option<i32>) {
        if let Some(task) = self.build_task.take() {
            let start = self.build_start.take().unwrap_or_else(Instant::now);
            self.build_history.push(BuildRecord {
                task: task.clone(),
                exit_code: code,
                duration: start.elapsed(),
                finished_at: Instant::now(),
            });
            if code == Some(0) {
                self.show_toast(format!("{task} OK"));
            } else {
                self.show_toast(format!("{task} failed: exit {code:?}"));
            }
            // Start the auto-close countdown (3 seconds) for the build popup.
            if self.build_popup_open {
                self.build_popup_auto_close = Some(Instant::now() + Duration::from_secs(3));
            }
        }
    }

    pub fn run_build_task(&mut self, task: &str) -> Result<()> {
        if self.build_child.is_some() {
            self.show_toast("Already running (s to stop)");
            return Ok(());
        }
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            self.show_toast("No device selected");
            return Ok(());
        };
        let gradlew = match find_gradlew(&self.project_root) {
            Some(g) => g,
            None => {
                self.show_toast("gradlew not found in project root");
                return Ok(());
            }
        };
        self.build_lines.clear();
        let spawn = spawn_gradle(
            &gradlew,
            &self.project_root,
            &[task],
            Some(serial.as_str()),
            self.tx_build.clone(),
        )?;
        self.build_child = Some(spawn.child);
        self.build_task = Some(task.to_string());
        self.build_start = Some(Instant::now());
        self.build_popup_open = true;
        self.build_popup_scroll = 0;
        self.build_popup_auto_close = None;
        Ok(())
    }

    pub fn run_clean_build(&mut self) -> Result<()> {
        if self.build_child.is_some() {
            self.show_toast("Already running (s to stop)");
            return Ok(());
        }
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            self.show_toast("No device selected");
            return Ok(());
        };
        let gradlew = match find_gradlew(&self.project_root) {
            Some(g) => g,
            None => {
                self.show_toast("gradlew not found in project root");
                return Ok(());
            }
        };
        let assemble = self.effective_assemble.clone();
        let display = format!("clean {assemble}");
        self.build_lines.clear();
        let spawn = spawn_gradle(
            &gradlew,
            &self.project_root,
            &["clean", assemble.as_str()],
            Some(serial.as_str()),
            self.tx_build.clone(),
        )?;
        self.build_child = Some(spawn.child);
        self.build_task = Some(display);
        self.build_start = Some(Instant::now());
        self.build_popup_open = true;
        self.build_popup_scroll = 0;
        self.build_popup_auto_close = None;
        Ok(())
    }

    pub fn poll_build_finished(&mut self) {
        if let Some(ref mut child) = self.build_child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code();
                    self.build_child = None;
                    let do_launch = self.launch_after_build && code == Some(0);
                    self.launch_after_build = false;
                    self.finish_build_record(code);
                    if do_launch {
                        self.launch_app();
                    }
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
    }

    /// Resolve the target package: picker selection → last built APK → first configured package.
    fn resolve_target_package(&self) -> Option<String> {
        if let Some(ref pkg) = self.active_package_filter {
            return Some(pkg.clone());
        }
        if let Some(pkg) = self.read_built_package_id() {
            return Some(pkg);
        }
        self.effective_packages.first().cloned()
    }

    fn uninstall_app(&mut self) {
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            self.show_toast("No device selected");
            return;
        };
        let Some(package) = self.resolve_target_package() else {
            self.show_toast("No package known — can't uninstall");
            return;
        };
        let result = std::process::Command::new(&self.adb.adb_path)
            .args(["-s", &serial, "uninstall", &package])
            .output();
        match result {
            Ok(out) if out.status.success() => self.show_toast(format!("Uninstalled {package}")),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                self.show_toast(format!("Uninstall failed: {stderr}"));
            }
            Err(e) => self.show_toast(format!("Uninstall error: {e}")),
        }
    }

    fn clear_app_data(&mut self) {
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            self.show_toast("No device selected");
            return;
        };
        let Some(package) = self.resolve_target_package() else {
            self.show_toast("No package known — can't clear data");
            return;
        };
        let result = std::process::Command::new(&self.adb.adb_path)
            .args(["-s", &serial, "shell", "pm", "clear", &package])
            .output();
        match result {
            Ok(out) if out.status.success() => {
                self.show_toast(format!("Cleared all data for {package}"));
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                self.show_toast(format!("Clear data failed: {stderr}"));
            }
            Err(e) => self.show_toast(format!("Clear data error: {e}")),
        }
    }

    fn clear_app_cache(&mut self) {
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            self.show_toast("No device selected");
            return;
        };
        let Some(package) = self.resolve_target_package() else {
            self.show_toast("No package known — can't clear cache");
            return;
        };
        let result = std::process::Command::new(&self.adb.adb_path)
            .args(["-s", &serial, "shell", "pm", "clear", "--cache-only", &package])
            .output();
        match result {
            Ok(out) if out.status.success() => {
                self.show_toast(format!("Cleared cache for {package}"));
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                self.show_toast(format!("Clear cache failed: {stderr}"));
            }
            Err(e) => self.show_toast(format!("Clear cache error: {e}")),
        }
    }

    fn launch_app(&mut self) {
        let Some(serial) = self.selected_serial().map(|s| s.to_string()) else {
            return;
        };

        // Priority 1 — user explicitly chose a package via the picker.
        // Priority 2 — read the applicationId that Gradle wrote into output-metadata.json
        //              (this is exactly what Android Studio does; it always matches the
        //              installed variant rather than the base/prod package ID).
        // Priority 3 — fall back to the first inferred package.
        let package = if let Some(pkg) = self.resolve_target_package() {
            pkg
        } else {
            self.show_toast("No package known — can't launch");
            return;
        };
        // `monkey -p <pkg> -c android.intent.category.LAUNCHER 1` reliably starts the app.
        let result = std::process::Command::new(&self.adb.adb_path)
            .args([
                "-s",
                &serial,
                "shell",
                "monkey",
                "-p",
                &package,
                "-c",
                "android.intent.category.LAUNCHER",
                "1",
            ])
            .output();
        match result {
            Ok(out) if out.status.success() => self.show_toast(format!("Launched {package}")),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                self.show_toast(format!("Launch failed: {stderr}"));
            }
            Err(e) => self.show_toast(format!("Launch error: {e}")),
        }
    }

    /// Walk `app/build/outputs/apk/` for the most recently written `output-metadata.json`
    /// and extract the `applicationId` field.  This mirrors how Android Studio determines
    /// which package ID was produced by the last Gradle build.
    fn read_built_package_id(&self) -> Option<String> {
        let apk_dir = self.project_root.join("app/build/outputs/apk");
        if !apk_dir.exists() {
            return None;
        }
        let mut best: Option<(std::time::SystemTime, String)> = None;
        collect_apk_metadata(&apk_dir, &mut best);
        best.map(|(_, pkg)| pkg)
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        match action {
            Action::Quit => return Ok(true),
            Action::ToggleLogcat => {
                if self.logcat_running {
                    self.stop_logcat();
                } else if let Err(e) = self.start_logcat() {
                    self.show_toast(format!("logcat: {e}"));
                }
            }
            Action::FocusFilter => {
                self.filter_focused = !self.filter_focused;
                if self.filter_focused {
                    self.exclude_focused = false;
                }
            }
            Action::ClearFilter => {
                self.filter_input.clear();
                self.filter_focused = false;
            }
            Action::FocusExclude => {
                self.exclude_focused = !self.exclude_focused;
                if self.exclude_focused {
                    self.filter_focused = false;
                }
            }
            Action::ClearExclude => {
                self.exclude_input.clear();
                self.exclude_focused = false;
            }
            Action::ExportLogs => {
                if let Err(e) = self.export_logs_to_file() {
                    self.show_toast(format!("export: {e}"));
                }
            }
            Action::OpenCrashDetail => {
                if self.crash_events.is_empty() {
                    self.show_toast("no crash captured yet");
                } else {
                    self.open_last_crash_detail();
                }
            }
            Action::CrashCopy => {
                if let Err(e) = self.yank_last_crash() {
                    self.show_toast(format!("yank: {e}"));
                } else {
                    self.crash_detail_open = false;
                }
            }
            Action::CrashAgent => {
                let Some(crash) = self.crash_events.last() else {
                    self.show_toast("no crash captured yet");
                    return Ok(false);
                };
                let body = Self::format_crash_text(crash);
                let prompt = format!("Solve this issue. Here are the crash logs:\n\n{body}");
                if let Err(e) = copy_to_clipboard(&prompt) {
                    self.show_toast(format!("copy: {e}"));
                } else {
                    self.show_toast("Agent prompt copied — paste into your AI assistant");
                    self.crash_detail_open = false;
                }
            }
            Action::CrashExport => {
                if let Err(e) = self.export_crash_to_file() {
                    self.show_toast(format!("export: {e}"));
                }
            }
            Action::CrashSearch => {
                if let Err(e) = self.search_crash_online() {
                    self.show_toast(format!("search: {e}"));
                }
            }
            Action::OpenHelp => {
                self.help_open = !self.help_open;
            }
            Action::ConfirmNo => {
                self.filter_focused = false;
                self.exclude_focused = false;
                self.level_picker_open = false;
                self.picker_open = false;
                self.device_picker_open = false;
                self.crash_detail_open = false;
                self.build_popup_open = false;
                self.package_picker_open = false;
                self.build_history_open = false;
                self.help_open = false;
                self.package_picker_input.clear();
            }
            Action::ClearLogs => self.clear_logs(),
            Action::ToggleLogcatPause => {
                self.logcat_paused = !self.logcat_paused;
            }
            Action::TogglePackageFilter => {
                self.show_all_logs = !self.show_all_logs;
                let mode = if self.show_all_logs {
                    "all logs"
                } else {
                    "package filter"
                };
                self.show_toast(format!("Logcat: {mode}"));
            }
            Action::OpenLevelPicker => {
                self.level_picker_open = true;
                self.level_picker_cursor = LEVEL_FILTER_OPTIONS
                    .iter()
                    .position(|mode| *mode == self.level_filter_mode)
                    .unwrap_or(0);
            }
            Action::ScrollUp => {
                if self.build_history_open {
                    self.build_history_scroll = self.build_history_scroll.saturating_sub(1);
                } else if self.crash_detail_open {
                    self.crash_detail_scroll = self.crash_detail_scroll.saturating_add(1);
                } else if self.build_popup_open {
                    self.build_popup_scroll = self.build_popup_scroll.saturating_add(1);
                    self.build_popup_auto_close = None;
                } else {
                    self.log_scroll = self.log_scroll.saturating_add(1);
                }
            }
            Action::ScrollDown => {
                if self.build_history_open {
                    let max = self.build_history.len().saturating_sub(1);
                    self.build_history_scroll = (self.build_history_scroll + 1).min(max);
                } else if self.crash_detail_open {
                    self.crash_detail_scroll = self.crash_detail_scroll.saturating_sub(1);
                } else if self.build_popup_open {
                    self.build_popup_scroll = self.build_popup_scroll.saturating_sub(1);
                    self.build_popup_auto_close = None;
                } else {
                    self.log_scroll = self.log_scroll.saturating_sub(1);
                }
            }
            Action::ScrollPageUp => {
                if self.build_history_open {
                    self.build_history_scroll = self.build_history_scroll.saturating_sub(10);
                } else if self.crash_detail_open {
                    self.crash_detail_scroll = self.crash_detail_scroll.saturating_add(10);
                } else {
                    self.log_scroll = self.log_scroll.saturating_add(20);
                }
            }
            Action::ScrollPageDown => {
                if self.build_history_open {
                    let max = self.build_history.len().saturating_sub(1);
                    self.build_history_scroll = (self.build_history_scroll + 10).min(max);
                } else if self.crash_detail_open {
                    self.crash_detail_scroll = self.crash_detail_scroll.saturating_sub(10);
                } else {
                    self.log_scroll = self.log_scroll.saturating_sub(20);
                }
            }
            Action::ScrollTail => {
                self.log_scroll = 0;
            }
            // ── popup opens ───────────────────────────────────────────────
            Action::OpenVariantPicker => {
                if !self.picker_variants.is_empty() {
                    self.picker_open = true;
                    self.picker_cursor = 0;
                }
            }
            Action::OpenDevicePicker => {
                self.device_picker_open = true;
                self.device_picker_cursor = self.selected_device;
            }
            Action::OpenBuildPopup => {
                self.build_popup_open = !self.build_popup_open;
                self.build_popup_scroll = 0;
                self.build_popup_auto_close = None;
            }
            Action::OpenBuildHistory => {
                self.build_history_open = !self.build_history_open;
                self.build_history_scroll = 0;
            }
            Action::OpenPackagePicker => {
                self.refresh_device_packages();
                self.package_picker_open = true;
                self.package_picker_input.clear();
                self.package_picker_cursor = 0;
            }
            // ── shared picker navigation ──────────────────────────────────
            Action::PickerNext => {
                if self.build_history_open {
                    let max = self.build_history.len().saturating_sub(1);
                    self.build_history_scroll = (self.build_history_scroll + 1).min(max);
                } else if self.level_picker_open {
                    self.level_picker_cursor =
                        (self.level_picker_cursor + 1) % LEVEL_FILTER_OPTIONS.len();
                } else if self.device_picker_open {
                    if !self.devices.is_empty() {
                        self.device_picker_cursor =
                            (self.device_picker_cursor + 1) % self.devices.len();
                    }
                } else if self.package_picker_open {
                    let filtered = self.filtered_package_list();
                    let has_custom_entry =
                        filtered.is_empty() && !self.package_picker_input.is_empty();
                    let n = filtered.len() + 1 + usize::from(has_custom_entry); // +1 for "All"
                    self.package_picker_cursor = (self.package_picker_cursor + 1) % n.max(1);
                } else if self.picker_open && !self.picker_variants.is_empty() {
                    self.picker_cursor = (self.picker_cursor + 1) % self.picker_variants.len();
                }
            }
            Action::PickerPrev => {
                if self.build_history_open {
                    self.build_history_scroll = self.build_history_scroll.saturating_sub(1);
                } else if self.level_picker_open {
                    self.level_picker_cursor = if self.level_picker_cursor == 0 {
                        LEVEL_FILTER_OPTIONS.len() - 1
                    } else {
                        self.level_picker_cursor - 1
                    };
                } else if self.device_picker_open {
                    if !self.devices.is_empty() {
                        self.device_picker_cursor = if self.device_picker_cursor == 0 {
                            self.devices.len() - 1
                        } else {
                            self.device_picker_cursor - 1
                        };
                    }
                } else if self.package_picker_open {
                    let filtered = self.filtered_package_list();
                    let has_custom_entry =
                        filtered.is_empty() && !self.package_picker_input.is_empty();
                    let n = filtered.len() + 1 + usize::from(has_custom_entry);
                    self.package_picker_cursor = if self.package_picker_cursor == 0 {
                        n.saturating_sub(1)
                    } else {
                        self.package_picker_cursor - 1
                    };
                } else if self.picker_open && !self.picker_variants.is_empty() {
                    self.picker_cursor = if self.picker_cursor == 0 {
                        self.picker_variants.len() - 1
                    } else {
                        self.picker_cursor - 1
                    };
                }
            }
            Action::PickerConfirm => {
                if self.device_picker_open {
                    self.selected_device = self.device_picker_cursor;
                    self.persist_preferred_device();
                    self.refresh_device_info();
                    self.refresh_device_packages();
                    if self.logcat_running {
                        self.stop_logcat();
                        let _ = self.start_logcat();
                    }
                    self.device_picker_open = false;
                } else if self.level_picker_open {
                    if let Some(mode) = LEVEL_FILTER_OPTIONS.get(self.level_picker_cursor).copied()
                    {
                        self.level_filter_mode = mode;
                        self.show_toast(format!("Levels: {}", self.current_level_filter_label()));
                    }
                    self.level_picker_open = false;
                } else if self.package_picker_open {
                    let filtered = self.filtered_package_list();
                    let has_custom_entry =
                        filtered.is_empty() && !self.package_picker_input.is_empty();
                    if self.package_picker_cursor == 0 {
                        // "All packages"
                        self.active_package_filter = None;
                        self.show_all_logs = true;
                    } else if let Some(pkg) = filtered.get(self.package_picker_cursor - 1).cloned()
                    {
                        self.active_package_filter = Some(pkg.clone());
                        self.show_all_logs = false;
                        self.show_toast(format!("Filtering: {pkg}"));
                    } else if has_custom_entry && self.package_picker_cursor == filtered.len() + 1 {
                        let chosen = self.package_picker_input.clone();
                        self.active_package_filter = Some(chosen.clone());
                        self.show_all_logs = false;
                        self.show_toast(format!("Filtering: {chosen}"));
                    }
                    self.package_picker_open = false;
                    self.package_picker_input.clear();
                    // Restart logcat with new filter
                    if self.logcat_running {
                        self.stop_logcat();
                        let _ = self.start_logcat();
                    }
                } else if self.picker_open {
                    if let Some((label, a, i)) =
                        self.picker_variants.get(self.picker_cursor).cloned()
                    {
                        self.effective_assemble = a;
                        self.effective_install = i;
                        self.show_toast(format!("Variant: {label}"));
                    }
                    self.picker_open = false;
                }
            }
            Action::PickerCancel => {
                self.level_picker_open = false;
                self.picker_open = false;
                self.device_picker_open = false;
                self.crash_detail_open = false;
                self.build_popup_open = false;
                self.package_picker_open = false;
                self.build_history_open = false;
                self.help_open = false;
                self.package_picker_input.clear();
            }
            Action::BuildDebug => {
                self.launch_after_build = false;
                let task = self.effective_assemble.clone();
                if let Err(e) = self.run_build_task(&task) {
                    self.show_toast(format!("build: {e}"));
                }
            }
            Action::CleanBuild => {
                self.launch_after_build = false;
                if let Err(e) = self.run_clean_build() {
                    self.show_toast(format!("clean build: {e}"));
                }
            }
            Action::InstallDebug => {
                self.launch_after_build = false;
                let task = self.effective_install.clone();
                if let Err(e) = self.run_build_task(&task) {
                    self.show_toast(format!("install: {e}"));
                }
            }
            Action::RunApp => {
                // install task builds + installs in one step; then we launch
                self.launch_after_build = true;
                let task = self.effective_install.clone();
                if let Err(e) = self.run_build_task(&task) {
                    self.launch_after_build = false;
                    self.show_toast(format!("run: {e}"));
                }
            }
            Action::StopProcess => {
                self.stop_build();
                self.stop_logcat();
            }
            Action::UninstallApp => self.uninstall_app(),
            Action::ClearAppData => self.clear_app_data(),
            Action::ClearAppCache => self.clear_app_cache(),
            Action::LaunchScrcpy => {
                let Some(serial) = self.selected_serial() else {
                    self.show_toast("No device");
                    return Ok(false);
                };
                let extra: Vec<String> = self.config.project.scrcpy_args.clone();
                if let Err(e) = launch_scrcpy(serial, &extra) {
                    self.show_toast(format!("scrcpy: {e}"));
                } else {
                    self.show_toast("Launched scrcpy");
                }
            }
        }
        Ok(false)
    }

    fn persist_preferred_device(&mut self) {
        if let Some(s) = self.selected_serial() {
            self.config.global.preferred_device_serial = Some(s.to_string());
            let _ = save_global_config(&self.config.global);
        }
    }

    fn tick_pid_refresh(&mut self) {
        if self.pid_refresh.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.pid_refresh = Instant::now();
        let Some(serial) = self.selected_serial() else {
            return;
        };
        let pkgs = self.filter_packages();
        if !pkgs.is_empty() {
            self.package_pids =
                refresh_pids_for_packages(&self.adb, serial, &pkgs).unwrap_or_default();
        }
    }
}

pub fn run_app(mut terminal: Terminal<CrosstermBackend<Stdout>>, mut app: App) -> Result<()> {
    let tick = Duration::from_millis(50);
    loop {
        app.drain_channels();
        app.poll_build_finished();
        app.tick_device_refresh();
        app.tick_git_refresh();
        app.tick_pid_refresh();
        app.tick_device_info_refresh();

        if let Some((_, t)) = &app.toast {
            if t.elapsed() > Duration::from_secs(4) {
                app.toast = None;
            }
        }

        // Auto-close the build popup after the countdown expires.
        if let Some(deadline) = app.build_popup_auto_close {
            if Instant::now() >= deadline {
                app.build_popup_open = false;
                app.build_popup_auto_close = None;
            }
        }

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if let Some(ev) = poll_event(tick, app.active_modal())? {
            match ev {
                AppEvent::Text(ch) => {
                    if app.package_picker_open {
                        app.package_picker_input.push(ch);
                        app.package_picker_cursor = 0;
                    } else if app.exclude_focused {
                        app.exclude_input.push(ch);
                    } else {
                        app.filter_input.push(ch);
                    }
                }
                AppEvent::Backspace => {
                    if app.package_picker_open {
                        app.package_picker_input.pop();
                        app.package_picker_cursor = 0;
                    } else if app.exclude_focused {
                        app.exclude_input.pop();
                    } else {
                        app.filter_input.pop();
                    }
                }
                AppEvent::Action(a) => {
                    if app.handle_action(a)? {
                        break;
                    }
                }
            }
        }
    }
    app.finalize_crash();
    app.stop_logcat();
    app.stop_build();
    restore_terminal()?;
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Recursively scan `dir` for `output-metadata.json` files and collect
/// (modified_time, applicationId) pairs.  Updates `best` with the most recent.
fn collect_apk_metadata(dir: &std::path::Path, best: &mut Option<(std::time::SystemTime, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_apk_metadata(&path, best);
        } else if path.file_name().and_then(|n| n.to_str()) == Some("output-metadata.json") {
            let Ok(content) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(meta) = path.metadata() else { continue };
            let Ok(modified) = meta.modified() else {
                continue;
            };

            // Extract `"applicationId": "com.example.app"` with a simple scan —
            // avoids pulling in serde_json as a direct dependency.
            if let Some(pkg) = extract_application_id_from_metadata(&content) {
                let is_newer = best.as_ref().map_or(true, |(t, _)| modified > *t);
                if is_newer {
                    *best = Some((modified, pkg));
                }
            }
        }
    }
}

fn copy_to_clipboard(text: &str) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .or_else(|_| {
            Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()
        })
        .or_else(|_| Command::new("wl-copy").stdin(Stdio::piped()).spawn())?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(text.as_bytes())?;
    }
    let _ = child.wait();
    Ok(())
}

fn url_encode_query(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(out, "%{:02X}", b);
            }
        }
    }
    out
}

fn open_url(url: &str) -> std::io::Result<()> {
    use std::process::Command;
    Command::new("open")
        .arg(url)
        .spawn()
        .or_else(|_| Command::new("xdg-open").arg(url).spawn())?;
    Ok(())
}

fn extract_application_id_from_metadata(json: &str) -> Option<String> {
    // The file is small and well-structured; a linear scan is fine.
    let key = "\"applicationId\"";
    let start = json.find(key)?;
    let after_key = &json[start + key.len()..];
    let colon = after_key.find(':')?;
    let after_colon = after_key[colon + 1..].trim_start();
    if after_colon.starts_with('"') {
        let inner = &after_colon[1..];
        let end = inner.find('"')?;
        let pkg = inner[..end].trim().to_string();
        if !pkg.is_empty() {
            return Some(pkg);
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct PackageMatchScore {
    tier: u8,
    metric: usize,
    len: usize,
}

fn package_match_score(pkg: &str, query: &str) -> Option<PackageMatchScore> {
    if query.is_empty() {
        return Some(PackageMatchScore {
            tier: 0,
            metric: 0,
            len: pkg.len(),
        });
    }

    let lower = pkg.to_lowercase();
    if lower.starts_with(query) {
        return Some(PackageMatchScore {
            tier: 0,
            metric: lower.len().saturating_sub(query.len()),
            len: lower.len(),
        });
    }
    if let Some(pos) = lower.find(query) {
        return Some(PackageMatchScore {
            tier: 1,
            metric: pos,
            len: lower.len(),
        });
    }

    let mut last_idx = 0usize;
    let mut started = false;
    let mut gap_cost = 0usize;
    let mut chars = lower.char_indices();

    for q in query.chars() {
        let Some((idx, _)) = chars.find(|(_, c)| *c == q) else {
            return None;
        };
        if started {
            gap_cost += idx.saturating_sub(last_idx);
        } else {
            gap_cost += idx;
            started = true;
        }
        last_idx = idx + q.len_utf8();
    }

    Some(PackageMatchScore {
        tier: 2,
        metric: gap_cost,
        len: lower.len(),
    })
}

fn merge_known_packages(
    effective_packages: &[String],
    inferred_packages: &[String],
    installed_packages: &[String],
) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut merged = Vec::new();

    for pkg in effective_packages
        .iter()
        .chain(inferred_packages.iter())
        .chain(installed_packages.iter())
    {
        if seen.insert(pkg.clone()) {
            merged.push(pkg.clone());
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::{
        merge_known_packages, package_match_score, preferred_device_index, scroll_offset_to_entry,
        Device, LevelFilterMode, LogEntry, PackageMatchScore,
    };

    fn entry(raw: &str) -> LogEntry {
        LogEntry {
            raw: raw.to_string(),
            timestamp: "12:00:00.000".to_string(),
            pid: "1234".to_string(),
            tag: "AndroidRuntime".to_string(),
            level: "E".to_string(),
            message: raw.to_string(),
            crash_start: false,
        }
    }

    #[test]
    fn scroll_offset_matches_visible_entries_after_target() {
        let before = entry("before");
        let crash = entry("FATAL EXCEPTION: main");
        let after_one = entry("after-one");
        let after_two = entry("after-two");

        let lines = vec![
            before.clone(),
            crash.clone(),
            after_one.clone(),
            after_two.clone(),
        ];

        let scroll = scroll_offset_to_entry(&lines, &crash, |_| true);
        assert_eq!(scroll, Some(2));
    }

    #[test]
    fn scroll_offset_skips_hidden_entries() {
        let before = entry("before");
        let crash = entry("FATAL EXCEPTION: main");
        let hidden = entry("hidden");
        let after = entry("after");

        let lines = vec![before.clone(), crash.clone(), hidden.clone(), after.clone()];

        let scroll = scroll_offset_to_entry(&lines, &crash, |entry| entry.raw != "hidden");
        assert_eq!(scroll, Some(1));
    }

    #[test]
    fn merge_known_packages_keeps_priority_and_dedups() {
        let effective = vec!["ai.wayve.app.dev".to_string()];
        let inferred = vec!["ai.wayve.app".to_string(), "ai.wayve.app.dev".to_string()];
        let installed = vec![
            "android".to_string(),
            "ai.wayve.app".to_string(),
            "com.example.tool".to_string(),
        ];

        assert_eq!(
            merge_known_packages(&effective, &inferred, &installed),
            vec![
                "ai.wayve.app.dev".to_string(),
                "ai.wayve.app".to_string(),
                "android".to_string(),
                "com.example.tool".to_string(),
            ]
        );
    }

    #[test]
    fn package_match_prefers_prefix_then_contains_then_fuzzy() {
        let prefix = package_match_score("wayve.driver", "way").unwrap();
        let contains = package_match_score("com.wayve.driver", "way").unwrap();
        let fuzzy = package_match_score("com.wax_y.app", "way").unwrap();

        assert_eq!(
            package_match_score("ai.wayve.driver", "ai"),
            Some(PackageMatchScore {
                tier: 0,
                metric: "ai.wayve.driver".len() - 2,
                len: "ai.wayve.driver".len(),
            })
        );
        assert!(prefix < contains);
        assert!(contains < fuzzy);
        assert!(package_match_score("com.example.app", "wve").is_none());
    }

    #[test]
    fn level_filter_mode_maps_to_expected_levels() {
        assert_eq!(LevelFilterMode::ErrorsOnly.levels(), Some("E,F"));
        assert_eq!(LevelFilterMode::WarningsPlus.levels(), Some("W,E,F"));
        assert_eq!(LevelFilterMode::All.levels(), Some(""));
        assert_eq!(LevelFilterMode::ConfigDefault.levels(), None);
    }

    #[test]
    fn preferred_device_index_prefers_previous_serial_then_preferred_serial() {
        let devices = vec![
            Device {
                serial: "emulator-5554".to_string(),
                model: "Emulator".to_string(),
            },
            Device {
                serial: "pixel-serial".to_string(),
                model: "Pixel".to_string(),
            },
        ];

        assert_eq!(
            preferred_device_index(&devices, Some("pixel-serial"), Some("emulator-5554"), 0),
            1
        );
        assert_eq!(
            preferred_device_index(&devices, None, Some("pixel-serial"), 0),
            1
        );
        assert_eq!(preferred_device_index(&devices, None, None, 0), 0);
    }
}
