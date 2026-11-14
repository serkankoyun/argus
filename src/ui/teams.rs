//! Teams view: every team (Agent Teams team, or project) as a card, plus the
//! requests that cross team boundaries. `t` filters all views to one team.

use std::collections::HashMap;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::common::*;
use crate::app::App;
use crate::model::{AgentState, MsgKind, Snapshot, TaskStatus, YOU};

/// Team key of an agent id in the full snapshot (`None` for the human).
fn team_of(s: &Snapshot, id: &str) -> Option<String> {
    s.agent(id).map(|a| a.team_key())
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let scope = app.host_scope();
    let s = &scope;
    let teams = s.teams();
    let cross_h = 9;
    let [cards, cross] =
        Layout::vertical([Constraint::Min(8), Constraint::Length(cross_h)]).areas(area);
    if teams.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(app.i18n.t("flow-empty"), Style::new().fg(DIM)))
                .block(pane(app.i18n.t("view-teams"), true)),
            area,
        );
        return;
    }
    let per_row = if cards.width >= 150 {
        3
    } else if cards.width >= 90 {
        2
    } else {
        1
    };
    let rows = teams.len().div_ceil(per_row);
    let row_areas = Layout::vertical(vec![Constraint::Fill(1); rows]).split(cards);
    for (r, chunk) in teams.chunks(per_row).enumerate() {
        let mut cons = vec![Constraint::Fill(1); chunk.len()];
        cons.resize(per_row, Constraint::Fill(1));
        let cols = Layout::horizontal(cons).split(row_areas[r]);
        for (c, team) in chunk.iter().enumerate() {
            let idx = r * per_row + c;
            draw_card(
                frame,
                cols[c],
                app,
                team,
                idx == app.team_cursor % teams.len(),
            );
        }
    }
    draw_cross(frame, cross, app);
}

fn draw_card(frame: &mut Frame, area: Rect, app: &App, team: &str, cursor: bool) {
    let scope = app.host_scope();
    let s = &scope;
    let i = &app.i18n;
    let mut members: Vec<_> = s.agents.iter().filter(|a| a.team_key() == team).collect();
    // Live members first, then the most recently finished.
    members.sort_by_key(|a| {
        (
            a.state == AgentState::Done,
            std::cmp::Reverse(a.last_activity),
        )
    });
    let count = |st: AgentState| members.iter().filter(|a| a.state == st).count();
    let ids: Vec<&str> = members.iter().map(|a| a.id.as_str()).collect();
    let window = 15 * 60_000;
    let recent = s.messages.iter().filter(|m| app.now - m.ts < window);
    let (mut inner, mut out, mut inbound) = (0, 0, 0);
    for m in recent {
        let (f, t) = (ids.contains(&m.from.as_str()), ids.contains(&m.to.as_str()));
        match (f, t) {
            (true, true) => inner += 1,
            (true, false) if m.to != YOU => out += 1,
            (false, true) => inbound += 1,
            _ => {}
        }
    }
    let approvals = s
        .messages
        .iter()
        .filter(|m| {
            m.kind == MsgKind::Perm && m.answered_at.is_none() && ids.contains(&m.from.as_str())
        })
        .count();
    let names: Vec<&str> = members.iter().map(|a| a.name.as_str()).collect();
    let open_tasks = s
        .tasks
        .iter()
        .filter(|t| {
            t.status != TaskStatus::Done
                && t.owner
                    .as_deref()
                    .is_some_and(|o| ids.contains(&o) || names.contains(&o))
        })
        .count();
    let cost: f64 = members.iter().map(|a| a.cost_usd).sum();
    let tokens_sum: u64 = members.iter().map(|a| a.tokens()).sum();
    let mut hosts: Vec<&str> = members.iter().map(|a| a.host.as_str()).collect();
    hosts.sort();
    hosts.dedup();

    let selected = app.team.as_deref() == Some(team);
    let mut lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", members.len()), bold().fg(FG)),
            Span::styled(format!("{}  ", i.t("teams-agents")), Style::new().fg(DIM)),
            Span::styled(
                format!("● {} ", count(AgentState::Working)),
                Style::new().fg(GREEN),
            ),
            Span::styled(
                format!("◐ {} ", count(AgentState::Waiting)),
                Style::new().fg(AMBER),
            ),
            Span::styled(
                format!("✗ {} ", count(AgentState::Stalled)),
                Style::new().fg(RED),
            ),
            Span::styled(
                format!("✓ {}", count(AgentState::Done)),
                Style::new().fg(ACCENT),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("${cost:.2}  "), bold().fg(FG)),
            Span::styled(
                format!("{} tok  ", tokens(tokens_sum)),
                Style::new().fg(DIM),
            ),
            Span::styled(
                i.t_args("teams-tasks", &[("n", open_tasks.to_string().as_str())]),
                Style::new().fg(ACCENT),
            ),
        ]),
        Line::from(vec![
            Span::styled(format!("{} ", i.t("teams-msgs")), Style::new().fg(DIM)),
            Span::styled(format!("⇄ {inner}  "), Style::new().fg(CYAN)),
            Span::styled(format!("→ {out}  "), Style::new().fg(PURPLE)),
            Span::styled(format!("← {inbound}"), Style::new().fg(PURPLE)),
            if approvals > 0 {
                Span::styled(
                    format!(
                        "   ▲ {}",
                        i.t_args(
                            "approvals-pending",
                            &[("n", approvals.to_string().as_str())]
                        )
                    ),
                    Style::new().fg(AMBER),
                )
            } else {
                Span::raw("")
            },
        ]),
        Line::styled(
            "─".repeat(area.width.saturating_sub(2) as usize),
            Style::new().fg(BORDER),
        ),
    ];
    let room = area.height.saturating_sub(6) as usize;
    // Names usually repeat the team ("qa-karar-kural-liste-alimi" in karar-kural):
    // drop it inside the card and give the name more room.
    let name_w = (area.width as usize / 2).clamp(12, 34);
    // Finished members only count; they are listed when `f` shows finished agents.
    let listed: Vec<_> = members
        .iter()
        .filter(|a| app.show_finished || a.state != AgentState::Done)
        .collect();
    for a in listed.iter().take(room) {
        let short_name = compact_name(&a.name, team);
        let c = pulse_color(app, a.state);
        let spin = if a.state == AgentState::Working {
            spinner(app)
        } else {
            a.state.symbol()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{spin} "), Style::new().fg(c)),
            Span::styled(
                format!("{:<w$} ", trunc(&short_name, name_w), w = name_w),
                bold().fg(FG),
            ),
            Span::styled(
                trunc(
                    &now_text(app, a),
                    area.width.saturating_sub(name_w as u16 + 6) as usize,
                ),
                Style::new().fg(DIM),
            ),
        ]));
    }
    if listed.len() > room {
        lines.push(Line::styled(
            format!("  +{}", listed.len() - room),
            Style::new().fg(DIM),
        ));
    }
    let title = match s.team_label(team) {
        Some(label) => format!("{team} · {label}  · {}", hosts.join(", ")),
        None => format!("{}  · {}", team, hosts.join(", ")),
    };
    let hint = if cursor {
        format!(" enter: {} ", i.t("teams-open"))
    } else if selected {
        format!(" {} ", i.t("teams-filtered"))
    } else {
        String::new()
    };
    let mut block =
        pane(title, selected || cursor).title_bottom(Line::styled(hint, Style::new().fg(ACCENT)));
    if cursor {
        block = block.border_type(ratatui::widgets::BorderType::Thick);
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

/// `qa-karar-kural-liste-alimi` in team `karar-kural` -> `qa-liste-alimi`.
pub fn compact_name(name: &str, team: &str) -> String {
    if team.is_empty() || name == team {
        return name.to_string();
    }
    let mid = format!("-{team}-");
    if let Some(i) = name.find(&mid) {
        return format!("{}-{}", &name[..i], &name[i + mid.len()..]);
    }
    if let Some(rest) = name.strip_prefix(&format!("{team}-")) {
        return rest.to_string();
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    fn team_is_dropped_from_member_names() {
        use super::compact_name;
        assert_eq!(
            compact_name("qa-karar-kural-liste-alimi", "karar-kural"),
            "qa-liste-alimi"
        );
        assert_eq!(compact_name("provizyon-x", "provizyon"), "x");
        assert_eq!(compact_name("Claimer#1b4b", "ekipsiz"), "Claimer#1b4b");
    }
}
