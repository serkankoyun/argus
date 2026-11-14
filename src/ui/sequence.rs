//! View 2 — Sequence: agents as columns, messages as arrows between them,
//! newest at the bottom. Side panes: task board and the latest message.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget, Wrap};
use ratatui::Frame;

use super::common::*;
use crate::app::App;
use crate::model::{TaskStatus, YOU};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let [seq, side] = Layout::horizontal([Constraint::Min(50), Constraint::Length(46)]).areas(area);
    let [tasks, detail] =
        Layout::vertical([Constraint::Min(8), Constraint::Length(12)]).areas(side);
    let block = pane(app.i18n.t("pane-sequence"), true);
    let inner = block.inner(seq);
    frame.render_widget(block, seq);
    draw_sequence(frame.buffer_mut(), inner, app);
    draw_tasks(frame, tasks, app);
    draw_detail(frame, detail, app);
}

/// Participants: agents that appear in recent messages, in tree order.
fn participants(app: &App) -> Vec<String> {
    let s = &app.snapshot;
    let mut ids: Vec<String> = app
        .ordered()
        .iter()
        .filter(|a| s.messages.iter().any(|m| m.from == a.id || m.to == a.id))
        .map(|a| a.id.clone())
        .collect();
    for m in &s.messages {
        for id in [&m.from, &m.to] {
            if !ids.contains(id) && id != YOU {
                ids.push(id.clone());
            }
        }
    }
    if s.messages.iter().any(|m| m.to == YOU || m.from == YOU) {
        ids.push(YOU.to_string());
    }
    ids
}

fn draw_sequence(buf: &mut Buffer, area: Rect, app: &App) {
    let s = &app.snapshot;
    let cols = participants(app);
    if cols.is_empty() || area.width < 20 {
        Paragraph::new(Line::styled(app.i18n.t("flow-empty"), Style::new().fg(DIM)))
            .render(area, buf);
        return;
    }
    let time_w = 9u16;
    let lane_w = (area.width.saturating_sub(time_w) / cols.len() as u16).max(8);
    let cx = |i: usize| area.x + time_w + lane_w * i as u16 + lane_w / 2;
    // Headers and lifelines.
    for (i, id) in cols.iter().enumerate() {
        let label = trunc(&name(app, id), lane_w.saturating_sub(2) as usize);
        let x = cx(i).saturating_sub(label.chars().count() as u16 / 2);
        let color = if id == YOU { AMBER } else { PURPLE };
        buf.set_string(x, area.y, &label, bold().fg(color));
        for y in area.y + 1..area.y + area.height {
            buf.set_string(cx(i), y, "┆", Style::new().fg(BORDER));
        }
    }
    let rows = (area.height.saturating_sub(2) / 2) as usize;
    let msgs: Vec<_> = s.messages.iter().rev().take(rows).collect();
    for (k, m) in msgs.iter().rev().enumerate() {
        let y = area.y + 2 + k as u16 * 2;
        let (Some(a), Some(b)) = (
            cols.iter().position(|c| *c == m.from),
            cols.iter().position(|c| *c == m.to),
        ) else {
            continue;
        };
        let c = kind_color(m.kind);
        let fresh = app.now - m.ts < 3_000;
        buf.set_string(area.x, y + 1, clock(m.ts), Style::new().fg(DIM));
        if a == b {
            continue;
        }
        let (lo, hi) = (cx(a.min(b)), cx(a.max(b)));
        let style = if fresh {
            Style::new().fg(c).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(c)
        };
        for x in lo + 1..hi {
            buf.set_string(x, y + 1, "─", style);
        }
        if b > a {
            buf.set_string(hi - 1, y + 1, "►", style);
        } else {
            buf.set_string(lo + 1, y + 1, "◄", style);
        }
        let label = trunc(&format!(" {} {} ", app.i18n.t(m.kind.key()), m.label), 44);
        let len = label.chars().count() as u16;
        let left_edge = area.x + time_w;
        let right_edge = area.x + area.width;
        let lx = ((lo + hi) / 2)
            .saturating_sub(len / 2)
            .clamp(left_edge, right_edge.saturating_sub(len).max(left_edge));
        buf.set_stringn(
            lx,
            y,
            &label,
            (right_edge - lx) as usize,
            Style::new().fg(FG),
        );
        buf.set_string(lx + 1, y, app.i18n.t(m.kind.key()), Style::new().fg(c));
    }
}

fn draw_tasks(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let i = &app.i18n;
    let mut lines = Vec::new();
    for (status, color) in [
        (TaskStatus::InProgress, GREEN),
        (TaskStatus::Blocked, AMBER),
        (TaskStatus::Pending, ACCENT),
        (TaskStatus::Done, DIM),
    ] {
        let items: Vec<_> = s.tasks.iter().filter(|t| t.status == status).collect();
        if items.is_empty() {
            continue;
        }
        lines.push(Line::from(vec![
            Span::styled(i.t(status.key()), bold().fg(color)),
            Span::styled(format!(" ({})", items.len()), Style::new().fg(DIM)),
        ]));
        for t in items {
            let owner = t
                .owner
                .as_deref()
                .map(|o| name(app, o))
                .unwrap_or_else(|| i.t("unassigned"));
            lines.push(Line::from(vec![
                Span::styled(format!(" #{:<4}", t.id), Style::new().fg(DIM)),
                Span::raw(format!("{:<24}", trunc(&t.title, 23))),
                Span::styled(trunc(&owner, 14), Style::new().fg(PURPLE)),
            ]));
        }
    }
    if lines.is_empty() {
        lines.push(Line::styled(i.t("tasks-none"), Style::new().fg(DIM)));
    }
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("pane-tasks"), false)),
        area,
    );
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
    let Some(m) = app.snapshot.messages.last() else {
        frame.render_widget(
            Paragraph::new("").block(pane(i.t("pane-message"), false)),
            area,
        );
        return;
    };
    let c = kind_color(m.kind);
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{} ", i.t(m.kind.key())), bold().fg(c)),
            Span::styled(name(app, &m.from), bold().fg(FG)),
            Span::styled(" ──► ", Style::new().fg(DIM)),
            Span::styled(
                name(app, &m.to),
                bold().fg(if m.to == YOU { AMBER } else { PURPLE }),
            ),
        ]),
        Line::styled(clock(m.ts), Style::new().fg(DIM)),
        Line::raw(""),
        Line::styled(
            if m.text.is_empty() {
                m.label.clone()
            } else {
                m.text.clone()
            },
            Style::new().fg(FG),
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(pane(i.t("pane-message"), false)),
        area,
    );
}
