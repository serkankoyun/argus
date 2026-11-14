//! Teams view: the coordinator on top, one card per busy team (each agent with
//! its task and what it does now), finished teams on one line, and the requests
//! that cross team boundaries.

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
    let coords: Vec<_> = s
        .agents
        .iter()
        .filter(|a| a.coordinator && a.parent.is_none())
        .collect();
    if order.is_empty() && coords.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(i.t("flow-empty"), Style::new().fg(DIM)))
                .block(pane(i.t("view-teams"), true)),
            area,
        );
        return;
    }
    let live = |t: &String| {
        s.agents
            .iter()
            .any(|a| a.team_key() == *t && !a.coordinator && a.state != AgentState::Done)
    };
    // Busy teams get cards; teams whose agents all finished share one line.
    let mut busy: Vec<&String> = order.iter().filter(|t| live(t)).collect();
    let idle: Vec<&String> = order.iter().filter(|t| !live(t)).collect();
    if app.show_finished {
        busy = order.iter().collect();
    }
    let coord_h = if coords.is_empty() {
        0
    } else {
        2 + coords.len() as u16 * 2
    };
    let cross_h = if area.height > 40 { 7 } else { 0 };
    let idle_h = if idle.is_empty() || app.show_finished {
        0
    } else {
        3
    };
    let [top, cards, idle_area, cross] = Layout::vertical([
        Constraint::Length(coord_h),
        Constraint::Min(6),
        Constraint::Length(idle_h),
        Constraint::Length(cross_h),
    ])
    .areas(area);
    if coord_h > 0 {
        draw_coordinators(frame, top, app, &coords);
    }
    let cursor_team = order.get(app.team_cursor % order.len().max(1));
    if !busy.is_empty() {
        // Wide cards: one column per busy team, up to what the width allows.
        let max_cols = (cards.width / 56).clamp(1, 3) as usize;
        let cols = busy.len().min(max_cols);
        let rows = busy.len().div_ceil(cols);
        let row_areas = Layout::vertical(vec![Constraint::Fill(1); rows]).split(cards);
        for (r, chunk) in busy.chunks(cols).enumerate() {
            let col_areas = Layout::horizontal(vec![Constraint::Fill(1); cols]).split(row_areas[r]);
            for (c, team) in chunk.iter().enumerate() {
                draw_card(frame, col_areas[c], app, team, cursor_team == Some(*team));
            }
        }
    }
    if idle_h > 0 {
        let mut spans = vec![Span::styled(
            format!("{}: ", i.t("teams-idle")),
            Style::new().fg(DIM),
        )];
        for t in &idle {
            let done = s.agents.iter().filter(|a| a.team_key() == **t).count();
            let style = if cursor_team == Some(*t) {
                bold().fg(ACCENT).bg(SELECTED_BG)
            } else {
                Style::new().fg(FG)
            };
            spans.push(Span::styled(t.to_string(), style));
            spans.push(Span::styled(format!(" ✓{done}   "), Style::new().fg(DIM)));
        }
        frame.render_widget(
            Paragraph::new(Line::from(spans)).block(pane(String::new(), false)),
            idle_area,
        );
    }
    if cross_h > 0 {
        draw_cross(frame, cross, app);
    }
}

/// The sessions that hand out the work, above the team cards.
fn draw_coordinators(frame: &mut Frame, area: Rect, app: &App, coords: &[&crate::model::Agent]) {
    let w = area.width.saturating_sub(4) as usize;
    let mut lines = Vec::new();
    for a in coords {
        let c = pulse_color(app, a.state);
        lines.push(Line::from(vec![
            Span::styled("◆ ", Style::new().fg(ACCENT)),
            Span::styled(a.name.clone(), bold().fg(FG)),
            Span::styled(format!("   {}", now_text(app, a)), Style::new().fg(c)),
            Span::styled(format!("   ${:.2}", a.cost_usd), Style::new().fg(DIM)),
        ]));
        let task = task_text(&a.task);
        lines.push(Line::styled(
            format!("  {}", trunc(task, w.saturating_sub(2))),
            Style::new().fg(DIM),
        ));
    }
    frame.render_widget(
        Paragraph::new(lines).block(pane(app.i18n.t("teams-coordinator"), true)),
        area,
    );
}

fn draw_card(frame: &mut Frame, area: Rect, app: &App, team: &str, cursor: bool) {
    let scope = app.host_scope();
    let s = &scope;
    let i = &app.i18n;
    let members: Vec<_> = s
        .agents
        .iter()
        .filter(|a| a.team_key() == team && !a.coordinator)
        .collect();
    let count = |st: AgentState| members.iter().filter(|a| a.state == st).count();
    let w = area.width.saturating_sub(2) as usize;
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!(
                "● {} {}  ",
                count(AgentState::Working),
                i.t("state-working")
            ),
            Style::new().fg(GREEN),
        ),
        Span::styled(
            format!(
                "◐ {} {}  ",
                count(AgentState::Waiting) + count(AgentState::Stalled),
                i.t("state-waiting")
            ),
            Style::new().fg(AMBER),
        ),
        Span::styled(
            format!("✓ {} {}", count(AgentState::Done), i.t("state-done")),
            Style::new().fg(DIM),
        ),
    ])];
    lines.push(Line::styled("─".repeat(w), Style::new().fg(BORDER)));
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
    // Team leads first, each followed by the agents it started (indented).
    let ids: Vec<&str> = listed.iter().map(|a| a.id.as_str()).collect();
    let is_child = |a: &crate::model::Agent| a.parent.as_deref().is_some_and(|p| ids.contains(&p));
    let mut ordered: Vec<(&crate::model::Agent, &str)> = Vec::new();
    for a in listed.iter().filter(|a| !is_child(a)) {
        ordered.push((a, ""));
        for c in listed
            .iter()
            .filter(|c| c.parent.as_deref() == Some(a.id.as_str()))
        {
            ordered.push((c, "  "));
        }
    }
    // Each agent: who, the task it was given, and what it does now.
    let room = (area.height.saturating_sub(4) as usize) / 3;
    for (a, indent) in ordered.iter().take(room.max(1)) {
        let c = pulse_color(app, a.state);
        let glyph = if a.state == AgentState::Working {
            spinner(app)
        } else {
            a.state.symbol()
        };
        let activity = match a.state {
            AgentState::Working if !a.tool.is_empty() => plain_activity(app, &a.tool, &a.now),
            _ => now_text(app, a),
        };
        lines.push(Line::from(vec![
            Span::raw(indent.to_string()),
            Span::styled(format!("{glyph} "), Style::new().fg(c)),
            Span::styled(trunc(&a.name, w.saturating_sub(4)), bold().fg(FG)),
        ]));
        let task = task_text(&a.task);
        if !task.is_empty() {
            lines.push(Line::styled(
                format!("{indent}  {}", trunc(task, w.saturating_sub(4))),
                Style::new().fg(FG),
            ));
        }
        lines.push(Line::styled(
            format!("{indent}  {}", trunc(&activity, w.saturating_sub(4))),
            Style::new().fg(c),
        ));
    }
    if ordered.len() > room.max(1) {
        lines.push(Line::styled(
            format!("  +{}", ordered.len() - room.max(1)),
            Style::new().fg(DIM),
        ));
    }
    let title = match s.team_label(team) {
        Some(label) => format!("{label} · {team}"),
        None => team.to_string(),
    };
    let mut block = pane(title, cursor || app.team.as_deref() == Some(team));
    if cursor {
        block = block.title_bottom(Line::styled(
            format!(" enter: {} ", i.t("teams-open")),
            Style::new().fg(ACCENT),
        ));
    }
    frame.render_widget(Paragraph::new(lines).block(block), area);
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
