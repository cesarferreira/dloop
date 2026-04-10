//! Full-width logcat pane with scrolling and package-filter toggle.
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::modules::logcat::{level_style, looks_like_stack_trace, tag_color};

pub fn render(f: &mut Frame<'_>, app: &mut App, area: Rect, border: Style) {
    let running = app.logcat_running;
    let paused = app.logcat_paused;
    let scrolled = app.log_scroll > 0;

    let title = if app.filter_focused {
        " Logcat  [FILTER] ".to_string()
    } else if scrolled {
        format!(" Logcat  ↑{} ", app.log_scroll)
    } else {
        " Logcat ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // ── header ────────────────────────────────────────────────────────────────
    let live_span = if running {
        Span::styled(
            "● LIVE",
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled("○ off", Style::default().fg(Color::DarkGray))
    };

    let state_label = if paused {
        "  PAUSED"
    } else if scrolled {
        "  scroll (End = tail)"
    } else {
        "  streaming"
    };
    let state_span = Span::styled(
        state_label,
        Style::default().fg(if paused || scrolled {
            Color::Yellow
        } else {
            Color::DarkGray
        }),
    );

    // Package filter indicator
    let pkg_span = if !app.effective_packages.is_empty() {
        if app.show_all_logs {
            Span::styled("  [all]", Style::default().fg(Color::DarkGray))
        } else {
            Span::styled("  [pkg]", Style::default().fg(Color::Cyan))
        }
    } else {
        Span::raw("")
    };

    let filter_line = if app.filter_focused {
        Line::from(vec![
            Span::styled("filter: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &app.filter_input,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("█", Style::default().fg(Color::Cyan)),
        ])
    } else if !app.filter_input.is_empty() {
        Line::from(vec![
            Span::styled("filter: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.filter_input, Style::default().fg(Color::Cyan)),
        ])
    } else {
        Line::from(Span::styled("no filter", Style::default().fg(Color::DarkGray)))
    };

    let header = vec![
        Line::from(vec![live_span, state_span, pkg_span]),
        filter_line,
        Line::from(""),
    ];
    let header_h = header.len() as u16;

    // ── log lines with scroll ─────────────────────────────────────────────────
    let visible = inner.height.saturating_sub(header_h + 1).max(1) as usize;
    let total = app.log_lines.len();

    // scroll=0 → tail; scroll=N → N lines from end
    let scroll = app.log_scroll.min(total.saturating_sub(visible));
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(visible);

    let mut lines = header;

    if total == 0 {
        lines.push(Line::from(Span::styled(
            if running {
                "Waiting for log lines…"
            } else {
                "Press l to start logcat"
            },
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for entry in app.log_lines[start..end].iter() {
            let lc = &entry.level;
            let lvl_color = level_style(lc);
            let tc = tag_color(&entry.tag, &mut app.tag_color_cache);
            let is_stack =
                looks_like_stack_trace(&entry.message) || looks_like_stack_trace(&entry.raw);
            let msg_style = if is_stack {
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let (lvl_style, tag_style) = match lc.as_str() {
                "E" | "F" => (
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Red)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(tc),
                ),
                "W" => (
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(tc),
                ),
                _ => (
                    Style::default().fg(lvl_color).add_modifier(Modifier::BOLD),
                    Style::default().fg(tc),
                ),
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:>8} ", entry.timestamp),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(format!(" {} ", lc), lvl_style),
                Span::styled(
                    format!(" {:12} ", trunc_tag(&entry.tag, 12)),
                    tag_style,
                ),
                // No manual truncation — ratatui clips at widget boundary
                Span::styled(entry.message.clone(), msg_style),
            ]));
        }
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn trunc_tag(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}
