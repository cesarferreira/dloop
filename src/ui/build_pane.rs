//! Left-sidebar build status strip — compact or expanded (e to toggle).
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render_compact(f: &mut Frame<'_>, app: &App, area: Rect, border: Style) {
    let expanded = app.build_expanded;
    let indicator = if expanded { "▾" } else { "▸" };
    let block = Block::default()
        .title(format!(" Build {indicator} "))
        .borders(Borders::ALL)
        .border_style(border);
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let w = inner.width.saturating_sub(1) as usize;

    let mut lines: Vec<Line> = Vec::new();

    // ── status / variant line ────────────────────────────────────────────────
    let (status_text, status_color) = if let Some(ref t) = app.build_task {
        (format!("▶ {}", short_task(t)), Color::Yellow)
    } else if let Some(last) = app.build_history.last() {
        let icon = if last.exit_code == Some(0) { "✓" } else { "✗" };
        let col = if last.exit_code == Some(0) { Color::Green } else { Color::Red };
        let secs = last.duration.as_secs_f64();
        (format!("{icon} {} ({secs:.1}s)", short_task(&last.task)), col)
    } else {
        (app.inference.variant_summary.clone(), Color::DarkGray)
    };
    lines.push(Line::from(Span::styled(
        trunc(&status_text, w),
        Style::default().fg(status_color).add_modifier(Modifier::BOLD),
    )));

    // ── build output lines (visible when expanded OR building) ───────────────
    let show_output = expanded || app.build_child.is_some();
    if show_output && inner.height > 1 {
        let available = inner.height.saturating_sub(1) as usize;
        let total = app.build_lines.len();
        let start = total.saturating_sub(available);
        for raw in app.build_lines[start..total].iter() {
            let text = raw
                .trim_start_matches("[stdout] ")
                .trim_start_matches("[stderr] ");
            let fg = if raw.contains("[stderr]") { Color::LightRed } else { Color::Gray };
            lines.push(Line::from(Span::styled(trunc(text, w), Style::default().fg(fg))));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
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

fn trunc(s: &str, max: usize) -> String {
    let max = max.max(1);
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}
