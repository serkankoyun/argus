//! Teams view: a tree with the coordinator on top, busy teams with their
//! agents (task and activity per agent) below, finished teams collapsed to one
//! line each, and the requests that cross team boundaries.

use std::collections::HashMap;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::common::*;
use crate::app::App;
use crate::model::{AgentState, MsgKind, Snapshot};

/// Team key of an agent id in the full snapshot (`None` for the human).
fn team_of(s: &Snapshot, id: &str) -> Option<String> {
    s.agent(id).map(|a| a.team_key())
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let scope = app.host_scope();
    let s = &scope;
    let i = &app.i18n;
    let order = app.team_order();
    let cross_h = 8.min(area.height / 3);
    let [tree, cross] =
        Layout::vertical([Constraint::Min(6), Constraint::Length(cross_h)]).areas(area);
    if order.is_empty() && !s.agents.iter().any(|a| a.coordinator) {
        frame.render_widget(
            Paragraph::new(Line::styled(i.t("flow-empty"), Style::new().fg(DIM)))
                .block(pane(i.t("view-teams"), true)),
            area,
        );
        return;
    }
    let width = tree.width.saturating_sub(4) as usize;
    let name_w = (width / 4).clamp(14, 34);
    let task_w = (width * 2 / 5).clamp(10, 70);
    let act_w = width.saturating_sub(name_w + task_w + 8);
    let member = |a: &crate::model::Agent, lead: &str| -> Line<'static> {
        let c = pulse_color(app, a.state);
        let glyph = if a.state == AgentState::Working {
            spinner(app).to_string()
        } else {
            a.state.symbol().to_string()
        };
        let task = task_text(&a.task).to_string();
        Line::from(vec![
            Span::styled(lead.to_string(), Style::new().fg(BORDER)),
            Span::styled(format!("{glyph} "), Style::new().fg(c)),
            Span::styled(
                format!("{:<name_w$} ", trunc(&a.name, name_w)),
                bold().fg(FG),
            ),
            Span::styled(
                format!("{:<task_w$} ", trunc(&task, task_w)),
                Style::new().fg(FG),
            ),
            Span::styled(trunc(&plain_now(app, a), act_w), Style::new().fg(DIM)),
        ])
    };
    let mut lines: Vec<Line> = Vec::new();
    // Coordinators lead the tree.
    for a in s
        .agents
        .iter()
        .filter(|a| a.coordinator && a.parent.is_none())
    {
        lines.push(member(a, ""));
    }
    let mut cursor_line = 0;
    let n = order.len();
    for (idx, team) in order.iter().enumerate() {
        let members: Vec<_> = s
            .agents
            .iter()
            .filter(|a| a.team_key() == *team && !a.coordinator)
            .collect();
        let count = |st: AgentState| members.iter().filter(|a| a.state == st).count();
        let (work, wait, done) = (
            count(AgentState::Working),
            count(AgentState::Waiting) + count(AgentState::Stalled),
            count(AgentState::Done),
        );
        let selected = idx == app.team_cursor % n.max(1);
        if selected {
            cursor_line = lines.len();
        }
        let last = idx + 1 == n;
        let branch = if last { "└─ " } else { "├─ " };
        let title = match s.team_label(team) {
            Some(l) => format!("{l} ({team})"),
            None => team.clone(),
        };
        let bg = if selected {
            Style::new().bg(SELECTED_BG)
        } else {
            Style::new()
        };
        let active = members.iter().any(|a| a.state != AgentState::Done);
        let mut head = vec![
            Span::styled(branch, Style::new().fg(BORDER)),
            Span::styled(
                format!("{title}  "),
                bold().fg(if active { ACCENT } else { DIM }),
            ),
        ];
        if work > 0 {
            head.push(Span::styled(
                format!("● {work} {}  ", i.t("state-working")),
                Style::new().fg(GREEN),
            ));
        }
        if wait > 0 {
            head.push(Span::styled(
                format!("◐ {wait} {}  ", i.t("state-waiting")),
                Style::new().fg(AMBER),
            ));
        }
        head.push(Span::styled(
            format!("✓ {done} {}", i.t("state-done")),
            Style::new().fg(DIM),
        ));
        if selected {
            head.push(Span::styled(
                format!("   enter: {}", i.t("teams-open")),
                Style::new().fg(DIM),
            ));
        }
        lines.push(Line::from(head).style(bg));
        if active || app.show_finished {
            let lead = if last { "    " } else { "│   " };
            let mut listed: Vec<_> = members
                .iter()
                .filter(|a| app.show_finished || a.state != AgentState::Done)
                .collect();
            listed.sort_by_key(|a| {
                (
                    a.state == AgentState::Done,
                    std::cmp::Reverse(a.last_activity),
                )
            });
            for a in listed {
                lines.push(member(a, lead));
            }
        }
    }
    // Keep the selected team in view.
    let inner = tree.height.saturating_sub(2) as usize;
    let scroll = cursor_line.saturating_sub(inner.saturating_sub(4)) as u16;
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((scroll, 0))
            .block(pane(i.t("view-teams"), true)),
        tree,
    );
    if cross_h >= 4 {
        draw_cross(frame, cross, app);
    }
}

/// Activity in plain words (the raw command stays in the agent view).
fn plain_now(app: &App, a: &crate::model::Agent) -> String {
    match a.state {
        AgentState::Working if !a.tool.is_empty() => plain_activity(app, &a.tool, &a.now),
        _ => now_text(app, a),
    }
}

fn draw_cross(frame: &mut Frame, area: Rect, app: &App) {
    let scope = app.host_scope();
    let s = &scope;
    let i = &app.i18n;
    let mut pairs: HashMap<(String, String), (usize, String, MsgKind, i64)> = HashMap::new();
    for m in &s.messages {
        let (Some(a), Some(b)) = (team_of(s, &m.from), team_of(s, &m.to)) else {
            continue;
        };
        if a == b {
            continue;
        }
        let e = pairs.entry((a, b)).or_insert((0, String::new(), m.kind, 0));
        e.0 += 1;
        e.1 = format!("{} → {}: {}", name(app, &m.from), name(app, &m.to), m.label);
        e.2 = m.kind;
        e.3 = m.ts;
    }
    let mut list: Vec<_> = pairs.into_iter().collect();
    list.sort_by_key(|(_, v)| std::cmp::Reverse(v.3));
    let header = Row::new(
        ["teams-from", "teams-to", "teams-count", "teams-latest"].map(|k| Cell::from(i.t(k))),
    )
    .style(Style::new().fg(DIM));
    let rows = list
        .into_iter()
        .take(area.height.saturating_sub(3) as usize)
        .map(|((a, b), (n, label, kind, ts))| {
            let fresh = app.now - ts < 4_000;
            let st = if fresh {
                Style::new()
                    .fg(kind_color(kind))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(kind_color(kind))
            };
            Row::new(vec![
                Cell::from(Span::styled(a, bold().fg(FG))),
                Cell::from(Span::styled(b, bold().fg(FG))),
                Cell::from(format!("{n}")),
                Cell::from(Line::from(vec![
                    Span::styled(format!("{} ", kind.glyph()), st),
                    Span::raw(label),
                ])),
            ])
        });
    let widths = [
        Constraint::Length(18),
        Constraint::Length(18),
        Constraint::Length(6),
        Constraint::Min(20),
    ];
    frame.render_widget(
        Table::new(rows, widths)
            .header(header)
            .column_spacing(1)
            .block(pane(i.t("teams-cross"), false)),
        area,
    );
}
