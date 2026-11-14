//! Rendering. Every view shares the header, the view bar and the key hints.

mod overview;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, View};

pub const ACCENT: Color = Color::Rgb(88, 166, 255);
pub const DIM: Color = Color::Rgb(139, 148, 158);
pub const GREEN: Color = Color::Rgb(63, 185, 80);
pub const AMBER: Color = Color::Rgb(210, 153, 34);
pub const RED: Color = Color::Rgb(248, 81, 73);
pub const BORDER: Color = Color::Rgb(48, 54, 61);

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
        other => draw_placeholder(frame, body, app, other),
    }
    draw_footer(frame, footer, app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let working = s
        .agents
        .iter()
        .filter(|a| a.state == crate::model::AgentState::Working)
        .count();
    let status = if app.paused {
        Span::styled(
            format!("● {}", app.i18n.t("status-paused")),
            Style::new().fg(AMBER),
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
            ("active", working.to_string().as_str()),
        ],
    );
    let line = Line::from(vec![
        Span::styled("argus ", Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(env!("CARGO_PKG_VERSION"), Style::new().fg(DIM)),
        Span::styled("  │  ", Style::new().fg(DIM)),
        Span::raw(summary),
        Span::styled("  │  ", Style::new().fg(DIM)),
        Span::raw(format!("${:.2}  ", s.total_cost())),
        status,
        Span::styled(
            format!("  {}", app.i18n.lang.to_uppercase()),
            Style::new().fg(DIM),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
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

fn draw_placeholder(frame: &mut Frame, area: Rect, app: &App, view: View) {
    let block = Block::bordered()
        .border_style(Style::new().fg(BORDER))
        .title(Span::styled(
            format!(" {} ", app.i18n.t(view.key())),
            Style::new().fg(ACCENT).bold(),
        ));
    let text = app
        .i18n
        .t_args("placeholder", &[("milestone", view.milestone())]);
    let p = Paragraph::new(Line::from(Span::styled(text, Style::new().fg(DIM)))).block(block);
    frame.render_widget(p, area);
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
        key("space"),
        label("key-pause"),
        key("q"),
        label("key-quit"),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}
