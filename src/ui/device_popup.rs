//! Device picker popup — opened with 'd'.
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    let n = app.devices.len().max(1);
    let popup_h = (n as u16 + 4).min(area.height.saturating_sub(6));
    let popup_w = 46_u16.min(area.width.saturating_sub(4));
    let popup = centered(popup_w, popup_h, area);

    f.render_widget(Clear, popup);
    let block = Block::default()
        .title(" Select Device  ↑↓ Enter ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if app.devices.is_empty() {
        f.render_widget(
            List::new(vec![ListItem::new(Line::from(Span::styled(
                "No devices connected",
                Style::default().fg(Color::Yellow),
            )))]),
            inner,
        );
        return;
    }

    let items: Vec<ListItem> = app
        .devices
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let sel = i == app.device_picker_cursor;
            let active = i == app.selected_device;
            let bg = if sel { Color::Cyan } else { Color::Reset };
            let fg = if sel {
                Color::Black
            } else if active {
                Color::Green
            } else {
                Color::White
            };
            let prefix = if active { "● " } else { "  " };
            let serial: String = d.serial.chars().take(16).collect();
            ListItem::new(Line::from(vec![Span::styled(
                format!("{prefix}{} [{serial}]", d.model),
                Style::default()
                    .fg(fg)
                    .bg(bg)
                    .add_modifier(if sel { Modifier::BOLD } else { Modifier::empty() }),
            )]))
        })
        .collect();

    f.render_widget(List::new(items), inner);
}

fn centered(width: u16, height: u16, area: Rect) -> Rect {
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width: width.min(area.width),
        height: height.min(area.height),
    }
}
