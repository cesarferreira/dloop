//! Full-screen logcat pane — rustycat-style rendering with word-wrap.
//!
//! Column layout:  [timestamp 12] [TAG right-aligned 23] [LEVEL 3] [message wraps]
//! Tags are suppressed on repeated runs (rustycat style).
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::modules::logcat::{level_style, looks_like_stack_trace, tag_color};

const TAG_WIDTH: usize = 23;
const TS_WIDTH: usize = 12;
// prefix = TS_WIDTH + 1(space) + TAG_WIDTH + 1(space) + 3(level) + 1(space) = 41
const PREFIX_WIDTH: usize = TS_WIDTH + 1 + TAG_WIDTH + 1 + 3 + 1;

pub fn render(f: &mut Frame<'_>, app: &mut App, area: Rect) {
    let paused = app.logcat_paused;
    let running = app.logcat_running;
    let scrolled = app.log_scroll > 0;

    let title = if app.filter_focused {
        " Logcat  [FILTER] "
    } else if paused {
        " Logcat  PAUSED "
    } else if scrolled {
        " Logcat  ↑scrolled — End to tail "
    } else {
        " Logcat "
    };

    let border_style = Style::default().fg(if app.filter_focused {
        Color::Cyan
    } else {
        Color::Rgb(50, 50, 70)
    });

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border_style);
    let inner = block.inner(area);
    f.render_widget(block, area);

    // ── filter bar (only when active) ────────────────────────────────────
    let filter_line: Option<Line> = if app.filter_focused || !app.filter_input.is_empty() {
        Some(if app.filter_focused {
            Line::from(vec![
                Span::styled(" filter: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &app.filter_input,
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
                ),
                Span::styled("█", Style::default().fg(Color::Cyan)),
            ])
        } else {
            Line::from(vec![
                Span::styled(" filter: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&app.filter_input, Style::default().fg(Color::Cyan)),
            ])
        })
    } else {
        None
    };

    let header_h = filter_line.is_some() as u16;
    let visible_rows = inner.height.saturating_sub(header_h).max(1) as usize;
    let msg_width = (inner.width as usize).saturating_sub(PREFIX_WIDTH).max(20);

    let total = app.log_lines.len();
    let scroll = app.log_scroll.min(total.saturating_sub(visible_rows));
    let entry_end = total.saturating_sub(scroll);

    let mut collected: Vec<Line> = Vec::new();
    let mut last_tag = String::new();

    // Iterate entries from bottom upward, collecting visual lines
    for entry in app.log_lines[..entry_end].iter().rev() {
        if collected.len() >= visible_rows * 3 {
            break;
        }
        let lc = &entry.level;
        let lvl_color = level_style(lc);
        let tc = tag_color(&entry.tag, &mut app.tag_color_cache);
        let is_stack =
            looks_like_stack_trace(&entry.message) || looks_like_stack_trace(&entry.raw);
        let msg_fg = if is_stack {
            Color::LightRed
        } else {
            match lc.as_str() {
                "E" | "F" => Color::LightRed,
                "W" => Color::Yellow,
                _ => Color::White,
            }
        };
        let msg_modifier = if is_stack || matches!(lc.as_str(), "E" | "F") {
            Modifier::BOLD
        } else {
            Modifier::empty()
        };
        let lvl_style_span = match lc.as_str() {
            "E" | "F" => {
                Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD)
            }
            "W" => Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            _ => Style::default().fg(lvl_color).add_modifier(Modifier::BOLD),
        };

        let chunks = word_wrap(&entry.message, msg_width);

        for (ci, chunk) in chunks.into_iter().enumerate().rev() {
            if ci == 0 {
                let show_tag = entry.tag != last_tag;
                let tag_display = if show_tag {
                    right_pad(&entry.tag, TAG_WIDTH)
                } else {
                    " ".repeat(TAG_WIDTH)
                };
                let tag_style = if show_tag {
                    Style::default().fg(tc)
                } else {
                    Style::default().fg(Color::Rgb(40, 40, 55))
                };
                collected.push(Line::from(vec![
                    Span::styled(
                        format!("{:<width$} ", entry.timestamp, width = TS_WIDTH),
                        Style::default().fg(Color::Rgb(80, 80, 100)),
                    ),
                    Span::styled(tag_display, tag_style),
                    Span::raw(" "),
                    Span::styled(format!(" {} ", lc), lvl_style_span),
                    Span::raw(" "),
                    Span::styled(chunk, Style::default().fg(msg_fg).add_modifier(msg_modifier)),
                ]));
            } else {
                let blank = " ".repeat(PREFIX_WIDTH);
                collected.push(Line::from(vec![
                    Span::raw(blank),
                    Span::styled(chunk, Style::default().fg(msg_fg).add_modifier(msg_modifier)),
                ]));
            }
        }
        last_tag = entry.tag.clone();
    }

    collected.reverse();
    let start = collected.len().saturating_sub(visible_rows);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(fl) = filter_line {
        lines.push(fl);
    }

    if total == 0 {
        lines.push(Line::from(Span::styled(
            if running {
                " Waiting for log lines…"
            } else {
                " Press l to start logcat"
            },
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.extend_from_slice(&collected[start..]);
    }

    f.render_widget(Paragraph::new(lines), inner);
}

fn word_wrap(msg: &str, max_chars: usize) -> Vec<String> {
    if max_chars == 0 {
        return vec![msg.to_string()];
    }
    if msg.chars().count() <= max_chars {
        return vec![msg.to_string()];
    }
    let mut lines = Vec::new();
    let mut remaining = msg;
    while !remaining.is_empty() {
        if remaining.chars().count() <= max_chars {
            lines.push(remaining.to_string());
            break;
        }
        let slice: String = remaining.chars().take(max_chars).collect();
        let split_at = slice.rfind(' ').unwrap_or(max_chars);
        let byte_split = remaining
            .char_indices()
            .nth(split_at)
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());
        lines.push(remaining[..byte_split].to_string());
        remaining = remaining[byte_split..].trim_start_matches(' ');
    }
    lines
}

fn right_pad(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count >= width {
        s.chars().take(width).collect()
    } else {
        format!("{s}{}", " ".repeat(width - count))
    }
}
