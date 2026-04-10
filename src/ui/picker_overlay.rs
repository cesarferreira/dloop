//! Floating overlay for selecting build variant (opened with 'v').
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    let total = app.picker_variants.len();
    // Size the popup to fit all variants with some padding
    let popup_h = (total as u16 + 4).min(area.height.saturating_sub(4));
    let popup_w = 48_u16.min(area.width.saturating_sub(4));

    let popup_area = centered_rect(popup_w, popup_h, area);

    // Clear the background
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Select Build Variant  ↑↓ Enter ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let items: Vec<ListItem> = app
        .picker_variants
        .iter()
        .enumerate()
        .map(|(i, (label, assemble, _install))| {
            let sel = i == app.picker_cursor;
            let active = assemble == &app.effective_assemble;
            let bg = if sel { Color::Cyan } else { Color::Reset };
            let fg = if sel {
                Color::Black
            } else if active {
                Color::Green
            } else {
                Color::White
            };
            let prefix = if active { "● " } else { "  " };
            ListItem::new(Line::from(vec![Span::styled(
                format!("{prefix}{label}"),
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

/// Return a rect centered within `area` with the given dimensions.
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
