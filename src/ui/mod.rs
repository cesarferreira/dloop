//! Layout: narrow sidebar (devices + build) | wide logcat + optional picker overlay
mod build_pane;
mod devices_pane;
mod logcat_pane;
mod picker_overlay;
mod status_bar;

use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::app::{App, Pane};

const SIDEBAR_W: u16 = 26;

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
        .constraints([Constraint::Min(6), Constraint::Length(4)])
        .split(area);

    let sidebar_w = SIDEBAR_W.min(area.width / 4);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(sidebar_w), Constraint::Min(20)])
        .split(rows[0]);

    let build_h = sidebar_build_height(app, cols[0].height);
    let device_h = cols[0].height.saturating_sub(build_h);
    let sidebar_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(device_h), Constraint::Length(build_h)])
        .split(cols[0]);

    let d_style = pane_border_style(app.active_pane == Pane::Devices);
    let b_style = pane_border_style(app.active_pane == Pane::Build);
    let l_style = pane_border_style(app.active_pane == Pane::Logs);

    devices_pane::render(f, app, sidebar_rows[0], d_style);
    build_pane::render_compact(f, app, sidebar_rows[1], b_style);
    logcat_pane::render(f, app, cols[1], l_style);
    status_bar::render(f, app, rows[1]);

    // Picker overlay is rendered on top of everything
    if app.picker_open {
        picker_overlay::render(f, app, area);
    }
}

fn sidebar_build_height(app: &App, total_h: u16) -> u16 {
    if app.build_expanded {
        // Take up to half the sidebar
        (total_h / 2).max(6)
    } else if app.build_child.is_some() {
        6_u16.min(total_h / 3)
    } else {
        4_u16.min(total_h / 4)
    }
}

fn pane_border_style(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
