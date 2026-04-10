//! Full-width logcat pane — rustycat-style rendering with word-wrap.
//!
//! Column layout (matches rustycat):
//!   [timestamp 12] [TAG right-aligned 23] [LEVEL 3] [message wraps to edge]
//!
//! Tags are only shown when they change; repeated runs of the same tag show blanks.
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::modules::logcat::{level_style, looks_like_stack_trace, tag_color};

const TAG_WIDTH: usize = 23;
const TS_WIDTH: usize = 12; // "HH:MM:SS.mmm"
// prefix = TS_WIDTH + 1 + TAG_WIDTH + 1 + 3 (level) + 1 = 41
const PREFIX_WIDTH: usize = TS_WIDTH + 1 + TAG_WIDTH + 1 + 3 + 1;

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

    // ── header (3 lines) ─────────────────────────────────────────────────────
    let live_span = if running {
        Span::styled("● LIVE", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
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
        Style::default().fg(if paused || scrolled { Color::Yellow } else { Color::DarkGray }),
    );
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
            Span::styled(&app.filter_input, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
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
    let visible_rows = inner.height.saturating_sub(header_h + 1).max(1) as usize;

    // ── build log lines with wrapping ────────────────────────────────────────
    let msg_width = (inner.width as usize).saturating_sub(PREFIX_WIDTH).max(20);
    let total = app.log_lines.len();

    // Build all visual lines first so we know how many there are for scroll.
    // We process entries in REVERSE from the scroll position to fill `visible_rows`.
    let scroll = app.log_scroll;
    let entry_end = total.saturating_sub(scroll);

    let mut last_tag: String = String::new();

    // Collect entries from the end, building lines until we have enough
    let mut collected: Vec<Line> = Vec::new();
    for entry in app.log_lines[..entry_end].iter().rev() {
        if collected.len() >= visible_rows * 2 {
            // Enough pre-collected; trim later
            break;
        }
        let lc = &entry.level;
        let lvl_color = level_style(lc);
        let tc = tag_color(&entry.tag, &mut app.tag_color_cache);
        let is_stack = looks_like_stack_trace(&entry.message) || looks_like_stack_trace(&entry.raw);
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
            "E" | "F" => Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD),
            "W" => Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
            _ => Style::default().fg(lvl_color).add_modifier(Modifier::BOLD),
        };

        // Word-wrap the message into chunks
        let chunks = word_wrap(&entry.message, msg_width);

        // Build lines in REVERSE (since we're iterating entries in reverse)
        for (ci, chunk) in chunks.into_iter().enumerate().rev() {
            let is_first_chunk = ci == 0;
            if is_first_chunk {
                // Show timestamp + tag + level
                let show_tag = entry.tag != last_tag;
                let tag_display = if show_tag {
                    right_pad(&entry.tag, TAG_WIDTH)
                } else {
                    " ".repeat(TAG_WIDTH)
                };
                let tag_style = if show_tag {
                    Style::default().fg(tc)
                } else {
                    Style::default().fg(Color::DarkGray)
                };

                collected.push(Line::from(vec![
                    Span::styled(
                        format!("{:<width$} ", entry.timestamp, width = TS_WIDTH),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(tag_display, tag_style),
                    Span::raw(" "),
                    Span::styled(format!(" {} ", lc), lvl_style_span),
                    Span::raw(" "),
                    Span::styled(chunk, Style::default().fg(msg_fg).add_modifier(msg_modifier)),
                ]));
            } else {
                // Continuation line: blank prefix + chunk
                let blank_prefix = " ".repeat(TS_WIDTH + 1 + TAG_WIDTH + 1 + 3 + 1);
                collected.push(Line::from(vec![
                    Span::raw(blank_prefix),
                    Span::styled(chunk, Style::default().fg(msg_fg).add_modifier(msg_modifier)),
                ]));
            }
        }

        // Update last_tag after processing this entry (we're going in reverse)
        last_tag = entry.tag.clone();
    }

    // Reverse collected (we built it bottom-up) and take last `visible_rows`
    collected.reverse();
    let start = collected.len().saturating_sub(visible_rows);
    let mut lines = header;

    if total == 0 {
        lines.push(Line::from(Span::styled(
            if running { "Waiting for log lines…" } else { "Press l to start logcat" },
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        lines.extend_from_slice(&collected[start..]);
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Word-wrap `msg` into lines of at most `max_chars` characters.
/// Breaks at word boundaries where possible.
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
        let char_count = remaining.chars().count();
        if char_count <= max_chars {
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

/// Right-pad a string to `width` chars, truncating with no ellipsis if too long.
fn right_pad(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count >= width {
        // Clip without ellipsis to avoid confusion with actual content
        s.chars().take(width).collect()
    } else {
        format!("{s}{}", " ".repeat(width - count))
    }
}
