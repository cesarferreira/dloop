//! Two-row top info bar.
//! Row 1 (static): device · variant · branch · last build result
//! Row 2 (live):   build-in-progress banner  OR  toast  OR  logcat · pkg · level · crashes
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

const BG: Color = Color::Rgb(14, 14, 22);
const SEP_COL: Color = Color::Rgb(45, 45, 62);
const DIM: Color = Color::Rgb(80, 80, 100);
const MUTED: Color = Color::Rgb(100, 105, 130);

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    f.render_widget(row1(app), rows[0]);
    f.render_widget(row2(app), rows[1]);
}

// ── Row 1: Device · Variant · Branch · Build result ──────────────────────────
fn row1(app: &App) -> Paragraph<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw("  "));

    // Device
    if app.devices.is_empty() {
        spans.push(Span::styled(
            "⚠ no device",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    } else {
        let d = &app.devices[app.selected_device];
        spans.push(Span::styled(
            "󰟜 ",
            Style::default().fg(Color::Rgb(100, 220, 255)),
        ));
        spans.push(Span::styled(
            d.model.clone(),
            Style::default()
                .fg(Color::Rgb(180, 230, 255))
                .add_modifier(Modifier::BOLD),
        ));

        let rel = app
            .device_props
            .get("ro.build.version.release")
            .cloned()
            .unwrap_or_default();
        let sdk = app
            .device_props
            .get("ro.build.version.sdk")
            .cloned()
            .unwrap_or_default();
        if !rel.is_empty() || !sdk.is_empty() {
            let label = match (rel.is_empty(), sdk.is_empty()) {
                (false, false) => format!("  Android {rel}  API {sdk}"),
                (false, true) => format!("  Android {rel}"),
                (true, false) => format!("  API {sdk}"),
                (true, true) => String::new(),
            };
            spans.push(Span::styled(label, Style::default().fg(Color::Rgb(130, 140, 170))));
        }
        if let Some(b) = app.device_battery {
            spans.push(Span::styled(
                format!("  🔋{b}%"),
                Style::default().fg(Color::Rgb(180, 220, 140)),
            ));
        }
    }

    // Variant
    spans.push(sep());
    spans.push(Span::styled(
        short_task(&app.effective_assemble).to_string(),
        Style::default()
            .fg(Color::Rgb(180, 160, 255))
            .add_modifier(Modifier::BOLD),
    ));

    // Branch
    if let Some(branch) = &app.current_branch {
        spans.push(sep());
        spans.push(Span::styled(
            " ",
            Style::default().fg(Color::Rgb(120, 210, 150)),
        ));
        spans.push(Span::styled(
            branch.clone(),
            Style::default()
                .fg(Color::Rgb(160, 230, 185))
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Build result
    spans.push(sep());
    if app.build_task.is_some() {
        spans.push(Span::styled(
            "⟳ building…",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    } else if let Some(last) = app.build_history.last() {
        let (icon, col) = if last.exit_code == Some(0) {
            ("✓", Color::Rgb(130, 230, 130))
        } else {
            ("✗", Color::Rgb(230, 90, 90))
        };
        spans.push(Span::styled(
            format!("{icon} "),
            Style::default().fg(col).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{:.1}s", last.duration.as_secs_f64()),
            Style::default().fg(MUTED),
        ));
    } else {
        spans.push(Span::styled("no build yet", Style::default().fg(DIM)));
    }

    Paragraph::new(Line::from(spans)).style(Style::default().bg(BG))
}

// ── Row 2: build banner · toast · live logcat state ──────────────────────────
fn row2(app: &App) -> Paragraph<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw("  "));

    // Priority 1: build in progress — animated banner
    if let Some(ref task) = app.build_task {
        let elapsed = app.build_start.map(|s| s.elapsed().as_secs()).unwrap_or(0);
        let spin = SPINNER[(elapsed as usize) % SPINNER.len()];
        let action = if task.starts_with("install") {
            "INSTALLING"
        } else {
            "BUILDING"
        };
        let label = if app.launch_after_build {
            format!("{action} + LAUNCHING")
        } else {
            action.to_string()
        };
        spans.push(Span::styled(
            format!(" {spin} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(Color::Rgb(255, 220, 80))
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {}  {elapsed}s", short_task(task)),
            Style::default().fg(Color::Rgb(200, 190, 140)),
        ));
        return Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::Rgb(30, 28, 10)));
    }

    // Priority 2: toast
    if let Some((ref msg, _)) = app.toast {
        spans.push(Span::styled(
            msg.clone(),
            Style::default()
                .fg(Color::Rgb(255, 220, 100))
                .add_modifier(Modifier::BOLD),
        ));
        return Paragraph::new(Line::from(spans)).style(Style::default().bg(BG));
    }

    // Priority 3: live state
    if app.logcat_running {
        let count = app.log_lines.len();
        let c = if count >= 1_000 {
            format!("{}k", count / 1000)
        } else {
            count.to_string()
        };
        spans.push(Span::styled(
            "● ",
            Style::default()
                .fg(Color::Rgb(100, 230, 100))
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{c} lines"),
            Style::default().fg(Color::Rgb(140, 220, 140)),
        ));
        if app.log_scroll > 0 {
            spans.push(Span::styled(
                format!("  ↑+{}", app.log_scroll),
                Style::default().fg(DIM),
            ));
        }
    } else {
        spans.push(Span::styled("○ logcat off", Style::default().fg(DIM)));
    }

    // Package
    spans.push(sep());
    if app.show_all_logs {
        spans.push(Span::styled("all packages", Style::default().fg(DIM)));
    } else {
        let pkg = app
            .active_package_filter
            .clone()
            .or_else(|| app.effective_packages.first().cloned())
            .unwrap_or_else(|| "?".to_string());
        spans.push(Span::styled(
            pkg,
            Style::default()
                .fg(Color::Rgb(100, 200, 230))
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Level (only when non-default)
    let lvl = app.current_level_filter_summary();
    if lvl != "config" && lvl != "all" {
        spans.push(sep());
        spans.push(Span::styled(
            lvl,
            Style::default()
                .fg(Color::Rgb(244, 162, 97))
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Active filter input
    if app.filter_focused || !app.filter_input.is_empty() {
        spans.push(sep());
        spans.push(Span::styled(
            format!("filter: {}", app.filter_input),
            Style::default().fg(Color::Rgb(255, 200, 100)),
        ));
    }
    if app.exclude_focused || !app.exclude_input.is_empty() {
        spans.push(sep());
        spans.push(Span::styled(
            format!("exclude: {}", app.exclude_input),
            Style::default().fg(Color::Rgb(255, 160, 120)),
        ));
    }

    // Crash count
    let nc = app.crash_events.len();
    if nc > 0 {
        spans.push(sep());
        spans.push(Span::styled(
            format!("{nc} crash{}", if nc == 1 { "" } else { "es" }),
            Style::default()
                .fg(Color::Rgb(255, 120, 120))
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Multiple devices indicator
    if app.devices.len() > 1 {
        spans.push(sep());
        spans.push(Span::styled(
            format!("{} devices", app.devices.len()),
            Style::default().fg(DIM),
        ));
    }

    Paragraph::new(Line::from(spans)).style(Style::default().bg(BG))
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn sep() -> Span<'static> {
    Span::styled("  │  ", Style::default().fg(SEP_COL))
}

fn short_task(task: &str) -> &str {
    if let Some(rest) = task.strip_prefix("assemble") {
        rest
    } else if let Some(rest) = task.strip_prefix("install") {
        rest
    } else {
        task
    }
}
