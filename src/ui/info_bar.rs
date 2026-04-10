//! Three-row top info bar: device · build · logcat · package · activity
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

const BG: Color = Color::Rgb(14, 14, 22);
const BG2: Color = Color::Rgb(18, 18, 28);
const SEP_COL: Color = Color::Rgb(45, 45, 62);
const DIM: Color = Color::Rgb(80, 80, 100);
const MUTED: Color = Color::Rgb(100, 105, 130);

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    f.render_widget(row1(app), rows[0]);
    f.render_widget(row2(app), rows[1]);
    f.render_widget(row3(app), rows[2]);
}

// ── Row 1: Device + Build variant + Build status ──────────────────────────────
fn row1(app: &App) -> Paragraph<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    spans.push(Span::raw("  "));

    if app.devices.is_empty() {
        spans.push(Span::styled(
            "⚠ no device connected",
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
        spans.push(Span::styled(
            format!("  [{}]", truncate(&d.serial, 10)),
            Style::default().fg(Color::Rgb(70, 90, 110)),
        ));
    }

    spans.push(sep());

    let variant = short_task(&app.effective_assemble).to_string();
    spans.push(Span::styled(" variant  ", Style::default().fg(DIM)));
    spans.push(Span::styled(
        variant,
        Style::default()
            .fg(Color::Rgb(180, 160, 255))
            .add_modifier(Modifier::BOLD),
    ));

    spans.push(sep());

    // Build result (compact — the big in-progress banner lives on row 3)
    spans.push(Span::raw("  "));
    if app.build_task.is_some() {
        // Just a brief marker; row 3 has the full banner
        spans.push(Span::styled(
            "⟳ building…",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
    } else if let Some(last) = app.build_history.last() {
        let (icon, col, label) = if last.exit_code == Some(0) {
            ("✓", Color::Rgb(130, 230, 130), "build ok")
        } else {
            ("✗", Color::Rgb(230, 90, 90), "build failed")
        };
        spans.push(Span::styled(
            format!("{icon} "),
            Style::default().fg(col).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{label}  ({:.1}s)", last.duration.as_secs_f64()),
            Style::default().fg(MUTED),
        ));
    } else {
        spans.push(Span::styled("no build yet", Style::default().fg(DIM)));
    }

    Paragraph::new(Line::from(spans)).style(Style::default().bg(BG))
}

// ── Row 2: Logcat status + package filter ────────────────────────────────────
fn row2(app: &App) -> Paragraph<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    spans.push(Span::raw("  "));

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
            format!("logcat  {c} lines"),
            Style::default().fg(Color::Rgb(140, 220, 140)),
        ));

        if app.log_scroll > 0 {
            spans.push(Span::styled(
                format!("  ↑ +{}", app.log_scroll),
                Style::default().fg(DIM),
            ));
        }
    } else {
        spans.push(Span::styled("○ logcat off", Style::default().fg(DIM)));
    }

    spans.push(sep());

    spans.push(Span::raw("  "));
    spans.push(Span::styled("pkg  ", Style::default().fg(DIM)));
    if app.show_all_logs {
        spans.push(Span::styled("all processes", Style::default().fg(DIM)));
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

    if app.devices.len() > 1 {
        spans.push(sep());
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} devices", app.devices.len()),
            Style::default().fg(DIM),
        ));
    }

    Paragraph::new(Line::from(spans)).style(Style::default().bg(BG2))
}

// ── Row 3: Activity banner / project / toast ──────────────────────────────────
fn row3(app: &App) -> Paragraph<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::raw("  "));

    // Priority 1: toast messages (launch result, errors, etc.)
    if let Some((ref msg, _)) = app.toast {
        spans.push(Span::styled(
            msg.clone(),
            Style::default()
                .fg(Color::Rgb(255, 220, 100))
                .add_modifier(Modifier::BOLD),
        ));
        return Paragraph::new(Line::from(spans)).style(Style::default().bg(BG));
    }

    // Priority 2: build/install in progress — prominent animated banner
    if let Some(ref task) = app.build_task {
        let elapsed = app
            .build_start
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(0);
        let frame = (elapsed as usize) % SPINNER.len();
        let spin = SPINNER[frame];

        let action_label = if task.starts_with("install") {
            "INSTALLING"
        } else {
            "BUILDING"
        };
        let label_with_launch = if app.launch_after_build {
            format!("{action_label} + LAUNCHING")
        } else {
            action_label.to_string()
        };

        spans.push(Span::styled(
            format!(" {spin} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label_with_launch} "),
            Style::default()
                .fg(Color::Rgb(255, 220, 80))
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {} ", short_task(task)),
            Style::default().fg(Color::Rgb(200, 190, 140)),
        ));
        spans.push(Span::styled(
            format!(" {elapsed}s"),
            Style::default().fg(DIM),
        ));

        return Paragraph::new(Line::from(spans))
            .style(Style::default().bg(Color::Rgb(30, 28, 10)));
    }

    // Priority 3: project path + filter hint
    let path = app
        .project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();
    spans.push(Span::styled("project  ", Style::default().fg(DIM)));
    spans.push(Span::styled(
        path,
        Style::default().fg(MUTED),
    ));

    if app.filter_focused {
        spans.push(sep());
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("filter: {}", app.filter_input),
            Style::default().fg(Color::Rgb(255, 200, 100)),
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

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}
