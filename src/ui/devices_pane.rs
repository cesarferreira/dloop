//! Left-sidebar top section: compact device list.
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame<'_>, app: &mut App, area: Rect, border: Style) {
    let block = Block::default()
        .title(" Devices ")
        .borders(Borders::ALL)
        .border_style(border);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut items: Vec<ListItem> = Vec::new();

    if app.devices.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            "None — press r",
            Style::default().fg(Color::Yellow),
        ))));
    } else {
        let w = inner.width.saturating_sub(3) as usize;
        for (i, d) in app.devices.iter().enumerate() {
            let sel = i == app.selected_device;
            let bg = if sel { Color::Cyan } else { Color::Reset };
            let fg = if sel { Color::Black } else { Color::White };
            let style = Style::default().fg(fg).bg(bg).add_modifier(if sel {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });

            // Model (truncated to fit)
            let label = truncate(&d.model, w);
            items.push(ListItem::new(Line::from(Span::styled(
                format!("{} {label}", if sel { "▶" } else { " " }),
                style,
            ))));

            // Serial + state in one dim line
            let serial = truncate(&d.serial, w.saturating_sub(2));
            items.push(ListItem::new(Line::from(Span::styled(
                format!("  {serial}"),
                Style::default().fg(Color::DarkGray),
            ))));
        }
    }

    f.render_widget(List::new(items), inner);
}

fn truncate(s: &str, max: usize) -> String {
    let max = max.max(1);
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{t}…")
    }
}
