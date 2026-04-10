//! Application state and main run loop.
use anyhow::Result;
use std::collections::HashMap;
use std::io::Stdout;
use std::path::PathBuf;
use std::process::Child;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::action::Action;
use crate::adb::{AdbClient, Device};
use crate::event::{poll_event, AppEvent};
use crate::modules::build::{find_gradlew, spawn_gradle};
use crate::modules::config::save_global_config;
use crate::modules::config::MergedConfig;
use crate::modules::device::scan_devices;
use crate::modules::logcat::{
    clear_buffer, parse_log_line, refresh_pids_for_packages, spawn_logcat_reader, LogEntry,
    LogcatFilter,
};
use crate::modules::mirror::launch_scrcpy;
use crate::modules::project::{infer_project, ProjectInference};

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
        variants.push(("Debug".to_string(), "assembleDebug".to_string(), "installDebug".to_string()));
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
use crate::tui::restore_terminal;
use crate::ui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Devices,
    Build,
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

pub struct App {
    pub adb: AdbClient,
    pub project_root: PathBuf,
    pub config: MergedConfig,

    pub devices: Vec<Device>,
    pub selected_device: usize,
    pub active_pane: Pane,

    pub logcat_child: Option<Child>,
    pub logcat_running: bool,
    pub logcat_paused: bool,
    pub log_lines: Vec<LogEntry>,
    pub max_log_lines: usize,
    pub filter_input: String,
    pub filter_focused: bool,
    pub tag_color_cache: HashMap<String, usize>,
    pub package_pids: Vec<String>,

    pub build_child: Option<Child>,
    pub build_task: Option<String>,
    pub build_start: Option<Instant>,
    pub build_lines: Vec<String>,
    pub build_history: Vec<BuildRecord>,
    /// Whether the build pane is expanded to show full output.
    pub build_expanded: bool,

    pub toast: Option<(String, Instant)>,

    rx_log: mpsc::Receiver<String>,
    tx_log: mpsc::Sender<String>,
    rx_build: mpsc::Receiver<String>,
    tx_build: mpsc::Sender<String>,

    pub last_device_refresh: Instant,
    pub pid_refresh: Instant,

    /// Parsed from `app/build.gradle(.kts)` when possible.
    pub inference: ProjectInference,
    /// Resolved application IDs for logcat (config overrides Gradle inference).
    pub effective_packages: Vec<String>,
    pub effective_assemble: String,
    pub effective_install: String,

    /// How many lines from the bottom the logcat viewport is scrolled (0 = follow tail).
    pub log_scroll: usize,
    /// When true, show ALL logcat lines (no package filter). Default = true.
    pub show_all_logs: bool,

    // Variant picker state
    pub picker_open: bool,
    pub picker_cursor: usize,
    /// Generated list of build variants (assemble + install pairs).
    pub picker_variants: Vec<(String, String, String)>, // (label, assemble, install)
}

impl App {
    pub fn new(project_root: PathBuf, config: MergedConfig) -> Result<Self> {
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

        let picker_variants = build_variant_list(&inference, &effective_assemble, &effective_install);

        let (tx_log, rx_log) = mpsc::channel();
        let (tx_build, rx_build) = mpsc::channel();
        let mut app = Self {
            adb,
            project_root,
            config,
            devices: Vec::new(),
            selected_device: 0,
            active_pane: Pane::Devices,
            logcat_child: None,
            logcat_running: false,
            logcat_paused: false,
            log_lines: Vec::new(),
            max_log_lines: 10_000,
            filter_input: String::new(),
            filter_focused: false,
            tag_color_cache: HashMap::new(),
            package_pids: Vec::new(),
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
            pid_refresh: Instant::now(),
            inference,
            effective_packages,
            effective_assemble,
            effective_install,
            log_scroll: 0,
            show_all_logs: true, // show everything by default; 'a' toggles package filter
            picker_open: false,
            picker_cursor: 0,
            picker_variants,
        };
        app.refresh_devices()?;
        if let Some(ref serial) = app.config.global.preferred_device_serial {
            if let Some(idx) = app.devices.iter().position(|d| &d.serial == serial) {
                app.selected_device = idx;
            }
        }
        // Auto-start logcat if a device is already connected.
        if !app.devices.is_empty() {
            let _ = app.start_logcat();
        }
        Ok(app)
    }

    pub fn selected_serial(&self) -> Option<&str> {
        self.devices.get(self.selected_device).map(|d| d.serial.as_str())
    }

    pub fn refresh_devices(&mut self) -> Result<()> {
        let prev = self.selected_serial().map(|s| s.to_string());
        self.devices = scan_devices(&self.adb)?;
        if let Some(prev_serial) = prev {
            if !self.devices.iter().any(|d| d.serial == prev_serial) {
                self.show_toast("Device disconnected — stopped logcat");
                self.stop_logcat();
                self.selected_device = 0;
            } else if let Some(idx) = self.devices.iter().position(|d| d.serial == prev_serial) {
                self.selected_device = idx;
            }
        }
        if !self.devices.is_empty() && self.selected_device >= self.devices.len() {
            self.selected_device = self.devices.len() - 1;
        }
        self.last_device_refresh = Instant::now();
        // Auto-start logcat when a device becomes available and it isn't running yet.
        if !self.devices.is_empty() && !self.logcat_running {
            let _ = self.start_logcat();
        }
        Ok(())
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn drain_channels(&mut self) {
        let mut got_new = false;
        while let Ok(line) = self.rx_log.try_recv() {
            if self.logcat_paused {
                continue;
            }
            if let Some(entry) = parse_log_line(&line) {
                let filter = self.current_log_filter();
                if filter.allows(&entry, &self.package_pids) {
                    if self.log_lines.len() >= self.max_log_lines {
                        let drop = self.log_lines.len() - self.max_log_lines + 1;
                        self.log_lines.drain(0..drop);
                    }
                    self.log_lines.push(entry);
                    got_new = true;
                }
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
        let level = self
            .config
            .project
            .log_level
            .clone()
            .or_else(|| self.config.global.default_log_level.clone());
        let content = if !self.filter_input.is_empty() {
            Some(self.filter_input.clone())
        } else {
            None
        };
        // Only filter by package PID when the user explicitly opted in AND packages are known.
        let filter_by_pkg = !self.show_all_logs && !self.effective_packages.is_empty();
        LogcatFilter {
            filter_by_application_ids: filter_by_pkg,
            tag_substrings: self.config.project.log_filters.clone(),
            levels: level,
            content,
            exclude: None,
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
        if !self.effective_packages.is_empty() {
            self.package_pids = refresh_pids_for_packages(
                &self.adb,
                serial.as_str(),
                &self.effective_packages,
            )
            .unwrap_or_default();
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
            task,
            Some(serial.as_str()),
            self.tx_build.clone(),
        )?;
        self.build_child = Some(spawn.child);
        self.build_task = Some(task.to_string());
        self.build_start = Some(Instant::now());
        Ok(())
    }

    pub fn poll_build_finished(&mut self) {
        if let Some(ref mut child) = self.build_child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code();
                    self.build_child = None;
                    self.finish_build_record(code);
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }
    }

    fn handle_action(&mut self, action: Action) -> Result<bool> {
        if self.filter_focused {
            match action {
                Action::Quit => return Ok(true),
                Action::FocusFilter => {
                    self.filter_focused = false;
                }
                Action::ConfirmNo => {
                    self.filter_focused = false;
                }
                _ => {}
            }
            if matches!(
                action,
                Action::Quit | Action::FocusFilter | Action::ConfirmNo
            ) {
                return Ok(matches!(action, Action::Quit));
            }
        }

        match action {
            Action::Quit => return Ok(true),
            Action::RefreshDevices => {
                if let Err(e) = self.refresh_devices() {
                    self.show_toast(format!("devices: {e}"));
                } else {
                    let prev = self.selected_serial().map(|s| s.to_string());
                    if let Some(s) = prev {
                        if let Some(i) = self.devices.iter().position(|d| d.serial == s) {
                            self.selected_device = i;
                        }
                    }
                    if self.logcat_running {
                        self.stop_logcat();
                        let _ = self.start_logcat();
                    }
                }
            }
            Action::NextPane => {
                self.active_pane = match self.active_pane {
                    Pane::Devices => Pane::Build,
                    Pane::Build => Pane::Logs,
                    Pane::Logs => Pane::Devices,
                };
            }
            Action::PrevPane => {
                self.active_pane = match self.active_pane {
                    Pane::Devices => Pane::Logs,
                    Pane::Build => Pane::Devices,
                    Pane::Logs => Pane::Build,
                };
            }
            Action::NextDevice => {
                if !self.devices.is_empty() {
                    self.selected_device = (self.selected_device + 1) % self.devices.len();
                    self.persist_preferred_device();
                }
            }
            Action::PrevDevice => {
                if !self.devices.is_empty() {
                    self.selected_device = if self.selected_device == 0 {
                        self.devices.len() - 1
                    } else {
                        self.selected_device - 1
                    };
                    self.persist_preferred_device();
                }
            }
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
                    self.active_pane = Pane::Logs;
                }
            }
            Action::ClearLogs => self.clear_logs(),
            Action::ToggleLogcatPause => {
                self.logcat_paused = !self.logcat_paused;
            }
            Action::TogglePackageFilter => {
                self.show_all_logs = !self.show_all_logs;
                let mode = if self.show_all_logs { "all logs" } else { "package filter" };
                self.show_toast(format!("Logcat: {mode}"));
            }
            Action::ScrollUp => {
                self.log_scroll = self.log_scroll.saturating_add(1);
            }
            Action::ScrollDown => {
                self.log_scroll = self.log_scroll.saturating_sub(1);
            }
            Action::ScrollPageUp => {
                self.log_scroll = self.log_scroll.saturating_add(20);
            }
            Action::ScrollPageDown => {
                self.log_scroll = self.log_scroll.saturating_sub(20);
            }
            Action::ScrollTail => {
                self.log_scroll = 0;
            }
            Action::ToggleBuildExpand => {
                self.build_expanded = !self.build_expanded;
            }
            Action::OpenVariantPicker => {
                if !self.picker_variants.is_empty() {
                    self.picker_open = true;
                    self.picker_cursor = 0;
                }
            }
            Action::PickerNext => {
                if !self.picker_variants.is_empty() {
                    self.picker_cursor = (self.picker_cursor + 1) % self.picker_variants.len();
                }
            }
            Action::PickerPrev => {
                if !self.picker_variants.is_empty() {
                    self.picker_cursor = if self.picker_cursor == 0 {
                        self.picker_variants.len() - 1
                    } else {
                        self.picker_cursor - 1
                    };
                }
            }
            Action::PickerConfirm => {
                if let Some((label, a, i)) = self.picker_variants.get(self.picker_cursor).cloned() {
                    self.effective_assemble = a;
                    self.effective_install = i;
                    self.show_toast(format!("Variant: {label}"));
                }
                self.picker_open = false;
            }
            Action::PickerCancel => {
                self.picker_open = false;
            }
            Action::BuildDebug => {
                let task = self.effective_assemble.clone();
                if let Err(e) = self.run_build_task(&task) {
                    self.show_toast(format!("build: {e}"));
                }
            }
            Action::InstallDebug => {
                let task = self.effective_install.clone();
                if let Err(e) = self.run_build_task(&task) {
                    self.show_toast(format!("install: {e}"));
                }
            }
            Action::StopProcess => {
                self.stop_build();
                self.stop_logcat();
            }
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
            Action::ConfirmYes | Action::ConfirmNo => {}
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
        if !self.effective_packages.is_empty() {
            self.package_pids = refresh_pids_for_packages(&self.adb, serial, &self.effective_packages)
                .unwrap_or_default();
        }
    }
}

pub fn run_app(
    mut terminal: Terminal<CrosstermBackend<Stdout>>,
    mut app: App,
) -> Result<()> {
    let tick = Duration::from_millis(50);
    loop {
        app.drain_channels();
        app.poll_build_finished();
        app.tick_pid_refresh();

        if let Some((_, t)) = &app.toast {
            if t.elapsed() > Duration::from_secs(4) {
                app.toast = None;
            }
        }

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if let Some(ev) = poll_event(tick, app.filter_focused, app.active_pane, app.picker_open)? {
            match ev {
                AppEvent::Text(ch) => {
                    app.filter_input.push(ch);
                }
                AppEvent::Backspace => {
                    app.filter_input.pop();
                }
                AppEvent::Action(a) => {
                    if app.handle_action(a)? {
                        break;
                    }
                }
            }
        }
    }
    app.stop_logcat();
    app.stop_build();
    restore_terminal()?;
    Ok(())
}
