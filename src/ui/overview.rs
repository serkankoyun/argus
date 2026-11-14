//! View 1 — Overview: hosts, agent tree, flow summary, metrics and events.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::common::*;
use crate::app::App;

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let agent_rows = app.snapshot.agents.len().max(3) as u16 + 3;
    let [top, mid, events] = Layout::vertical([
        Constraint::Length(agent_rows.clamp(8, 16)),
        Constraint::Min(8),
        Constraint::Length(7),
    ])
    .areas(area);
    let [hosts, agents] =
        Layout::horizontal([Constraint::Length(30), Constraint::Min(0)]).areas(top);
    let [flow, metrics] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(44)]).areas(mid);
    draw_hosts(frame, hosts, app);
    draw_agents(frame, agents, app);
    draw_flow(frame, flow, app);
    draw_metrics(frame, metrics, app);
    draw_events(frame, events, app);
}

fn draw_hosts(frame: &mut Frame, area: Rect, app: &App) {
    let mut lines = Vec::new();
    for h in &app.snapshot.hosts {
        let (dot, color) = if h.connected {
            ("●", GREEN)
        } else {
            ("◐", AMBER)
        };
        let count = app
            .snapshot
            .agents
            .iter()
            .filter(|a| a.host == h.name)
            .count();
        lines.push(Line::from(vec![
            Span::styled(format!("{dot} "), Style::new().fg(color)),
            Span::styled(format!("{:<8}", trunc(&h.name, 8)), bold().fg(FG)),
            Span::styled(
                format!("{:<11}", trunc(&h.region, 10)),
                Style::new().fg(DIM),
            ),
            Span::raw(count.to_string()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  cpu ", Style::new().fg(DIM)),
            Span::styled(
                bar(h.cpu_pct, 10, '█', '░'),
                Style::new().fg(level(h.cpu_pct)),
            ),
            Span::raw(format!(" {}%", h.cpu_pct)),
        ]));
    }
    let p = Paragraph::new(lines).block(pane(app.i18n.t("pane-hosts"), false));
    frame.render_widget(p, area);
}

fn draw_agents(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
    if app.snapshot.agents.is_empty() {
        let msg = if app.snapshot.taken_at == 0 {
            i.t("loading")
        } else {
            i.t_args("empty", &[("dir", app.snapshot.source.as_str())])
        };
        let p = Paragraph::new(Line::styled(msg, Style::new().fg(DIM)))
            .block(pane(i.t("pane-agents"), true));
        frame.render_widget(p, area);
        return;
    }
    let header = Row::new(
        [
            "col-agent",
            "col-host",
            "col-state",
            "col-tool",
            "col-tokens",
            "col-context",
            "col-cost",
            "col-time",
        ]
        .map(|k| Cell::from(i.t(k))),
    )
    .style(Style::new().fg(DIM));

    let order = app.snapshot.tree_order();
    let rows = order.iter().map(|&(idx, depth)| {
        let a = &app.snapshot.agents[idx];
        let name = if depth == 0 {
            Span::styled(format!("▼ {}", a.name), bold().fg(FG))
        } else {
            Span::raw(format!("{}└ {}", "  ".repeat(depth), a.name))
        };
        let sc = pulse_color(app, a.state);
        let mut row = Row::new(vec![
            Cell::from(name),
            Cell::from(Span::styled(a.host.clone(), Style::new().fg(DIM))),
            Cell::from(Span::styled(
                format!("{} {}", a.state.symbol(), i.t(a.state.key())),
                Style::new().fg(sc),
            )),
            Cell::from(trunc(&a.tool, 12)),
            Cell::from(format!("{:>7}", tokens(a.tokens()))),
            Cell::from(Span::styled(
                format!(
                    "{} {:>2}%",
                    bar(a.context_pct(), 8, '▰', '▱'),
                    a.context_pct()
                ),
                Style::new().fg(level(a.context_pct())),
            )),
            Cell::from(format!("${:.2}", a.cost_usd)),
            Cell::from(Span::styled(
                duration(a.elapsed_secs(app.now)),
                Style::new().fg(DIM),
            )),
        ]);
        if app.selected.as_deref() == Some(a.id.as_str()) {
            row = row.style(Style::new().bg(SELECTED_BG));
        }
        row
    });
    let widths = [
        Constraint::Min(18),
        Constraint::Length(8),
        Constraint::Length(13),
        Constraint::Length(12),
        Constraint::Length(8),
        Constraint::Length(13),
        Constraint::Length(8),
        Constraint::Length(7),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .block(pane(i.t("pane-agents"), true));
    frame.render_widget(table, area);
}

fn draw_flow(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let visible = area.height.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = s
        .messages
        .iter()
        .rev()
        .take(visible)
        .map(|m| {
            let c = kind_color(m.kind);
            Line::from(vec![
                Span::styled(format!("{} ", clock(m.ts)), Style::new().fg(DIM)),
                Span::styled(
                    format!("{:>12}", trunc(s.name_of(&m.from), 12)),
                    Style::new().fg(PURPLE),
                ),
                Span::styled(" ──", Style::new().fg(DIM)),
                Span::styled(format!("{} ", app.i18n.t(m.kind.key())), Style::new().fg(c)),
                Span::styled(trunc(&m.label, 44), Style::new().fg(FG)),
                Span::styled(" ──▶ ", Style::new().fg(DIM)),
                Span::styled(
                    trunc(s.name_of(&m.to), 14),
                    Style::new().fg(if m.to == crate::model::YOU {
                        AMBER
                    } else {
                        PURPLE
                    }),
                ),
            ])
        })
        .collect();
    if lines.is_empty() {
        lines.push(Line::styled(app.i18n.t("flow-empty"), Style::new().fg(DIM)));
    }
    frame.render_widget(
        Paragraph::new(lines).block(pane(app.i18n.t("pane-flow"), false)),
        area,
    );
}

fn draw_metrics(frame: &mut Frame, area: Rect, app: &App) {
    let m = &app.snapshot.metrics;
    let w = area.width.saturating_sub(2 + 11 + 8) as usize;
    let row = |label: &str, series: &crate::model::Series, value: String, color| {
        Line::from(vec![
            Span::styled(format!("{label:<10} "), Style::new().fg(DIM)),
            Span::styled(sparkline(series, w), Style::new().fg(color)),
            Span::styled(format!("{value:>8}"), Style::new().fg(FG)),
        ])
    };
    let i = &app.i18n;
    let lines = vec![
        row(
            &i.t("m-tokens"),
            &m.tokens_per_sec,
            format!("{:.0}", m.tokens_per_sec.last()),
            GREEN,
        ),
        row(
            &i.t("m-cost"),
            &m.cost_per_hour,
            format!("${:.2}", m.cost_per_hour.last()),
            CYAN,
        ),
        row(
            &i.t("m-tools"),
            &m.tools_per_min,
            format!("{:.0}", m.tools_per_min.last()),
            ACCENT,
        ),
        row(
            &i.t("m-msgs"),
            &m.msgs_per_min,
            format!("{:.0}", m.msgs_per_min.last()),
            PURPLE,
        ),
        row(
            &i.t("m-errors"),
            &m.errors,
            format!("{:.0}", m.errors.last()),
            RED,
        ),
        row(
            &i.t("m-ctx"),
            &m.ctx_avg,
            format!("{:.0}%", m.ctx_avg.last()),
            AMBER,
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("pane-metrics"), false)),
        area,
    );
}

fn draw_events(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let visible = area.height.saturating_sub(2) as usize;
    let lines: Vec<Line> = s
        .tools
        .iter()
        .rev()
        .take(visible)
        .map(|t| {
            let host = s.agent(&t.agent).map(|a| a.host.as_str()).unwrap_or("");
            let (mark, color) = match t.ok {
                Some(true) => ("✓", DIM),
                Some(false) => ("✗", RED),
                None => ("●", GREEN),
            };
            let dur = t
                .duration_ms
                .map(|d| format!("{:.1}s", d as f64 / 1000.0))
                .unwrap_or_default();
            Line::from(vec![
                Span::styled(format!("{} ", clock(t.ts)), Style::new().fg(DIM)),
                Span::styled(format!("{:<8}", trunc(host, 8)), Style::new().fg(CYAN)),
                Span::styled(
                    format!("{:<14}", trunc(s.name_of(&t.agent), 13)),
                    Style::new().fg(PURPLE),
                ),
                Span::styled(format!("{mark} "), Style::new().fg(color)),
                Span::styled(
                    format!("{:<10}", trunc(&t.tool, 10)),
                    Style::new().fg(ACCENT),
                ),
                Span::raw(trunc(&t.target, 60)),
                Span::styled(format!("  {dur}"), Style::new().fg(DIM)),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines).block(pane(app.i18n.t("pane-events"), false)),
        area,
    );
}
