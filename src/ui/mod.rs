//! Rendering. Every view shares the header, the view bar and the key hints.

mod agent;
mod channel;
mod common;
mod fleet;
mod graph;
mod lanes;
mod layout;
mod overview;
mod pair;
mod room;
mod sequence;
mod taskmap;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, View};
use crate::model::AgentState;
use common::*;

pub fn draw(frame: &mut Frame, app: &App) {
    let [header, tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);
    draw_tabs(frame, tabs, app);
    match app.view {
        View::Overview => overview::draw(frame, body, app),
        View::AgentDetail => agent::draw(frame, body, app),
        View::Graph => graph::draw(frame, body, app),
        View::Room => room::draw(frame, body, app),
        View::Lanes => lanes::draw(frame, body, app),
        View::Sequence => sequence::draw(frame, body, app),
        View::Fleet => fleet::draw(frame, body, app),
        View::Channel => channel::draw(frame, body, app),
        View::TaskMap => taskmap::draw(frame, body, app),
        View::Pair => pair::draw(frame, body, app),
    }
    draw_footer(frame, footer, app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let count = |st: AgentState| s.agents.iter().filter(|a| a.state == st).count();
    let status = if app.paused {
        Span::styled(
            format!("● {}", app.i18n.t("status-paused")),
            Style::new().fg(AMBER),
        )
    } else if app.demo {
        Span::styled(
            format!("● {}", app.i18n.t("status-demo")),
            Style::new().fg(PURPLE),
        )
    } else {
        Span::styled(
            format!("● {}", app.i18n.t("status-live")),
            Style::new().fg(GREEN),
        )
    };
    let summary = app.i18n.t_args(
        "header-summary",
        &[
            ("hosts", s.hosts.len().to_string().as_str()),
            ("agents", s.agents.len().to_string().as_str()),
            ("active", count(AgentState::Working).to_string().as_str()),
        ],
    );
    let mut spans = vec![
        Span::styled("argus ", Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(env!("CARGO_PKG_VERSION"), Style::new().fg(DIM)),
        Span::styled("  │  ", Style::new().fg(DIM)),
        Span::raw(summary),
    ];
    let waiting = count(AgentState::Waiting);
    if waiting > 0 {
        spans.push(Span::styled(
            format!(
                " · {}",
                app.i18n
                    .t_args("header-waiting", &[("n", waiting.to_string().as_str())])
            ),
            Style::new().fg(AMBER),
        ));
    }
    let stalled = count(AgentState::Stalled);
    if stalled > 0 {
        spans.push(Span::styled(
            format!(
                " · {}",
                app.i18n
                    .t_args("header-stalled", &[("n", stalled.to_string().as_str())])
            ),
            Style::new().fg(RED),
        ));
    }
    spans.extend([
        Span::styled("  │  ", Style::new().fg(DIM)),
        Span::raw(format!("${:.2}  ", s.total_cost())),
        status,
        Span::styled(
            format!("  {}  {}", clock(app.now), app.i18n.lang.to_uppercase()),
            Style::new().fg(DIM),
        ),
    ]);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let titles = View::ALL.iter().enumerate().map(|(i, v)| {
        let n = if i == 9 { 0 } else { i + 1 };
        Line::from(format!("{n} {}", app.i18n.t(v.key())))
    });
    let tabs = Tabs::new(titles)
        .select(app.view.index())
        .style(Style::new().fg(DIM))
        .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider(Span::styled("│", Style::new().fg(BORDER)));
    frame.render_widget(tabs, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let key =
        |k: &'static str| Span::styled(format!(" {k} "), Style::new().bg(BORDER).fg(Color::White));
    let label = |id: &str| Span::styled(format!(" {}  ", app.i18n.t(id)), Style::new().fg(DIM));
    let line = Line::from(vec![
        key("1-0"),
        label("key-views"),
        key("tab"),
        label("key-next"),
        key("↑↓"),
        label("key-select"),
        key("⏎"),
        label("key-detail"),
        key("space"),
        label("key-pause"),
        key("q"),
        label("key-quit"),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}
