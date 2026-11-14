//! Rendering. Every view shares the header, the view bar and the key hints.

mod agent;
mod channel;
mod common;
mod fleet;
mod graph;
mod history;
mod lanes;
mod layout;
mod overview;
mod pair;
mod room;
mod sequence;
mod taskmap;
mod teams;

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
        View::Teams => teams::draw(frame, body, app),
        View::History => history::draw(frame, body, app),
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
    if let Some(h) = &app.host {
        spans.push(Span::styled(
            format!(" · {} {h}", app.i18n.t("host-label")),
            Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(t) = &app.team {
        spans.push(Span::styled(
            format!(" · {} {t}", app.i18n.t("team-label")),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    }
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

/// Tab titles as drawn in the view bar.
fn tab_titles(app: &App) -> Vec<String> {
    View::ALL
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let n = match i {
                9 => "0".to_string(),
                10 => "T".to_string(),
                11 => "H".to_string(),
                _ => (i + 1).to_string(),
            };
            format!("{n} {}", app.i18n.t(v.key()))
        })
        .collect()
}

/// Which tab is at terminal column `col` of row `row` (the view bar is row 1).
pub fn tab_at(app: &App, row: u16, col: u16) -> Option<usize> {
    if row != 1 {
        return None;
    }
    let mut x = 0u16;
    for (i, t) in tab_titles(app).iter().enumerate() {
        // Each tab is " title " followed by a one-column divider.
        let w = Line::raw(t.as_str()).width() as u16 + 2;
        if col >= x && col < x + w {
            return Some(i);
        }
        x += w + 1;
    }
    None
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let titles = tab_titles(app).into_iter().map(Line::from);
    let tabs = Tabs::new(titles)
        .select(app.view.index())
        .style(Style::new().fg(DIM))
        .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider(Span::styled("│", Style::new().fg(BORDER)));
    frame.render_widget(tabs, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(buf) = &app.input {
        let cursor = if (app.frame / 8) % 2 == 0 { "▌" } else { " " };
        let hint = if buf.starts_with('>') {
            app.i18n.t("input-prompt-hint")
        } else {
            app.i18n.t("input-cmd-hint")
        };
        let line = Line::from(vec![
            Span::styled(
                buf.clone(),
                Style::new().fg(FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::new().fg(GREEN)),
            Span::styled(format!("   {hint}"), Style::new().fg(DIM)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    let key =
        |k: &'static str| Span::styled(format!(" {k} "), Style::new().bg(BORDER).fg(Color::White));
    let label = |id: &str| Span::styled(format!(" {}  ", app.i18n.t(id)), Style::new().fg(DIM));
    let mut spans = if app.view == View::Teams {
        vec![
            key("←→"),
            label("key-team-move"),
            key("enter"),
            label("teams-open"),
            key("1-0"),
            label("key-views"),
        ]
    } else {
        let mut v = vec![
            key("1-0"),
            label("key-views"),
            key("↑↓"),
            label("key-select"),
            key("enter"),
            label("key-detail"),
        ];
        if (app.team.is_some() || app.host.is_some()) && app.view == View::Overview {
            v.extend([key("esc"), label("key-leave-team")]);
        }
        v
    };
    if app.pending_approval().is_some() {
        spans.extend([
            key("a"),
            Span::styled(
                format!(" {}  ", app.i18n.t("key-approve")),
                Style::new().fg(AMBER),
            ),
            key("r"),
            label("key-reject"),
        ]);
    }
    spans.extend([
        key(":"),
        label("key-cmd"),
        key(">"),
        label("key-prompt"),
        key("space"),
        label("key-pause"),
        key("q"),
        label("key-quit"),
    ]);
    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(48)]).areas(area);
    frame.render_widget(Paragraph::new(Line::from(spans)), left);
    if app.debug_keys && !app.last_key.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!("key: {}", app.last_key),
                Style::new().fg(CYAN),
            ))
            .right_aligned(),
            right,
        );
        return;
    }
    if let Some((at, n)) = &app.flash {
        if app.now - at < 5_000 {
            let (text, color) = match n {
                Ok(t) => (format!("✓ {t}"), GREEN),
                Err(e) => (format!("✗ {e}"), RED),
            };
            frame.render_widget(
                Paragraph::new(Line::styled(text, Style::new().fg(color))).right_aligned(),
                right,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::i18n::I18n;
    use crate::model::YOU;
    use crate::source::demo::DemoSource;
    use crate::source::{Channels, Source};

    /// A snapshot far bigger than any screen: the demo team copied many times.
    fn crowded() -> crate::model::Snapshot {
        let mut s = DemoSource::new("en").poll();
        let base_agents = s.agents.clone();
        let base_msgs = s.messages.clone();
        let base_tools = s.tools.clone();
        for k in 1..8 {
            let tag = |id: &str| {
                if id == YOU {
                    id.to_string()
                } else {
                    format!("{id}-{k}")
                }
            };
            for a in &base_agents {
                let mut a = a.clone();
                a.id = tag(&a.id);
                a.name = format!("{}-{k}", a.name);
                a.parent = a.parent.as_deref().map(tag);
                a.host = format!("host-{}", k % 3);
                a.team = Some(format!("team-{}", k % 4));
                s.agents.push(a);
            }
            for m in &base_msgs {
                let mut m = m.clone();
                m.from = tag(&m.from);
                m.to = tag(&m.to);
                s.messages.push(m);
            }
            for t in &base_tools {
                let mut t = t.clone();
                t.agent = tag(&t.agent);
                s.tools.push(t);
            }
        }
        s
    }

    fn app_with(snapshot: crate::model::Snapshot) -> App {
        let (snap_tx, snap_rx) = mpsc::channel();
        let (ctl_tx, _ctl_rx) = mpsc::channel();
        let (_note_tx, note_rx) = mpsc::channel();
        snap_tx.send(snapshot).unwrap();
        let mut app = App::new(
            I18n::new(Some("en")),
            Channels {
                snapshots: snap_rx,
                controls: ctl_tx,
                notices: note_rx,
            },
            true,
        );
        app.on_tick();
        app
    }

    #[test]
    fn every_view_renders_crowded_and_tiny_without_panicking() {
        let mut app = app_with(crowded());
        assert!(app.snapshot.agents.len() > 60);
        for (w, h) in [(187, 54), (120, 36), (80, 24), (40, 12), (10, 5)] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            for v in View::ALL {
                app.view = v;
                for grouped in [false, true] {
                    app.group_by_team = grouped;
                    terminal
                        .draw(|f| draw(f, &app))
                        .unwrap_or_else(|e| panic!("{v:?} at {w}x{h}: {e}"));
                }
            }
        }
    }

    #[test]
    fn every_view_renders_empty() {
        let mut app = app_with(crate::model::Snapshot::default());
        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        for v in View::ALL {
            app.view = v;
            terminal.draw(|f| draw(f, &app)).unwrap();
        }
    }
}
