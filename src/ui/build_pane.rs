//! Build output popup — opened with 'e'.
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render_popup(f: &mut Frame<'_>, app: &App, area: Rect) {
    let popup_h = (area.height * 2 / 3)
        .max(10)
        .min(area.height.saturating_sub(4));
    let popup_w = (area.width * 4 / 5)
        .max(60)
        .min(area.width.saturating_sub(4));
    let popup = centered(popup_w, popup_h, area);

    f.render_widget(Clear, popup);

    let (status_text, status_color) = if let Some(ref t) = app.build_task {
        (format!("▶ {t}"), Color::Yellow)
    } else if let Some(last) = app.build_history.last() {
        let icon = if last.exit_code == Some(0) {
            "✓"
        } else {
            "✗"
        };
        let col = if last.exit_code == Some(0) {
            Color::Green
        } else {
            Color::Red
        };
        (
            format!("{icon} {} ({:.1}s)", last.task, last.duration.as_secs_f64()),
            col,
        )
    } else {
        ("No build yet".to_string(), Color::DarkGray)
    };

    let countdown = app.build_popup_auto_close.map(|deadline| {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        format!("  closing in {}s", remaining.as_secs() + 1)
    });
    let title = if let Some(cd) = countdown {
        format!(" Build  {status_text}{cd}  ↑↓ scroll  Esc close ")
    } else {
        format!(" Build  {status_text}  ↑↓ scroll  Esc close ")
    };
    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    let w = inner.width.saturating_sub(1) as usize;
    let total = app.build_lines.len();

    if total == 0 {
        let p = Paragraph::new(Line::from(Span::styled(
            "No build output yet — press b to build",
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(p, inner);
        return;
    }

    let visible = inner.height as usize;
    let scroll = app.build_popup_scroll.min(total.saturating_sub(visible));
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(visible);

    let lines: Vec<Line> = app.build_lines[start..end]
        .iter()
        .map(|raw| {
            let text = raw
                .trim_start_matches("[stdout] ")
                .trim_start_matches("[stderr] ");
            let fg = if raw.contains("[stderr]") {
                Color::LightRed
            } else {
                Color::Gray
            };
            let display: String = text.chars().take(w).collect();
            Line::from(Span::styled(display, Style::default().fg(fg)))
        })
        .collect();

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
