//! Layout: info bar (top) | logcat (full) | status bar (bottom) + popup overlays
mod build_pane;
mod device_popup;
mod info_bar;
mod logcat_pane;
mod package_picker;
mod picker_overlay;
mod status_bar;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::app::App;

/// Height of the top info bar in rows.
const INFO_H: u16 = 3;

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let area = f.area();
    if area.width < 50 || area.height < 10 {
        let block = Block::default()
            .title(" Terminal too small ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));
        f.render_widget(block, area);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(INFO_H),
            Constraint::Min(4),
            Constraint::Length(4),
        ])
        .split(area);

    info_bar::render(f, app, rows[0]);
    logcat_pane::render(f, app, rows[1]);
    status_bar::render(f, app, rows[2]);

    // ── overlays (render on top) ──────────────────────────────────────────
    let any_popup = app.picker_open
        || app.device_picker_open
        || app.build_popup_open
        || app.package_picker_open;

    if any_popup {
        dim_overlay(f, area);
    }

    if app.picker_open {
        picker_overlay::render(f, app, area);
    }
    if app.device_picker_open {
        device_popup::render(f, app, area);
    }
    if app.build_popup_open {
        build_pane::render_popup(f, app, area);
    }
    if app.package_picker_open {
        package_picker::render(f, app, area);
    }
}

/// Render a dark semi-opaque overlay to visually suppress background content.
/// Uses DIM + near-black to create a "behind the modal" effect without `Clear`.
fn dim_overlay(f: &mut Frame<'_>, area: ratatui::layout::Rect) {
    use ratatui::widgets::Paragraph;
    // We fill every cell with a near-black background + the DIM attribute.
    // The terminal will render these cells over the existing content, effectively
    // making the background appear dark/greyed out behind the popup.
    let overlay = Paragraph::new(
        (0..area.height)
            .map(|_| ratatui::text::Line::from(" ".repeat(area.width as usize)))
            .collect::<Vec<_>>(),
    )
    .style(
        Style::default()
            .bg(Color::Rgb(8, 8, 14))
            .add_modifier(Modifier::DIM),
    );
    f.render_widget(overlay, area);
}
