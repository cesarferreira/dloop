//! Overlay listing recent Gradle build results (opened with `H`).
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;

pub fn render(f: &mut Frame<'_>, app: &App, area: Rect) {
    let hist = &app.build_history;
    let n = hist.len();
    let popup_h = (n as u16 + 8).min(area.height.saturating_sub(4)).max(10);
    let popup_w = (area.width.saturating_sub(4)).min(72);

    let popup_area = centered_rect(popup_w, popup_h, area);
    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Build history  Esc/q close  j/k scroll ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(180, 190, 254)));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if n == 0 {
        let p = Paragraph::new(Line::from(Span::styled(
            "No builds yet — use b / i / n",
            Style::default().fg(Color::DarkGray),
        )));
        f.render_widget(p, inner);
        return;
    }

    let max_rows = inner.height.saturating_sub(4) as usize;
    let rev: Vec<_> = hist.iter().rev().collect();
    let scroll = app.build_history_scroll.min(rev.len().saturating_sub(1));

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![Span::styled(
        format!("{:<4} {:<28} {:<10} {:>8}", "#", "Task", "Result", "Time"),
        Style::default()
            .fg(Color::Rgb(150, 160, 200))
            .add_modifier(Modifier::BOLD),
    )]));
    let sep_w = (inner.width as usize).min(64);
    lines.push(Line::from(Span::styled(
        "─".repeat(sep_w),
        Style::default().fg(Color::Rgb(60, 60, 80)),
    )));

    for (i, rec) in rev.iter().enumerate().skip(scroll).take(max_rows) {
        let display_num = i + 1;
        let ok = rec.exit_code == Some(0);
        let (res, res_col) = if ok {
            ("ok", Color::Rgb(130, 230, 130))
        } else {
            ("fail", Color::Rgb(230, 90, 90))
        };
        let task = truncate(&rec.task, 26);
        let dur = format!("{:.1}s", rec.duration.as_secs_f64());
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<4} ", display_num),
                Style::default().fg(Color::Rgb(120, 130, 160)),
            ),
            Span::styled(
                format!("{:<28} ", task),
                Style::default().fg(Color::Rgb(220, 220, 240)),
            ),
            Span::styled(
                format!("{:<10} ", res),
                Style::default().fg(res_col).add_modifier(Modifier::BOLD),
            ),
            Span::styled(dur, Style::default().fg(Color::Rgb(180, 180, 200))),
        ]));
    }

    let sum_dur: f64 = hist.iter().map(|r| r.duration.as_secs_f64()).sum();
    let avg = sum_dur / n as f64;
    let last = hist.last().map(|r| r.duration.as_secs_f64()).unwrap_or(0.0);
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{n} builds  ·  avg {avg:.1}s  ·  last {last:.1}s"),
        Style::default().fg(Color::Rgb(160, 170, 200)),
    )));

    f.render_widget(Paragraph::new(lines), inner);
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

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max.saturating_sub(1)).collect::<String>() + "…"
    }
}
