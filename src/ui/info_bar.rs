//! Single-line top info bar: device · variant · build status · log count · package filter
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    let dim = Style::default().fg(Color::Rgb(90, 90, 110));
    let sep = "│";
    let sep_style = Style::default().fg(Color::Rgb(50, 50, 65));

    let mut parts: Vec<String> = Vec::new();
    let mut styles: Vec<Style> = Vec::new();

    macro_rules! push {
        ($text:expr, $style:expr) => {
            parts.push($text.to_string());
            styles.push($style);
        };
    }
    macro_rules! sep {
        () => {
            push!(sep, sep_style);
        };
    }

    // ── device ────────────────────────────────────────────────────────────
    if app.devices.is_empty() {
        push!(" no device  d to pick ", Style::default().fg(Color::Yellow));
    } else {
        let d = &app.devices[app.selected_device];
        let serial_short: String = d.serial.chars().take(8).collect();
        push!(
            format!("  {}  [{}…] ", d.model, serial_short),
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        );
    }
    sep!();

    // ── variant ───────────────────────────────────────────────────────────
    let variant = short_task(&app.effective_assemble).to_string();
    push!(
        format!(" {variant} "),
        Style::default().fg(Color::Rgb(180, 190, 254))
    );
    sep!();

    // ── build status ──────────────────────────────────────────────────────
    if let Some(ref task) = app.build_task {
        push!(
            format!(" ▶ {} ", short_task(task)),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        );
    } else if let Some(last) = app.build_history.last() {
        let (icon, col) = if last.exit_code == Some(0) {
            ("✓", Color::Green)
        } else {
            ("✗", Color::Red)
        };
        push!(
            format!(" {icon} {:.1}s ", last.duration.as_secs_f64()),
            Style::default().fg(col)
        );
    } else {
        push!(" idle ", dim);
    }
    sep!();

    // ── logcat ────────────────────────────────────────────────────────────
    if app.logcat_running {
        let count = app.log_lines.len();
        let c = if count >= 1_000 {
            format!("{}k", count / 1000)
        } else {
            count.to_string()
        };
        push!(
            format!(" ● {c} lines "),
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        );
    } else {
        push!(" ○ off ", dim);
    }
    sep!();

    // ── package filter ────────────────────────────────────────────────────
    if app.show_all_logs {
        push!(" all logs ", dim);
    } else {
        let pkg = app
            .active_package_filter
            .clone()
            .or_else(|| app.effective_packages.first().cloned())
            .unwrap_or_else(|| "?".to_string());
        push!(format!(" {pkg} "), Style::default().fg(Color::Cyan));
    }

    let spans: Vec<Span<'static>> = parts
        .into_iter()
        .zip(styles)
        .map(|(t, s)| Span::styled(t, s))
        .collect();

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Rgb(20, 20, 30))),
        area,
    );
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
