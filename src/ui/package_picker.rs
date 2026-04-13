//! Package picker popup — opened with 'p'. Fuzzy-searches known packages.
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    let filtered = app.filtered_package_list();
    let has_custom_entry = filtered.is_empty() && !app.package_picker_input.is_empty();
    let item_count = (filtered.len() + 2 + usize::from(has_custom_entry)) as u16; // +1 for "All" +1 for input + optional custom row
    let popup_h = (item_count + 4).min(area.height.saturating_sub(6)).max(7);
    let popup_w = 56_u16.min(area.width.saturating_sub(4));
    let popup = centered(popup_w, popup_h, area);

    f.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Filter by Package  type to search  Enter select  Esc cancel ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.height < 3 {
        return;
    }

    use ratatui::layout::{Constraint, Direction, Layout};
    let parts = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    // ── search input ──────────────────────────────────────────────────────
    let cursor_char = if app.package_picker_open { "█" } else { "" };
    let input_line = Line::from(vec![
        Span::styled("  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            &app.package_picker_input,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(cursor_char, Style::default().fg(Color::Cyan)),
    ]);
    f.render_widget(Paragraph::new(input_line), parts[0]);

    // divider
    let divider = "─".repeat(inner.width as usize);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            divider,
            Style::default().fg(Color::Rgb(50, 50, 65)),
        ))),
        parts[1],
    );

    // ── list items: "All" + filtered packages ─────────────────────────────
    let mut items: Vec<ListItem> = Vec::new();

    // "All packages" entry
    {
        let sel = app.package_picker_cursor == 0;
        let active = app.show_all_logs && app.active_package_filter.is_none();
        let prefix = if active { "● " } else { "  " };
        items.push(ListItem::new(Line::from(Span::styled(
            format!("{prefix}All packages"),
            Style::default()
                .fg(if sel {
                    Color::Black
                } else if active {
                    Color::Green
                } else {
                    Color::White
                })
                .bg(if sel { Color::Cyan } else { Color::Reset })
                .add_modifier(if sel {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))));
    }

    for (i, pkg) in filtered.iter().enumerate() {
        let sel = app.package_picker_cursor == i + 1;
        let active = app.active_package_filter.as_deref() == Some(pkg.as_str());
        let prefix = if active { "● " } else { "  " };

        // Highlight the matching substring
        let mut row_spans = vec![Span::raw(prefix.to_string())];
        if !app.package_picker_input.is_empty() {
            row_spans.extend(highlight_match(pkg, &app.package_picker_input));
        } else {
            row_spans.push(Span::raw(pkg.clone()));
        }

        let bg = if sel { Color::Cyan } else { Color::Reset };
        let row_spans: Vec<Span> = row_spans
            .into_iter()
            .map(|s| {
                if sel {
                    Span::styled(
                        s.content.into_owned(),
                        Style::default()
                            .fg(Color::Black)
                            .bg(bg)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    s
                }
            })
            .collect();

        items.push(ListItem::new(Line::from(row_spans)));
    }

    // If input has text but no matches, offer to use the typed text
    if has_custom_entry {
        let sel = app.package_picker_cursor == filtered.len() + 1;
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  Use: {}", app.package_picker_input),
            Style::default()
                .fg(if sel { Color::Black } else { Color::Yellow })
                .bg(if sel { Color::Cyan } else { Color::Reset })
                .add_modifier(if sel {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))));
    }

    f.render_widget(List::new(items), parts[2]);
}

fn highlight_match<'a>(pkg: &'a str, query: &str) -> Vec<Span<'a>> {
    let lower = pkg.to_lowercase();
    let q = query.to_lowercase();
    if let Some(pos) = lower.find(&q) {
        let end = pos + q.len();
        vec![
            Span::styled(&pkg[..pos], Style::default().fg(Color::White)),
            Span::styled(
                &pkg[pos..end],
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(&pkg[end..], Style::default().fg(Color::White)),
        ]
    } else {
        vec![Span::raw(pkg)]
    }
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
