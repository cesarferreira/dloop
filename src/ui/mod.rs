//! Layout: info bar (top) | logcat (full) | status bar (bottom) + popup overlays
mod build_pane;
mod device_popup;
mod info_bar;
mod logcat_pane;
mod package_picker;
mod picker_overlay;
mod status_bar;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::app::App;

pub fn draw(f: &mut Frame<'_>, app: &mut App) {
    let area = f.area();
    if area.width < 50 || area.height < 8 {
        let block = Block::default()
            .title(" Terminal too small ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red));
        f.render_widget(block, area);
        return;
    }

    // [ info bar  1 row ]
    // [ logcat    fill  ]
    // [ status    4 rows]
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(4),
        ])
        .split(area);

    info_bar::render(f, app, rows[0]);
    logcat_pane::render(f, app, rows[1]);
    status_bar::render(f, app, rows[2]);

    // ── overlays (render on top) ──────────────────────────────────────────
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
