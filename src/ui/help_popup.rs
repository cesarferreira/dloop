//! Help popup — opened with `?`, shows all keybindings grouped by category.
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

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
        format!(" {text}"),
        Style::default().fg(Color::Rgb(205, 214, 244)),
    )
}

fn section(title: &'static str) -> Line<'static> {
    Line::from(Span::styled(
        format!(" {title}"),
        Style::default()
            .fg(Color::Rgb(148, 226, 213))
            .add_modifier(Modifier::BOLD),
    ))
}

fn blank() -> Line<'static> {
    Line::from("")
}

fn row(key: &'static str, desc: &'static str, bg: Color) -> Line<'static> {
    Line::from(vec![Span::raw("  "), cap(key, bg), label(desc)])
}

pub fn render(f: &mut Frame<'_>, _app: &App, area: Rect) {
    let lines: Vec<Line> = vec![
        section("BUILD"),
        row("b", "build", Color::Rgb(137, 180, 250)),
        row("i", "install", Color::Rgb(245, 194, 231)),
        row("r", "run (install + launch)", Color::Rgb(166, 227, 161)),
        row("s", "stop", Color::Rgb(243, 139, 168)),
        row("e", "build log", Color::Rgb(137, 220, 235)),
        row("H", "build history", Color::Rgb(180, 190, 254)),
        blank(),
        section("LOGCAT"),
        row("l", "toggle logcat", Color::Rgb(203, 166, 247)),
        row("L", "log levels", Color::Rgb(244, 162, 97)),
        row("f", "filter", Color::Rgb(249, 226, 175)),
        row("x", "exclude", Color::Rgb(250, 179, 135)),
        row("a", "all / package toggle", Color::Rgb(249, 226, 175)),
        row("c", "clear logs", Color::Rgb(137, 220, 235)),
        row("Space", "pause / resume", Color::Rgb(205, 214, 244)),
        row("w", "export logs", Color::Rgb(166, 218, 149)),
        blank(),
        section("APP"),
        row("u", "uninstall", Color::Rgb(243, 139, 168)),
        row("C", "clear app data", Color::Rgb(250, 179, 135)),
        row("T", "clear app cache", Color::Rgb(249, 226, 175)),
        blank(),
        section("DEVICE & MISC"),
        row("d", "device picker", Color::Rgb(148, 226, 213)),
        row("v", "build variant", Color::Rgb(180, 190, 254)),
        row("p", "package filter", Color::Rgb(249, 226, 175)),
        row("m", "scrcpy mirror", Color::Rgb(180, 190, 254)),
        row("y", "crash details", Color::Rgb(243, 188, 219)),
        row("q", "quit", Color::Rgb(148, 226, 213)),
    ];

    let content_h = lines.len() as u16 + 2;
    let popup_h = content_h.min(area.height.saturating_sub(4));
    let popup_w = 48u16.min(area.width.saturating_sub(4));
    let popup = centered(popup_w, popup_h, area);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .title(Span::styled(
            " Keybindings  Esc close ",
            Style::default()
                .fg(Color::Rgb(137, 180, 250))
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(137, 180, 250)));

    let inner = block.inner(popup);
    f.render_widget(block, popup);
    f.render_widget(Paragraph::new(lines), inner);
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
