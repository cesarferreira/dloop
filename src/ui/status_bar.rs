//! Footer: dim status line + pastel key caps (labels beside each key).
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;

/// Dark text on pastel background (key cap).
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

fn status_text(app: &App) -> String {
    if let Some(ref task) = app.build_task {
        return format!("Building: {task} …");
    }
    if app.log_scroll > 0 {
        return format!("Scrolled ↑{}  —  End or G to follow tail", app.log_scroll);
    }
    if app.logcat_paused {
        return "Logcat PAUSED  —  Space to resume".to_string();
    }
    let n = app.devices.len();
    if n == 0 {
        return "No devices — connect one or press r to refresh".to_string();
    }
    format!(
        "{} device{}  ·  {} log lines",
        n,
        if n == 1 { "" } else { "s" },
        app.log_lines.len()
    )
}

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    // Line 1: dim status (toast overrides when present)
    let line1 = if let Some((msg, _)) = &app.toast {
        Line::from(vec![Span::styled(
            msg.as_str(),
            Style::default().fg(Color::Rgb(250, 179, 135)),
        )])
    } else {
        Line::from(vec![Span::styled(
            status_text(app),
            Style::default().fg(Color::Rgb(108, 112, 134)),
        )])
    };

    // Line 2: essential shortcuts only — press ? for the full list
    let c = |k: &str, d: &str, bg: Color| vec![cap(k, bg), label(d)];
    let mut spans: Vec<Span> = Vec::new();
    spans.extend(c("b", "build", Color::Rgb(137, 180, 250)));
    spans.extend(c("i", "install", Color::Rgb(245, 194, 231)));
    spans.extend(c("n", "run", Color::Rgb(166, 227, 161)));
    spans.extend(c("l", "logcat", Color::Rgb(203, 166, 247)));
    spans.extend(c("q", "quit", Color::Rgb(148, 226, 213)));
    spans.extend(c("?", "help", Color::Rgb(249, 226, 175)));

    let line2 = Line::from(spans);

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Rgb(86, 95, 137)));

    let p = Paragraph::new(vec![line1, line2]).block(block);
    f.render_widget(p, area);
}
