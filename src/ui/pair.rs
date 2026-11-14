//! View 0 — Pair: two agents side by side with the conversation between
//! them in the middle. Left = selected agent, right = its busiest partner.

use std::collections::HashMap;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use ratatui::Frame;

use super::common::*;
use super::graph::TRAVEL;
use crate::app::App;
use crate::model::{Agent, AgentState, Message};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(left) = app.selected_agent() else {
        frame.render_widget(
            Paragraph::new(app.i18n.t("agent-none")).block(pane(app.i18n.t("view-pair"), true)),
            area,
        );
        return;
    };
    let partner = partner_of(app, left);
    let [main, others] = Layout::vertical([Constraint::Min(10), Constraint::Length(5)]).areas(area);
    let [l, mid, r] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(40),
        Constraint::Fill(1),
    ])
    .areas(main);
    let between: Vec<&Message> = match partner {
        Some(p) => app
            .snapshot
            .messages
            .iter()
            .filter(|m| (m.from == left.id && m.to == p.id) || (m.from == p.id && m.to == left.id))
            .collect(),
        None => Vec::new(),
    };
    draw_terminal(frame, l, app, left, &between);
    draw_channel(frame, mid, app, left, partner, &between);
    match partner {
        Some(p) => draw_terminal(frame, r, app, p, &between),
        None => frame.render_widget(
            Paragraph::new(app.i18n.t("pair-none")).block(pane("—", false)),
            r,
        ),
    }
    draw_others(frame, others, app, left, partner);
}

/// The agent `a` exchanged the most messages with (falls back to its parent).
fn partner_of<'a>(app: &'a App, a: &Agent) -> Option<&'a Agent> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for m in &app.snapshot.messages {
        if m.from == a.id {
            *counts.entry(&m.to).or_default() += 1;
        } else if m.to == a.id {
            *counts.entry(&m.from).or_default() += 1;
        }
    }
    let best = counts
        .into_iter()
        .filter(|(id, _)| app.snapshot.agent(id).is_some())
        .max_by_key(|(id, n)| (*n, std::cmp::Reverse(id.to_string())))
        .map(|(id, _)| id.to_string())
        .or_else(|| a.parent.clone());
    best.and_then(|id| app.snapshot.agent(&id))
}

fn draw_terminal(frame: &mut Frame, area: Rect, app: &App, a: &Agent, between: &[&Message]) {
    let active = a.state == AgentState::Working;
    let c = pulse_color(app, a.state);
    let rows = area.height.saturating_sub(3) as usize;
    let mut lines: Vec<Line> = vec![Line::styled(
        format!(
            "{} · ctx {}% · ${:.2} · {}",
            a.project,
            a.context_pct(),
            a.cost_usd,
            trunc(&a.git_branch, 24)
        ),
        Style::new().fg(DIM),
    )];
    // Transcript plus incoming messages from the partner, by time.
    let mut items: Vec<(i64, Line)> = a
        .transcript
        .iter()
        .map(|(ts, role, text)| {
            let line = match role.as_str() {
                "tool" => Line::from(vec![
                    Span::styled("● ", Style::new().fg(GREEN)),
                    Span::styled(text.clone(), Style::new().fg(FG)),
                ]),
                "assistant" => Line::styled(
                    format!("✻ {text}"),
                    Style::new().fg(PURPLE).add_modifier(Modifier::ITALIC),
                ),
                "error" => Line::styled(format!("  ⎿ {text}"), Style::new().fg(RED)),
                _ => Line::styled(format!("> {text}"), Style::new().fg(FG)),
            };
            (*ts, line)
        })
        .collect();
    for m in between
        .iter()
        .filter(|m| m.to == a.id && app.now - m.ts >= TRAVEL)
    {
        items.push((
            m.ts + TRAVEL,
            Line::styled(
                format!(
                    "◄ {}: \"{}\"",
                    name(app, &m.from),
                    if m.text.is_empty() { &m.label } else { &m.text }
                ),
                bold().fg(CYAN),
            ),
        ));
    }
    items.sort_by_key(|(ts, _)| *ts);
    let start = items.len().saturating_sub(rows);
    lines.extend(items.into_iter().skip(start).map(|(_, l)| l));
    if active {
        if let Some(last) = lines.last_mut() {
            last.push_span(Span::styled(" ▌", Style::new().fg(FG)));
        }
    }
    let state = if active {
        format!("{} {}", spinner(app), app.i18n.t("state-working"))
    } else {
        format!("{} {}", a.state.symbol(), app.i18n.t(a.state.key()))
    };
    let block = pane(a.name.clone(), active)
        .border_style(Style::new().fg(if active { c } else { BORDER }))
        .title(Line::styled(format!(" {state} "), Style::new().fg(c)).right_aligned());
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block),
        area,
    );
}

fn draw_channel(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    left: &Agent,
    right: Option<&Agent>,
    between: &[&Message],
) {
    let i = &app.i18n;
    let inner_w = area.width.saturating_sub(4) as usize;
    let mut lines = Vec::new();
    let rname = right.map(|r| r.name.clone()).unwrap_or_default();
    lines.push(Line::from(vec![
        Span::styled(trunc(&left.name, inner_w / 2), Style::new().fg(PURPLE)),
        Span::raw(" ".repeat(inner_w.saturating_sub(
            left.name.chars().count().min(inner_w / 2) + rname.chars().count().min(inner_w / 2),
        ))),
        Span::styled(trunc(&rname, inner_w / 2), Style::new().fg(CYAN)),
    ]));
    // Track with the message currently in flight.
    let mut track = vec![' '; inner_w];
    for (k, ch) in track.iter_mut().enumerate() {
        if k % 2 == 0 {
            *ch = '·';
        }
    }
    let flying = between
        .iter()
        .rev()
        .find(|m| (0..TRAVEL).contains(&(app.now - m.ts)));
    let mut track_line = Line::styled(track.iter().collect::<String>(), Style::new().fg(BORDER));
    if let Some(m) = flying {
        let p = (app.now - m.ts) as f64 / TRAVEL as f64;
        let ltr = m.from == left.id;
        let x = if ltr { p } else { 1.0 - p };
        let pos = ((inner_w.saturating_sub(2)) as f64 * x).round() as usize;
        let glyph = if ltr { "◆►" } else { "◄◆" };
        let before: String = track.iter().take(pos).collect();
        let after: String = track.iter().skip(pos + 2).collect();
        track_line = Line::from(vec![
            Span::styled(before, Style::new().fg(BORDER)),
            Span::styled(glyph, bold().fg(kind_color(m.kind))),
            Span::styled(after, Style::new().fg(BORDER)),
        ]);
    }
    lines.push(track_line);
    lines.push(Line::raw(""));
    match flying.or(between.last()) {
        Some(m) => {
            let c = kind_color(m.kind);
            lines.push(Line::styled(
                format!(
                    "{} → {} · {}",
                    name(app, &m.from),
                    name(app, &m.to),
                    i.t(m.kind.key())
                ),
                Style::new().fg(c),
            ));
            lines.push(Line::styled(
                if m.text.is_empty() {
                    m.label.clone()
                } else {
                    m.text.clone()
                },
                Style::new().fg(FG),
            ));
        }
        None => lines.push(Line::styled(i.t("pair-quiet"), Style::new().fg(DIM))),
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled(i.t("pair-history"), Style::new().fg(DIM)));
    for m in between.iter().rev().skip(1).take(6) {
        let ltr = m.from == left.id;
        let c = if ltr { PURPLE } else { CYAN };
        let indent = if ltr { "" } else { "   " };
        lines.push(Line::styled(
            format!("{indent}{} ► {}", name(app, &m.from), name(app, &m.to)),
            Style::new().fg(c),
        ));
        lines.push(Line::raw(format!(
            "{indent}{}",
            trunc(&m.label, inner_w.saturating_sub(3))
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(pane(i.t("pair-channel"), true)),
        area,
    );
}

fn draw_others(frame: &mut Frame, area: Rect, app: &App, left: &Agent, right: Option<&Agent>) {
    let others: Vec<&Agent> = app
        .ordered()
        .into_iter()
        .filter(|a| a.id != left.id && Some(a.id.as_str()) != right.map(|r| r.id.as_str()))
        .collect();
    let cols = 3usize;
    let per = others.len().div_ceil(cols).max(1);
    let areas = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .split(pane("", false).inner(area));
    frame.render_widget(pane(app.i18n.t("pair-others"), false), area);
    for (k, chunk) in others.chunks(per).enumerate().take(cols) {
        let lines: Vec<Line> = chunk
            .iter()
            .map(|a| {
                let c = pulse_color(app, a.state);
                let spin = if a.state == AgentState::Working {
                    spinner(app)
                } else {
                    a.state.symbol()
                };
                Line::from(vec![
                    Span::styled(format!("{spin} "), Style::new().fg(c)),
                    Span::styled(format!("{} ", trunc(&a.name, 12)), bold().fg(FG)),
                    Span::styled(now_text(app, a), Style::new().fg(DIM)),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), areas[k]);
    }
}
