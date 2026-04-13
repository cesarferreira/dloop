//! Crash detail popup — opened with `y` when a crash exists.
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::modules::logcat::LogEntry;

fn line_style(entry: &LogEntry) -> Style {
    let m = entry.message.to_lowercase();
    let r = entry.raw.to_lowercase();
    if m.contains("fatal exception")
        || r.contains("fatal exception")
        || m.contains("anr in ")
        || r.contains("anr in ")
    {
        return Style::default()
            .fg(Color::LightRed)
            .add_modifier(Modifier::BOLD);
    }
    if m.contains("caused by:") || r.contains("caused by:") {
        return Style::default().fg(Color::LightRed);
    }
    if m.trim_start().starts_with("at ") {
        return Style::default().fg(Color::Yellow);
    }
    Style::default().fg(Color::Gray)
}

fn cap(key: &str, bg: Color) -> Span<'static> {
    let s: String = format!(" {} ", key);
    Span::styled(
        s,
        Style::default()
            .fg(Color::Black)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )
}

fn label(text: &str) -> Span<'static> {
    Span::styled(
        format!(" {text}   "),
        Style::default().fg(Color::Rgb(205, 214, 244)),
    )
}

pub fn render_popup(f: &mut Frame<'_>, app: &App, area: Rect) {
    let Some(crash) = app.crash_events.last() else {
        return;
    };

    let popup_h = (area.height * 2 / 3)
        .max(12)
        .min(area.height.saturating_sub(4));
    let popup_w = (area.width * 4 / 5)
        .max(60)
        .min(area.width.saturating_sub(4));
    let popup = centered(popup_w, popup_h, area);

    f.render_widget(Clear, popup);

    let title = format!(
        " Crash Details  {} @ {}  ↑↓ scroll  Esc close ",
        truncate(&crash.summary, 40),
        crash.timestamp
    );
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::LightRed)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(243, 139, 168)));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(4), Constraint::Length(1)])
        .split(inner);

    let body_area = rows[0];
    let footer_area = rows[1];

    let total = crash.lines.len();
    let w = body_area.width.saturating_sub(1) as usize;
    let visible = body_area.height as usize;

    let scroll = app.crash_detail_scroll.min(total.saturating_sub(visible));
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(visible);

    let lines: Vec<Line> = crash.lines[start..end]
        .iter()
        .map(|entry| {
            let text: String = entry.raw.chars().take(w).collect();
            Line::from(Span::styled(text, line_style(entry)))
        })
        .collect();

    f.render_widget(Paragraph::new(lines), body_area);

    let mut spans: Vec<Span> = Vec::new();
    let c = |k: &str, d: &str, bg: Color| vec![cap(k, bg), label(d)];
    spans.extend(c("c", "copy", Color::Rgb(243, 188, 219)));
    spans.extend(c("a", "agent", Color::Rgb(166, 227, 161)));
    spans.extend(c("w", "export", Color::Rgb(137, 180, 250)));
    spans.extend(c("s", "search", Color::Rgb(249, 226, 175)));
    let footer = Paragraph::new(Line::from(spans));
    f.render_widget(footer, footer_area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
