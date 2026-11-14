//! View 7 — Lanes: one timeline per agent, time flowing right to left;
//! messages between agents are vertical arrows. Below: who asks whom.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table, Widget};
use ratatui::Frame;

use super::common::*;
use crate::app::App;
use crate::model::{Agent, AgentState, Millis, MsgKind, YOU};

const LANE_H: u16 = 3;
const LABEL_W: u16 = 16;
const NOW_W: u16 = 38;

pub fn tool_color(tool: &str) -> Color {
    match tool {
        "Read" | "Grep" | "Glob" | "LS" | "NotebookRead" => ACCENT,
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => GREEN,
        "Bash" | "BashOutput" | "KillShell" => AMBER,
        "Task" | "Agent" | "SendMessage" | "TaskCreate" | "TaskUpdate" => PURPLE,
        "WebFetch" | "WebSearch" => CYAN,
        _ => DIM,
    }
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let lanes = app.snapshot.agents.len() as u16 + u16::from(has_you(app));
    let matrix_h = 9;
    let [top, bottom] = Layout::vertical([
        Constraint::Min((lanes * LANE_H + 3).min(area.height.saturating_sub(matrix_h))),
        Constraint::Length(matrix_h),
    ])
    .areas(area);
    let window = app.lanes_window();
    let block = pane(app.i18n.t("pane-lanes"), true).title_bottom(Line::styled(
        format!(
            " {} ",
            app.i18n.t_args(
                "lanes-legend",
                &[("w", duration((window / 1000) as u64).as_str())]
            )
        ),
        Style::new().fg(DIM),
    ));
    let inner = block.inner(top);
    frame.render_widget(block, top);
    draw_lanes(frame.buffer_mut(), inner, app, window);
    draw_matrix(frame, bottom, app);
}

fn has_you(app: &App) -> bool {
    app.snapshot.messages.iter().any(|m| m.to == YOU)
}

fn draw_lanes(buf: &mut Buffer, area: Rect, app: &App, window: Millis) {
    let s = &app.snapshot;
    let i = &app.i18n;
    if area.width < LABEL_W + NOW_W + 10 || s.agents.is_empty() {
        Paragraph::new(Line::styled(i.t("flow-empty"), Style::new().fg(DIM))).render(area, buf);
        return;
    }
    let tl = Rect {
        x: area.x + LABEL_W,
        y: area.y + 1,
        width: area.width - LABEL_W - NOW_W - 1,
        height: area.height.saturating_sub(1),
    };
    // Axis.
    let axis = Style::new().fg(DIM);
    for k in 0..=4u16 {
        let x = tl.x + tl.width * k / 4;
        let ago = window * (4 - k as i64) / 4;
        let label = if k == 4 {
            format!("{} ▸", i.t("lanes-now"))
        } else {
            format!("-{}", duration((ago / 1000) as u64))
        };
        let lx = if k == 4 {
            x.saturating_sub(label.chars().count() as u16)
        } else {
            x
        };
        buf.set_string(
            lx,
            area.y,
            label,
            if k == 4 {
                Style::new().fg(ACCENT)
            } else {
                axis
            },
        );
    }
    buf.set_string(
        tl.x + tl.width + 1,
        area.y,
        trunc(&i.t("lanes-now-col"), NOW_W as usize),
        axis,
    );

    let mut rows: Vec<(String, Option<&Agent>)> = app
        .ordered()
        .into_iter()
        .map(|a| (a.id.clone(), Some(a)))
        .collect();
    if has_you(app) {
        rows.push((YOU.to_string(), None));
    }
    let lane_y: HashMap<&str, u16> = rows
        .iter()
        .enumerate()
        .map(|(k, (id, _))| (id.as_str(), tl.y + k as u16 * LANE_H))
        .collect();
    let x_of = |ts: Millis| -> Option<u16> {
        let age = app.now - ts;
        if !(0..=window).contains(&age) {
            return None;
        }
        Some(tl.x + tl.width - ((age as f64 / window as f64) * tl.width as f64).round() as u16)
    };

    for (id, agent) in &rows {
        let y = lane_y[id.as_str()];
        if y + 1 >= area.y + area.height {
            break;
        }
        for x in area.x..area.x + area.width {
            buf.set_string(
                x,
                y + LANE_H - 1,
                "─",
                Style::new().fg(Color::Rgb(33, 38, 45)),
            );
        }
        match agent {
            Some(a) => {
                let c = pulse_color(app, a.state);
                buf.set_string(
                    area.x,
                    y,
                    format!("{} ", a.state.symbol()),
                    Style::new().fg(c),
                );
                buf.set_stringn(area.x + 2, y, &a.name, LABEL_W as usize - 3, bold().fg(FG));
                buf.set_stringn(
                    area.x + 2,
                    y + 1,
                    &a.host,
                    LABEL_W as usize - 3,
                    Style::new().fg(DIM),
                );
                let spin = if a.state == AgentState::Working {
                    spinner(app)
                } else {
                    a.state.symbol()
                };
                let nx = tl.x + tl.width + 1;
                buf.set_string(nx, y, format!("{spin} "), Style::new().fg(c));
                buf.set_stringn(
                    nx + 2,
                    y,
                    now_text(app, a),
                    NOW_W as usize - 3,
                    Style::new().fg(FG),
                );
                let (label, who, wc) = waiting_for(app, a);
                buf.set_stringn(
                    nx + 2,
                    y + 1,
                    format!("{label} {who}"),
                    NOW_W as usize - 3,
                    Style::new().fg(wc),
                );
            }
            None => {
                let c = pulse_color(app, AgentState::Waiting);
                buf.set_string(area.x, y, "▲ ", Style::new().fg(c));
                buf.set_string(area.x + 2, y, i.t("you"), bold().fg(FG));
                buf.set_stringn(
                    tl.x + tl.width + 3,
                    y,
                    i.t("press-a"),
                    NOW_W as usize - 3,
                    Style::new().fg(c),
                );
            }
        }
    }

    // Tool calls as blocks on each lane.
    for t in &s.tools {
        let Some(&y) = lane_y.get(t.agent.as_str()) else {
            continue;
        };
        let end = t
            .duration_ms
            .map(|d| t.ts + d)
            .unwrap_or(app.now)
            .min(app.now);
        let (Some(x1), x0) = (x_of(end.max(t.ts)), x_of(t.ts).unwrap_or(tl.x)) else {
            continue;
        };
        let w = x1.saturating_sub(x0).max(1);
        let c = tool_color(&t.tool);
        let style = Style::new().fg(FG).bg(darken(c));
        let text = format!(
            "{:<w$}",
            trunc(&format!("{} {}", t.tool, t.target), w as usize),
            w = w as usize
        );
        buf.set_stringn(x0, y + 1, &text, w as usize, style);
        buf.set_string(x0, y + 1, "▏", Style::new().fg(c).bg(darken(c)));
    }

    // Messages as vertical arrows between lanes.
    for m in &s.messages {
        let (Some(&ya), Some(&yb), Some(x)) = (
            lane_y.get(m.from.as_str()),
            lane_y.get(m.to.as_str()),
            x_of(m.ts),
        ) else {
            continue;
        };
        let (ya, yb) = (ya + 1, yb + 1);
        let c = kind_color(m.kind);
        let (lo, hi) = (ya.min(yb), ya.max(yb));
        for y in lo..=hi {
            if area.contains(Position { x, y }) {
                buf.set_string(x, y, "┊", Style::new().fg(c));
            }
        }
        let head = if yb > ya { "▼" } else { "▲" };
        buf.set_string(x, yb, head, Style::new().fg(c).add_modifier(Modifier::BOLD));
        let mid = (lo + hi) / 2;
        let chip = format!(" {} ", trunc(&m.label, 20));
        let room = (tl.x + tl.width).saturating_sub(x + 1) as usize;
        if room > 4 {
            buf.set_stringn(
                x + 1,
                mid,
                chip,
                room,
                Style::new().fg(c).bg(Color::Rgb(13, 17, 23)),
            );
        }
    }
}

fn darken(c: Color) -> Color {
    match c {
        Color::Rgb(r, g, b) => Color::Rgb(r / 4, g / 4, b / 4),
        other => other,
    }
}

/// "waiting for X" / "asked by Y" for an agent, from open messages.
fn waiting_for(app: &App, a: &Agent) -> (String, String, Color) {
    let s = &app.snapshot;
    let open = |m: &&crate::model::Message| m.answered_at.is_none() && m.kind != MsgKind::Result;
    if let Some(m) = s
        .messages
        .iter()
        .rev()
        .filter(open)
        .find(|m| m.from == a.id)
    {
        let c = if m.to == YOU { AMBER } else { DIM };
        return (
            app.i18n.t("lanes-waiting-for"),
            s.name_of(&m.to).to_string(),
            c,
        );
    }
    if let Some(m) = s.messages.iter().rev().filter(open).find(|m| m.to == a.id) {
        return (
            app.i18n.t("lanes-asked-by"),
            s.name_of(&m.from).to_string(),
            DIM,
        );
    }
    (String::new(), String::new(), DIM)
}

fn draw_matrix(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let i = &app.i18n;
    let mut senders: Vec<&str> = Vec::new();
    let mut receivers: Vec<&str> = Vec::new();
    let mut cells: HashMap<(&str, &str), (usize, &str, MsgKind)> = HashMap::new();
    for m in &s.messages {
        if !senders.contains(&m.from.as_str()) {
            senders.push(&m.from);
        }
        if !receivers.contains(&m.to.as_str()) {
            receivers.push(&m.to);
        }
        let e = cells.entry((&m.from, &m.to)).or_insert((0, "", m.kind));
        e.0 += 1;
        e.1 = &m.label;
        e.2 = m.kind;
    }
    senders.truncate(area.height.saturating_sub(3) as usize);
    receivers.truncate(6);
    let hot = s.messages.last().map(|m| (m.from.as_str(), m.to.as_str()));
    let blink = (app.frame / 8) % 2 == 0;

    let mut header = vec![Cell::from(Span::styled(
        i.t("matrix-corner"),
        Style::new().fg(DIM),
    ))];
    header.extend(receivers.iter().map(|r| {
        Cell::from(Span::styled(
            trunc(s.name_of(r), 20),
            bold().fg(if *r == YOU { AMBER } else { FG }),
        ))
    }));
    let rows = senders.iter().map(|from| {
        let mut row = vec![Cell::from(Span::styled(
            trunc(s.name_of(from), 14),
            bold().fg(PURPLE),
        ))];
        for to in &receivers {
            let cell = match cells.get(&(*from, *to)) {
                Some((n, label, kind)) => {
                    let mut st = Style::new().fg(kind_color(*kind));
                    if hot == Some((*from, *to)) && blink {
                        st = st.bg(SELECTED_BG).add_modifier(Modifier::BOLD);
                    }
                    Cell::from(Line::from(vec![
                        Span::styled(trunc(label, 18), st),
                        Span::styled(format!(" ×{n}"), Style::new().fg(DIM)),
                    ]))
                }
                None if from == to => Cell::from(""),
                None => Cell::from(Span::styled("·", Style::new().fg(BORDER))),
            };
            row.push(cell);
        }
        Row::new(row)
    });
    let mut widths = vec![Constraint::Length(15)];
    widths.extend(receivers.iter().map(|_| Constraint::Fill(1)));
    let table = Table::new(rows, widths)
        .header(Row::new(header))
        .column_spacing(1)
        .block(pane(i.t("pane-matrix"), false));
    frame.render_widget(table, area);
}
