//! View 6 — Room: what every agent is doing right now, the request it is
//! sending (as a bubble), and every open request with its waiting time.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget, Wrap};
use ratatui::Frame;

use super::common::*;
use super::graph::{draw_links, TRAVEL};
use super::layout::{self, Node};
use crate::app::App;
use crate::model::{AgentState, Message, MsgKind, YOU};

const NODE_W: u16 = 30;
const NODE_H: u16 = 5;
/// Bubbles stay visible a little longer than the packet travels.
const BUBBLE: i64 = TRAVEL * 2;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let [top, bottom] = Layout::vertical([Constraint::Min(12), Constraint::Length(5)]).areas(area);
    let [room, requests] =
        Layout::horizontal([Constraint::Min(40), Constraint::Length(46)]).areas(top);

    let block = pane(app.i18n.t("pane-room"), true);
    let inner = block.inner(room);
    frame.render_widget(block, room);
    draw_room(frame.buffer_mut(), inner, app);
    draw_requests(frame, requests, app);
    draw_conversations(frame, bottom, app);
}

fn draw_room(buf: &mut Buffer, area: Rect, app: &App) {
    let s = &app.snapshot;
    if s.agents.is_empty() {
        Paragraph::new(Line::styled(app.i18n.t("flow-empty"), Style::new().fg(DIM)))
            .render(area, buf);
        return;
    }
    let with_you = s.messages.iter().any(|m| m.to == YOU);
    let (nodes, _) = layout::place(s, area, NODE_W, NODE_H + 1, with_you);
    let pos: HashMap<&str, &Node> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    draw_links(buf, area, app, &pos, false);

    for n in &nodes {
        let r = layout::node_rect(n, NODE_W, NODE_H, area);
        draw_card(buf, r, n, app);
    }
    // One bubble per sender: its most recent message still "in the air".
    let mut seen = Vec::new();
    for m in s
        .messages
        .iter()
        .rev()
        .filter(|m| (0..BUBBLE).contains(&(app.now - m.ts)))
    {
        if seen.contains(&m.from) {
            continue;
        }
        seen.push(m.from.clone());
        if let Some(n) = pos.get(m.from.as_str()) {
            draw_bubble(buf, area, n, m, app);
        }
    }
}

fn draw_card(buf: &mut Buffer, r: Rect, n: &Node, app: &App) {
    Clear.render(r, buf);
    let bg = Style::new().bg(Color::Rgb(22, 27, 34));
    if n.id == YOU {
        let c = pulse_color(app, AgentState::Waiting);
        let pending = app
            .snapshot
            .messages
            .iter()
            .filter(|m| m.to == YOU && m.answered_at.is_none())
            .count();
        let lines = vec![
            Line::from(vec![
                Span::styled("▲ ", Style::new().fg(c)),
                Span::styled(app.i18n.t("you"), bold().fg(FG)),
            ]),
            Line::styled(
                app.i18n
                    .t_args("approvals-pending", &[("n", pending.to_string().as_str())]),
                Style::new().fg(FG),
            ),
            Line::styled(app.i18n.t("press-a"), Style::new().fg(c)),
        ];
        Paragraph::new(lines)
            .block(Block::bordered().border_style(Style::new().fg(c)).style(bg))
            .render(r, buf);
        return;
    }
    let Some(a) = app.snapshot.agent(&n.id) else {
        return;
    };
    let c = pulse_color(app, a.state);
    let spin = match a.state {
        AgentState::Working => spinner(app),
        AgentState::Waiting => "…",
        other => other.symbol(),
    };
    let title = Line::from(vec![
        Span::styled(format!(" {} ", a.state.symbol()), Style::new().fg(c)),
        Span::styled(format!("{} ", a.name), bold().fg(FG)),
    ]);
    let host = Line::styled(format!(" {} ", a.host), Style::new().fg(DIM)).right_aligned();
    let text = Line::from(vec![
        Span::styled(
            format!("{} ", app.i18n.t("now-label")),
            Style::new().fg(DIM),
        ),
        Span::styled(format!("{spin} "), Style::new().fg(c)),
        Span::styled(now_text(app, a), Style::new().fg(FG)),
    ]);
    let selected = app.selected.as_deref() == Some(a.id.as_str());
    let border = if selected {
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(c)
    };
    Paragraph::new(vec![text])
        .wrap(Wrap { trim: true })
        .block(
            Block::bordered()
                .border_style(border)
                .title(title)
                .title(host)
                .style(bg),
        )
        .render(r, buf);
}

fn draw_bubble(buf: &mut Buffer, area: Rect, n: &Node, m: &Message, app: &App) {
    let c = kind_color(m.kind);
    let text = format!(
        " {} {}: {} ",
        m.kind.glyph(),
        name(app, &m.to).as_str(),
        trunc(&m.label, 40)
    );
    let w = text.chars().count() as u16;
    let y = n.y.saturating_sub(NODE_H / 2 + 1);
    if y < area.y {
        return;
    }
    let x = n.x.saturating_sub(w / 2).max(area.x);
    let room = (area.x + area.width).saturating_sub(x) as usize;
    if area.contains(Position { x, y }) {
        buf.set_stringn(
            x,
            y,
            &text,
            room,
            Style::new().fg(FG).bg(Color::Rgb(13, 17, 23)),
        );
        buf.set_stringn(
            x + 1,
            y,
            m.kind.glyph(),
            room,
            Style::new()
                .fg(c)
                .bg(Color::Rgb(13, 17, 23))
                .add_modifier(Modifier::BOLD),
        );
    }
}

/// Requests that are still open, or were answered recently.
pub fn open_requests(app: &App) -> Vec<&Message> {
    app.snapshot
        .messages
        .iter()
        .rev()
        .filter(|m| m.kind != MsgKind::Result)
        .filter(|m| m.answered_at.is_none() || app.now - m.answered_at.unwrap_or(0) < 60_000)
        .collect()
}

fn draw_requests(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
    let mut lines = Vec::new();
    for m in open_requests(app)
        .into_iter()
        .take(area.height.saturating_sub(2) as usize / 3)
    {
        let c = kind_color(m.kind);
        let (status, sc, wait) = match (m.kind, m.answered_at) {
            (_, Some(at)) => (
                i.t("req-answered"),
                DIM,
                i.t_args(
                    "req-answered-in",
                    &[("t", duration(((at - m.ts) / 1000).max(0) as u64).as_str())],
                ),
            ),
            (MsgKind::Perm, None) => (
                i.t("req-approval"),
                AMBER,
                i.t_args(
                    "req-waiting",
                    &[("t", duration(((app.now - m.ts) / 1000) as u64).as_str())],
                ),
            ),
            (MsgKind::Finding, None) => (
                i.t("req-new"),
                RED,
                i.t_args(
                    "req-waiting",
                    &[("t", duration(((app.now - m.ts) / 1000) as u64).as_str())],
                ),
            ),
            _ => (
                i.t("req-working"),
                GREEN,
                i.t_args(
                    "req-running",
                    &[("t", duration(((app.now - m.ts) / 1000) as u64).as_str())],
                ),
            ),
        };
        let bg = if m.kind == MsgKind::Perm && m.answered_at.is_none() {
            Style::new().bg(Color::Rgb(42, 35, 18))
        } else {
            Style::new()
        };
        lines.push(
            Line::from(vec![
                Span::styled(format!("{} ", m.kind.glyph()), Style::new().fg(c)),
                Span::styled(trunc(name(app, &m.from).as_str(), 12), bold().fg(FG)),
                Span::styled(" → ", Style::new().fg(DIM)),
                Span::styled(
                    format!("{:<14}", trunc(name(app, &m.to).as_str(), 12)),
                    bold().fg(FG),
                ),
                Span::styled(status, Style::new().fg(sc)),
            ])
            .style(bg),
        );
        lines.push(Line::raw(format!(
            "  {}",
            trunc(if m.text.is_empty() { &m.label } else { &m.text }, 42)
        )));
        lines.push(Line::styled(format!("  {wait}"), Style::new().fg(DIM)));
    }
    if lines.is_empty() {
        lines.push(Line::styled(i.t("req-none"), Style::new().fg(DIM)));
    }
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("pane-requests"), false)),
        area,
    );
}

fn draw_conversations(frame: &mut Frame, area: Rect, app: &App) {
    let rows = area.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = s
        .messages
        .iter()
        .rev()
        .take(rows)
        .enumerate()
        .map(|(k, m)| {
            let mut l = Line::from(vec![
                Span::styled(format!("{} ", clock(m.ts)), Style::new().fg(DIM)),
                Span::styled(
                    format!("{:>12}", trunc(name(app, &m.from).as_str(), 12)),
                    Style::new().fg(PURPLE),
                ),
                Span::styled(" → ", Style::new().fg(DIM)),
                Span::styled(
                    format!("{:<12} ", trunc(name(app, &m.to).as_str(), 12)),
                    Style::new().fg(if m.to == YOU { AMBER } else { PURPLE }),
                ),
                Span::styled(
                    format!("\"{}\"", if m.text.is_empty() { &m.label } else { &m.text }),
                    Style::new().fg(FG),
                ),
            ]);
            if k == 0 {
                l = l.style(Style::new().bg(SELECTED_BG));
            }
            l
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(pane(app.i18n.t("pane-conversations"), false)),
        area,
    );
}
