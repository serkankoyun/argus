//! View 9 — Task map: task dependencies left to right, bottlenecks and the
//! critical path, plus how busy each agent is.

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Widget};
use ratatui::Frame;

use super::common::*;
use super::layout::line_cells;
use crate::app::App;
use crate::model::{AgentState, Task, TaskStatus};

const CARD_W: u16 = 26;
const CARD_H: u16 = 5;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let [top, bottom] = Layout::vertical([Constraint::Min(12), Constraint::Length(7)]).areas(area);
    let [map, side] = Layout::horizontal([Constraint::Min(40), Constraint::Length(40)]).areas(top);
    let block = pane(app.i18n.t("pane-taskmap"), true);
    let inner = block.inner(map);
    frame.render_widget(block, map);
    draw_map(frame.buffer_mut(), inner, app);
    draw_side(frame, side, app);
    draw_utilization(frame, bottom, app);
}

/// Dependency depth of every task (0 = no blockers).
pub fn depths(tasks: &[Task]) -> HashMap<String, usize> {
    fn depth(
        id: &str,
        tasks: &[Task],
        memo: &mut HashMap<String, usize>,
        stack: &mut Vec<String>,
    ) -> usize {
        if let Some(d) = memo.get(id) {
            return *d;
        }
        if stack.iter().any(|s| s == id) {
            return 0; // cycle
        }
        stack.push(id.to_string());
        let d = tasks
            .iter()
            .find(|t| t.id == id)
            .map(|t| {
                t.blocked_by
                    .iter()
                    .map(|b| depth(b, tasks, memo, stack) + 1)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        stack.pop();
        memo.insert(id.to_string(), d);
        d
    }
    let mut memo = HashMap::new();
    for t in tasks {
        depth(&t.id, tasks, &mut memo, &mut Vec::new());
    }
    memo
}

/// Number of tasks that transitively wait on `id`.
pub fn downstream(tasks: &[Task], id: &str) -> usize {
    let mut seen: Vec<&str> = Vec::new();
    let mut stack = vec![id];
    while let Some(cur) = stack.pop() {
        for t in tasks
            .iter()
            .filter(|t| t.blocked_by.iter().any(|b| b == cur))
        {
            if !seen.contains(&t.id.as_str()) {
                seen.push(&t.id);
                stack.push(&t.id);
            }
        }
    }
    seen.len()
}

fn status_color(s: TaskStatus) -> Color {
    match s {
        TaskStatus::Done => GREEN,
        TaskStatus::InProgress => ACCENT,
        TaskStatus::Blocked => AMBER,
        TaskStatus::Pending => DIM,
    }
}

fn draw_map(buf: &mut Buffer, area: Rect, app: &App) {
    let tasks = &app.snapshot.tasks;
    let i = &app.i18n;
    if tasks.is_empty() {
        Paragraph::new(Line::styled(i.t("tasks-none"), Style::new().fg(DIM))).render(area, buf);
        return;
    }
    let d = depths(tasks);
    let cols = d.values().copied().max().unwrap_or(0) + 1;
    let col_w = area.width / cols as u16;
    let mut by_col: Vec<Vec<&Task>> = vec![Vec::new(); cols];
    for t in tasks {
        by_col[d[&t.id]].push(t);
    }
    let mut pos: HashMap<&str, (u16, u16)> = HashMap::new();
    for (c, col) in by_col.iter().enumerate() {
        let row_h = (area.height / col.len().max(1) as u16).max(CARD_H + 1);
        for (r, t) in col.iter().enumerate() {
            let x = area.x + col_w * c as u16 + col_w / 2;
            let y = area.y + row_h * r as u16 + row_h / 2;
            pos.insert(
                &t.id,
                (
                    x,
                    y.min(area.y + area.height.saturating_sub(CARD_H / 2 + 1)),
                ),
            );
        }
    }
    // Dependency edges, colored by the blocker's status; hand-offs travel on edges.
    for t in tasks {
        for b in &t.blocked_by {
            let (Some(&(bx, by)), Some(&(tx, ty))) = (pos.get(b.as_str()), pos.get(t.id.as_str()))
            else {
                continue;
            };
            let from = (bx + CARD_W / 2, by);
            let to = (tx.saturating_sub(CARD_W / 2 + 1), ty);
            let blocker = tasks
                .iter()
                .find(|x| &x.id == b)
                .map(|x| x.status)
                .unwrap_or(TaskStatus::Pending);
            let c = status_color(blocker);
            let cells = line_cells(from, to);
            for &(x, y) in &cells {
                if area.contains(Position { x, y }) {
                    buf.set_string(x, y, "·", Style::new().fg(c));
                }
            }
            if matches!(blocker, TaskStatus::Done | TaskStatus::InProgress) && !cells.is_empty() {
                let phase = ((app.now / 60) as usize + t.id.len() * 7) % (cells.len() * 2);
                if let Some(&(x, y)) = cells.get(phase) {
                    buf.set_string(x, y, "●", Style::new().fg(c));
                }
            }
        }
    }
    for t in tasks {
        let Some(&(x, y)) = pos.get(t.id.as_str()) else {
            continue;
        };
        let r = Rect {
            x: x.saturating_sub(CARD_W / 2).max(area.x),
            y: y.saturating_sub(CARD_H / 2).max(area.y),
            width: CARD_W.min(area.width),
            height: CARD_H,
        }
        .intersection(area);
        Clear.render(r, buf);
        let c = status_color(t.status);
        let blink = (app.frame / 8) % 2 == 0;
        let border = if t.status == TaskStatus::Blocked && !blink {
            AMBER_DIM
        } else {
            c
        };
        let owner = t
            .owner
            .as_deref()
            .map(|o| name(app, o))
            .unwrap_or_else(|| i.t("unassigned"));
        let owner_working = t
            .owner
            .as_deref()
            .and_then(|o| app.snapshot.agent(o))
            .is_some_and(|a| a.state == AgentState::Working);
        let spin = if t.status == TaskStatus::InProgress && owner_working {
            spinner(app)
        } else {
            "·"
        };
        let lines = vec![
            Line::from(vec![
                Span::styled(format!("#{} ", t.id), Style::new().fg(DIM)),
                Span::styled(trunc(&t.title, 20), bold().fg(FG)),
            ]),
            Line::from(vec![
                Span::styled(format!("{spin} "), Style::new().fg(c)),
                Span::styled(trunc(&owner, 20), Style::new().fg(PURPLE)),
            ]),
            Line::styled(i.t(t.status.key()), Style::new().fg(c)),
        ];
        Paragraph::new(lines)
            .block(
                Block::bordered()
                    .border_style(Style::new().fg(border))
                    .style(Style::new().bg(Color::Rgb(22, 27, 34))),
            )
            .render(r, buf);
    }
}

fn draw_side(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
    let tasks = &app.snapshot.tasks;
    let mut lines = Vec::new();
    let mut blocked: Vec<(&Task, usize)> = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Blocked)
        .map(|t| (t, downstream(tasks, &t.id)))
        .collect();
    blocked.sort_by_key(|b| std::cmp::Reverse(b.1));
    lines.push(Line::styled(i.t("bottlenecks"), bold().fg(AMBER)));
    for (t, n) in blocked.iter().take(4) {
        lines.push(Line::from(vec![
            Span::styled(format!("▲ #{} ", t.id), Style::new().fg(AMBER)),
            Span::raw(trunc(&t.title, 28)),
        ]));
        lines.push(Line::styled(
            format!(
                "  → {}",
                i.t_args("holds-n", &[("n", n.to_string().as_str())])
            ),
            Style::new().fg(DIM),
        ));
    }
    if blocked.is_empty() {
        lines.push(Line::styled(
            i.t("bottlenecks-none"),
            Style::new().fg(GREEN),
        ));
    }
    // Critical path: the deepest chain of unfinished tasks.
    let d = depths(tasks);
    let mut path: Vec<&Task> = Vec::new();
    if let Some(end) = tasks
        .iter()
        .filter(|t| t.status != TaskStatus::Done)
        .max_by_key(|t| d.get(&t.id).copied().unwrap_or(0))
    {
        let mut cur = Some(end);
        while let Some(t) = cur {
            path.push(t);
            cur = t
                .blocked_by
                .iter()
                .filter_map(|b| tasks.iter().find(|x| &x.id == b))
                .max_by_key(|x| d.get(&x.id).copied().unwrap_or(0));
        }
    }
    path.reverse();
    lines.push(Line::raw(""));
    lines.push(Line::styled(i.t("critical-path"), bold().fg(ACCENT)));
    let mut spans = Vec::new();
    for (k, t) in path.iter().enumerate() {
        if k > 0 {
            spans.push(Span::styled(" → ", Style::new().fg(DIM)));
        }
        spans.push(Span::styled(
            format!("#{}", t.id),
            Style::new().fg(status_color(t.status)),
        ));
    }
    lines.push(Line::from(spans));
    let done = tasks
        .iter()
        .filter(|t| t.status == TaskStatus::Done)
        .count();
    lines.push(Line::raw(""));
    lines.push(Line::styled(
        i.t_args(
            "tasks-progress",
            &[
                ("done", done.to_string().as_str()),
                ("total", tasks.len().to_string().as_str()),
            ],
        ),
        Style::new().fg(DIM),
    ));
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("pane-bottlenecks"), false)),
        area,
    );
}

fn draw_utilization(frame: &mut Frame, area: Rect, app: &App) {
    // Share of the last 15 minutes each agent spent inside tool calls.
    let window = 15 * 60_000;
    let agents = app.ordered();
    let half = agents.len().div_ceil(2);
    let [l, r] = Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)])
        .areas(pane("", false).inner(area));
    frame.render_widget(pane(app.i18n.t("pane-utilization"), false), area);
    for (col, chunk) in [l, r].into_iter().zip(agents.chunks(half.max(1))) {
        let lines: Vec<Line> = chunk
            .iter()
            .map(|a| {
                let busy: i64 = app
                    .snapshot
                    .tools
                    .iter()
                    .filter(|t| t.agent == a.id && app.now - t.ts < window)
                    .map(|t| t.duration_ms.unwrap_or(app.now - t.ts).max(0))
                    .sum();
                let pct = match a.state {
                    AgentState::Working => ((busy * 100 / window) as u8).clamp(20, 100),
                    AgentState::Waiting => 10,
                    _ => ((busy * 100 / window) as u8).min(100),
                };
                let w = col.width.saturating_sub(34) as usize;
                Line::from(vec![
                    Span::styled(
                        format!("{:<13}", trunc(&a.name, 12)),
                        Style::new().fg(PURPLE),
                    ),
                    Span::styled(
                        bar(pct, w, '█', '░'),
                        Style::new().fg(pulse_color(app, a.state)),
                    ),
                    Span::styled(format!(" {pct:>3}% "), Style::new().fg(FG)),
                    Span::styled(trunc(&now_text(app, a), 14), Style::new().fg(DIM)),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, deps: &[&str]) -> Task {
        Task {
            id: id.into(),
            title: id.into(),
            owner: None,
            status: TaskStatus::Pending,
            blocked_by: deps.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn computes_depths_and_downstream() {
        let ts = vec![t("a", &[]), t("b", &["a"]), t("c", &["b"]), t("d", &["a"])];
        let d = depths(&ts);
        assert_eq!(d["c"], 2);
        assert_eq!(d["d"], 1);
        assert_eq!(downstream(&ts, "a"), 3);
    }

    #[test]
    fn survives_cycles() {
        let ts = vec![t("a", &["b"]), t("b", &["a"])];
        let _ = depths(&ts);
    }
}
