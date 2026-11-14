//! View 8 — Channel: an IRC-style live log of what agents say to each
//! other, interleaved with their actions, plus a member list.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::common::*;
use crate::app::App;
use crate::model::{AgentState, Millis, MsgKind, YOU};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let [chans, log, members] = Layout::horizontal([
        Constraint::Length(26),
        Constraint::Min(40),
        Constraint::Length(36),
    ])
    .areas(area);
    draw_channels(frame, chans, app);
    draw_log(frame, log, app);
    draw_members(frame, members, app);
}

fn draw_channels(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let i = &app.i18n;
    let mut teams: Vec<String> = s
        .agents
        .iter()
        .map(|a| a.team.clone().unwrap_or_else(|| a.project.clone()))
        .collect();
    teams.sort();
    teams.dedup();
    let mut lines: Vec<Line> = Vec::new();
    for (k, t) in teams.iter().enumerate() {
        let st = if k == 0 {
            Style::new().fg(FG).bg(SELECTED_BG)
        } else {
            Style::new().fg(DIM)
        };
        lines.push(Line::styled(format!("# {}", trunc(t, 22)), st));
    }
    if s.messages.iter().any(|m| m.to == YOU) {
        lines.push(Line::styled(
            format!("# {}", i.t("chan-approvals")),
            Style::new().fg(AMBER),
        ));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(i.t("chan-direct"), Style::new().fg(DIM)));
    let mut pairs: Vec<(String, String)> = Vec::new();
    for m in s.messages.iter().filter(|m| m.kind == MsgKind::Msg) {
        let p = if m.from < m.to {
            (m.from.clone(), m.to.clone())
        } else {
            (m.to.clone(), m.from.clone())
        };
        if !pairs.contains(&p) {
            pairs.push(p);
        }
    }
    for (a, b) in pairs.iter().take(6) {
        lines.push(Line::from(vec![
            Span::styled("◆ ", Style::new().fg(CYAN)),
            Span::raw(format!(
                "{} ⇄ {}",
                trunc(&name(app, a), 9),
                trunc(&name(app, b), 9)
            )),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(i.t("chan-hosts"), Style::new().fg(DIM)));
    for h in &s.hosts {
        let (d, c) = if h.connected {
            ("●", GREEN)
        } else {
            ("◐", AMBER)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{d} "), Style::new().fg(c)),
            Span::raw(h.name.clone()),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("pane-channels"), false)),
        area,
    );
}

enum Entry<'a> {
    Msg(&'a crate::model::Message),
    Action(&'a crate::model::ToolCall),
}

impl Entry<'_> {
    fn ts(&self) -> Millis {
        match self {
            Entry::Msg(m) => m.ts,
            Entry::Action(t) => t.ts,
        }
    }
}

fn draw_log(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let i = &app.i18n;
    let rows = area.height.saturating_sub(4) as usize;
    let mut entries: Vec<Entry> = s.messages.iter().map(Entry::Msg).collect();
    // Only every few tool calls, so conversation stays readable.
    entries.extend(
        s.tools
            .iter()
            .filter(|t| matches!(t.tool.as_str(), "Edit" | "Write" | "MultiEdit" | "Bash"))
            .map(Entry::Action),
    );
    entries.sort_by_key(Entry::ts);
    let start = entries.len().saturating_sub(rows);
    let mut lines: Vec<Line> = Vec::new();
    for (k, e) in entries[start..].iter().enumerate() {
        let last = start + k + 1 == entries.len();
        let time = Span::styled(format!("{} ", clock(e.ts())), Style::new().fg(DIM));
        let mut line = match e {
            Entry::Msg(m) if m.kind == MsgKind::Perm => Line::from(vec![
                time,
                Span::styled(format!("{:>14} ", "-!-"), Style::new().fg(AMBER)),
                Span::styled(
                    i.t_args(
                        "chan-perm",
                        &[
                            ("who", name(app, &m.from).as_str()),
                            ("what", m.label.as_str()),
                        ],
                    ),
                    bold().fg(AMBER),
                ),
            ]),
            Entry::Msg(m) => {
                let to = if m.to == YOU {
                    i.t("you")
                } else {
                    name(app, &m.to)
                };
                let body = if m.text.is_empty() { &m.label } else { &m.text };
                let nick = format!("<{}>", trunc(&name(app, &m.from), 12));
                Line::from(vec![
                    time,
                    Span::styled(format!("{nick:>14} "), Style::new().fg(PURPLE)),
                    Span::styled(format!("@{to} "), Style::new().fg(CYAN)),
                    Span::styled(
                        format!("[{}] ", i.t(m.kind.key())),
                        Style::new().fg(kind_color(m.kind)),
                    ),
                    Span::raw(body.clone()),
                ])
            }
            Entry::Action(t) => Line::from(vec![
                time,
                Span::styled(format!("{:>14} ", "*"), Style::new().fg(DIM)),
                Span::styled(
                    format!("{} {} {}", name(app, &t.agent), t.tool, t.target),
                    Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
                ),
            ]),
        };
        if last {
            line = line.style(Style::new().bg(SELECTED_BG));
        }
        lines.push(line);
    }
    let cursor = if (app.frame / 8) % 2 == 0 { "▌" } else { " " };
    let title = s
        .agents
        .first()
        .and_then(|a| a.team.clone())
        .unwrap_or_else(|| i.t("view-channel"));
    let block = pane(format!("#{title}"), true).title_bottom(Line::from(vec![
        Span::styled(format!(" [{}] ", i.t("you")), Style::new().fg(AMBER)),
        Span::styled(cursor, Style::new().fg(GREEN)),
        Span::styled(
            format!("  {} ", i.t("chan-input-hint")),
            Style::new().fg(DIM),
        ),
    ]));
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn draw_members(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    for a in app.ordered() {
        let c = pulse_color(app, a.state);
        let spin = match a.state {
            AgentState::Working => spinner(app),
            other => other.symbol(),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", a.state.symbol()), Style::new().fg(c)),
            Span::styled(trunc(&a.name, 18), bold().fg(PURPLE)),
            Span::styled(format!(" {}", a.host), Style::new().fg(DIM)),
        ]));
        lines.push(Line::from(vec![
            Span::styled(format!("  {spin} "), Style::new().fg(c)),
            Span::raw(trunc(&now_text(app, a), 30)),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).block(pane(app.i18n.t("pane-members"), false)),
        area,
    );
}
