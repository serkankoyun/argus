//! View 5 — Graph: agents as nodes grouped by host; messages travel along
//! the edges in real time. Edge brightness shows traffic volume.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph, Widget};
use ratatui::Frame;

use super::common::*;
use super::layout::{self, Node};
use crate::app::App;
use crate::model::{Message, Millis, MsgKind, Snapshot, YOU};

/// How long a message takes to travel along an edge, in ms.
pub const TRAVEL: Millis = 4_000;
const NODE_W: u16 = 24;
const NODE_H: u16 = 5;
/// Edges reflect traffic from this window.
const WINDOW: Millis = 15 * 60_000;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let [graph, side] =
        Layout::horizontal([Constraint::Min(40), Constraint::Length(40)]).areas(area);
    let [traffic, ticker] =
        Layout::vertical([Constraint::Length(12), Constraint::Min(4)]).areas(side);

    let block = pane(app.i18n.t("pane-graph"), true).title_bottom(Line::styled(
        format!(" {} ", app.i18n.t("graph-legend")),
        Style::new().fg(DIM),
    ));
    let inner = block.inner(graph);
    frame.render_widget(block, graph);
    draw_graph(frame.buffer_mut(), inner, app);
    draw_traffic(frame, traffic, app);
    draw_ticker(frame, ticker, app);
}

/// Unordered pair key.
fn pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Message counts per agent pair within the window, with the dominant kind.
pub fn traffic(s: &Snapshot, now: Millis) -> Vec<((String, String), usize, MsgKind)> {
    let mut map: HashMap<(String, String), (usize, HashMap<u8, usize>)> = HashMap::new();
    for m in s.messages.iter().filter(|m| now - m.ts < WINDOW) {
        let e = map.entry(pair(&m.from, &m.to)).or_default();
        e.0 += 1;
        *e.1.entry(m.kind as u8).or_default() += 1;
    }
    let kinds = [
        MsgKind::Task,
        MsgKind::Msg,
        MsgKind::Result,
        MsgKind::Perm,
        MsgKind::Finding,
    ];
    let mut out: Vec<_> = map
        .into_iter()
        .map(|(k, (n, by))| {
            let dom = by
                .into_iter()
                .max_by_key(|(_, c)| *c)
                .map(|(k, _)| k)
                .unwrap_or(0);
            (k, n, kinds[dom as usize % kinds.len()])
        })
        .collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

fn draw_graph(buf: &mut Buffer, area: Rect, app: &App) {
    let s = &app.snapshot;
    if s.agents.is_empty() {
        Paragraph::new(Line::styled(app.i18n.t("flow-empty"), Style::new().fg(DIM)))
            .render(area, buf);
        return;
    }
    let with_you = s.messages.iter().any(|m| m.to == YOU || m.from == YOU);
    let (nodes, groups) = layout::place(s, area, NODE_W, NODE_H, with_you, app.group_by_team);
    let pos: HashMap<&str, &Node> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();

    for g in &groups {
        let host = if app.group_by_team {
            None
        } else {
            s.hosts.iter().find(|h| h.name == g.host)
        };
        let (dot, c) = match host {
            Some(h) if !h.connected => ("◐", AMBER),
            _ => ("●", GREEN),
        };
        let title = Line::from(vec![
            Span::styled(format!(" {dot} "), Style::new().fg(c)),
            Span::styled(g.host.clone(), Style::new().fg(CYAN)),
            Span::styled(
                host.and_then(|h| h.rtt_ms)
                    .map(|r| format!(" · {r}ms "))
                    .unwrap_or_else(|| " ".into()),
                Style::new().fg(DIM),
            ),
        ]);
        Block::bordered()
            .border_type(BorderType::Plain)
            .border_style(Style::new().fg(Color::Rgb(38, 43, 50)))
            .title(title)
            .render(g.area, buf);
    }

    draw_links(buf, area, app, &pos, true);

    for n in &nodes {
        let r = layout::node_rect(n, NODE_W, NODE_H, area);
        draw_node(buf, r, n, app);
    }
}

/// Draws edges (parent links and message traffic) and messages in flight.
pub fn draw_links(
    buf: &mut Buffer,
    area: Rect,
    app: &App,
    pos: &HashMap<&str, &Node>,
    labels: bool,
) {
    let s = &app.snapshot;
    // Edges: parent links plus every pair that exchanged messages.
    let mut edges: HashMap<(String, String), (usize, MsgKind)> = HashMap::new();
    for a in &s.agents {
        if let Some(p) = &a.parent {
            edges.entry(pair(&a.id, p)).or_insert((0, MsgKind::Task));
        }
    }
    for (k, n, kind) in traffic(s, app.now) {
        edges.insert(k, (n, kind));
    }
    for ((a, b), (n, kind)) in &edges {
        let (Some(na), Some(nb)) = (pos.get(a.as_str()), pos.get(b.as_str())) else {
            continue;
        };
        let color = kind_color(*kind);
        let style = match n {
            0 => Style::new().fg(BORDER),
            1..=3 => Style::new().fg(dim_color(color)),
            4..=9 => Style::new().fg(color),
            _ => Style::new().fg(color).add_modifier(Modifier::BOLD),
        };
        let glyph = if *n >= 10 { "•" } else { "·" };
        for (x, y) in layout::line_cells((na.x, na.y), (nb.x, nb.y)) {
            if area.contains(Position { x, y }) {
                put(buf, area, x, y, glyph, style);
            }
        }
    }

    // Messages in flight.
    for m in s
        .messages
        .iter()
        .filter(|m| (0..TRAVEL).contains(&(app.now - m.ts)))
    {
        let (Some(a), Some(b)) = (pos.get(m.from.as_str()), pos.get(m.to.as_str())) else {
            continue;
        };
        let t = (app.now - m.ts) as f64 / TRAVEL as f64;
        let (x, y) = layout::lerp((a.x, a.y), (b.x, b.y), t);
        if area.contains(Position { x, y }) {
            let c = kind_color(m.kind);
            if !labels {
                put(
                    buf,
                    area,
                    x,
                    y,
                    m.kind.glyph(),
                    Style::new().fg(c).add_modifier(Modifier::BOLD),
                );
                continue;
            }
            put(
                buf,
                area,
                x,
                y,
                m.kind.glyph(),
                Style::new().fg(c).add_modifier(Modifier::BOLD),
            );
            let label = format!(" {} ", trunc(&m.label, 22));
            let lx = x.saturating_add(2);
            let room = (area.x + area.width).saturating_sub(lx) as usize;
            putn(
                buf,
                area,
                lx,
                y,
                &label,
                room,
                Style::new().fg(FG).bg(Color::Rgb(13, 17, 23)),
            );
        }
    }
}

fn dim_color(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(r / 2 + 10, g / 2 + 10, b / 2 + 10),
        other => other,
    }
}

fn draw_node(buf: &mut Buffer, r: Rect, n: &Node, app: &App) {
    Clear.render(r, buf);
    if n.id == YOU {
        let c = pulse_color(app, crate::model::AgentState::Waiting);
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
                Style::new().fg(DIM),
            ),
            Line::styled(app.i18n.t("press-a"), Style::new().fg(c)),
        ];
        Paragraph::new(lines)
            .block(
                Block::bordered()
                    .border_style(Style::new().fg(c))
                    .style(Style::new().bg(Color::Rgb(22, 27, 34))),
            )
            .render(r, buf);
        return;
    }
    let Some(a) = app.snapshot.agent(&n.id) else {
        return;
    };
    let c = pulse_color(app, a.state);
    let selected = app.selected.as_deref() == Some(a.id.as_str());
    let spin = if a.state == crate::model::AgentState::Working {
        spinner(app)
    } else {
        a.state.symbol()
    };
    let inner_w = r.width.saturating_sub(2) as usize;
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{spin} "), Style::new().fg(c)),
            Span::styled(trunc(&a.name, inner_w.saturating_sub(2)), bold().fg(FG)),
        ]),
        Line::styled(trunc(&now_text(app, a), inner_w), Style::new().fg(DIM)),
        Line::from(vec![
            Span::styled(bar(a.context_pct(), 5, '█', '░'), Style::new().fg(c)),
            Span::styled(
                format!(" {}% ${:.2}", a.context_pct(), a.cost_usd),
                Style::new().fg(DIM),
            ),
        ]),
    ];
    let border = if selected {
        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(c)
    };
    Paragraph::new(lines)
        .block(
            Block::bordered()
                .border_style(border)
                .style(Style::new().bg(Color::Rgb(22, 27, 34))),
        )
        .render(r, buf);
}

fn draw_traffic(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let rows = area.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = traffic(s, app.now)
        .into_iter()
        .take(rows)
        .map(|((a, b), n, kind)| {
            let label = format!(
                "{} ↔ {}",
                trunc(name(app, &a).as_str(), 11),
                trunc(name(app, &b).as_str(), 11)
            );
            Line::from(vec![
                Span::raw(format!("{label:<25}")),
                Span::styled(format!("{n:>3} "), bold().fg(FG)),
                Span::styled(
                    "▇".repeat(n.div_ceil(2).min(8)),
                    Style::new().fg(kind_color(kind)),
                ),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(pane(app.i18n.t("pane-traffic"), false)),
        area,
    );
}

fn draw_ticker(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let rows = area.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = s
        .messages
        .iter()
        .rev()
        .take(rows)
        .enumerate()
        .map(|(i, m)| ticker_line(app, m, i == 0))
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(pane(app.i18n.t("pane-now"), false)),
        area,
    );
}

fn ticker_line<'a>(app: &App, m: &Message, first: bool) -> Line<'a> {
    let c = kind_color(m.kind);
    let mut line = Line::from(vec![
        Span::styled(format!("{} ", clock(m.ts)), Style::new().fg(DIM)),
        Span::styled(format!("{} ", m.kind.glyph()), Style::new().fg(c)),
        Span::raw(format!(
            "{}→{} ",
            trunc(name(app, &m.from).as_str(), 9),
            trunc(name(app, &m.to).as_str(), 9)
        )),
        Span::styled(trunc(&m.label, 30), Style::new().fg(FG)),
    ]);
    if first {
        line = line.style(Style::new().bg(SELECTED_BG));
    }
    line
}
