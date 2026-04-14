//! Floating overlay for selecting the active log level filter (opened with 'L').
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};
use ratatui::Frame;

use crate::app::{App, LEVEL_FILTER_OPTIONS};

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    let total = LEVEL_FILTER_OPTIONS.len();
    let popup_h = (total as u16 + 4).min(area.height.saturating_sub(4));
    let popup_w = 54_u16.min(area.width.saturating_sub(4));
    let popup_area = centered_rect(popup_w, popup_h, area);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Filter Log Levels  ↑↓ Enter ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(244, 162, 97)));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let items: Vec<ListItem> = LEVEL_FILTER_OPTIONS
        .iter()
        .enumerate()
        .map(|(i, mode)| {
            let sel = i == app.level_picker_cursor;
            let active = *mode == app.level_filter_mode;
            let bg = if sel {
                Color::Rgb(244, 162, 97)
            } else {
                Color::Reset
            };
            let fg = if sel {
                Color::Black
            } else if active {
                Color::Green
            } else {
                Color::White
            };
            let prefix = if active { "● " } else { "  " };
            ListItem::new(Line::from(vec![Span::styled(
                format!("{prefix}{}", mode.title()),
                Style::default().fg(fg).bg(bg).add_modifier(if sel {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
            )]))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;
    Rect {
        x,
        y,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
