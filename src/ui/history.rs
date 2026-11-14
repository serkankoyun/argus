//! History view: agents that have finished, newest first. They are hidden from
//! the other views unless `f` is pressed.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, Wrap};
use ratatui::Frame;

use super::common::*;
use crate::app::App;
use crate::model::{Agent, MsgKind};

/// The last thing a finished agent reported: its result message, else the last
/// transcript line.
fn outcome(app: &App, a: &Agent) -> String {
    app.full
        .messages
        .iter()
        .rev()
        .find(|m| m.from == a.id && m.kind == MsgKind::Result)
        .map(|m| m.label.clone())
        .or_else(|| {
            a.transcript
                .last()
                .map(|(_, _, t)| t.lines().next().unwrap_or("").to_string())
        })
        .unwrap_or_default()
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
    let done = app.finished();
    let [table_area, detail] =
        Layout::vertical([Constraint::Min(6), Constraint::Length(7)]).areas(area);
    let total: f64 = done.iter().map(|a| a.cost_usd).sum();
    let header = Row::new(
        [
            "col-agent",
            "col-host",
            "col-team",
            "col-finished",
            "col-time",
            "col-tokens",
            "col-cost",
            "col-outcome",
        ]
        .map(|k| Cell::from(i.t(k))),
    )
    .style(Style::new().fg(DIM));
    let rows = done.iter().enumerate().map(|(k, a)| {
        let dur = ((a.last_activity - a.started).max(0) / 1000) as u64;
        let row = Row::new(vec![
            Cell::from(Span::styled(a.name.clone(), bold().fg(FG))),
            Cell::from(Span::styled(a.host.clone(), Style::new().fg(CYAN))),
            Cell::from(Span::styled(a.team_key(), Style::new().fg(PURPLE))),
            Cell::from(Span::styled(clock(a.last_activity), Style::new().fg(DIM))),
            Cell::from(duration(dur)),
            Cell::from(format!("{:>7}", tokens(a.tokens()))),
            Cell::from(format!("${:.2}", a.cost_usd)),
            Cell::from(Span::styled(outcome(app, a), Style::new().fg(GREEN))),
        ]);
        if k == 0 {
            row.style(Style::new().bg(SELECTED_BG))
        } else {
            row
        }
    });
    let widths = [
        Constraint::Length(18),
        Constraint::Length(12),
        Constraint::Length(16),
        Constraint::Length(9),
        Constraint::Length(7),
        Constraint::Length(8),
        Constraint::Length(8),
        Constraint::Min(20),
    ];
    let title = i.t_args(
        "history-title",
        &[
            ("n", done.len().to_string().as_str()),
            ("cost", format!("{total:.2}").as_str()),
        ],
    );
    let block = pane(title, true).title_bottom(Line::styled(
        format!(" {} ", i.t("history-hint")),
        Style::new().fg(DIM),
    ));
    if done.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(i.t("history-empty"), Style::new().fg(DIM))).block(block),
            table_area,
        );
    } else {
        frame.render_widget(
            Table::new(rows, widths)
                .header(header)
                .column_spacing(1)
                .block(block),
            table_area,
        );
    }
    // Details of the most recently finished agent.
    let lines: Vec<Line> = match done.first() {
        Some(a) => a
            .transcript
            .iter()
            .rev()
            .take(detail.height.saturating_sub(2) as usize)
            .rev()
            .map(|(ts, role, text)| {
                Line::from(vec![
                    Span::styled(format!("{} ", clock(*ts)), Style::new().fg(DIM)),
                    Span::styled(format!("{role:<10}"), Style::new().fg(ACCENT)),
                    Span::raw(text.lines().next().unwrap_or("").to_string()),
                ])
            })
            .collect(),
        None => Vec::new(),
    };
    let name = done.first().map(|a| a.name.clone()).unwrap_or_default();
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(pane(format!("{} · {name}", i.t("history-last")), false)),
        detail,
    );
}
