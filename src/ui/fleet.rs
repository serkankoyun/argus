//! View 4 — Fleet: every connected host, its health, alerts and costs.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::common::*;
use crate::app::App;
use crate::model::{AgentState, Host};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let rows = app.snapshot.hosts.len() as u16 + 3;
    let [hosts, mid, cards] = Layout::vertical([
        Constraint::Length(rows.max(5)),
        Constraint::Min(8),
        Constraint::Length(8),
    ])
    .areas(area);
    draw_hosts(frame, hosts, app);
    let [alerts, info] =
        Layout::horizontal([Constraint::Min(40), Constraint::Length(48)]).areas(mid);
    draw_alerts(frame, alerts, app);
    draw_info(frame, info, app);
    draw_cards(frame, cards, app);
}

fn host_cost(app: &App, h: &Host) -> f64 {
    app.snapshot
        .agents
        .iter()
        .filter(|a| a.host == h.name)
        .map(|a| a.cost_usd)
        .sum()
}

fn draw_hosts(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
    let header = Row::new(
        [
            "col-host",
            "col-region",
            "col-collector",
            "col-rtt",
            "col-cpu",
            "col-mem",
            "col-agents",
            "col-cost",
            "col-last",
        ]
        .map(|k| Cell::from(i.t(k))),
    )
    .style(Style::new().fg(DIM));
    let rows = app.snapshot.hosts.iter().map(|h| {
        let agents: Vec<_> = app
            .snapshot
            .agents
            .iter()
            .filter(|a| a.host == h.name)
            .collect();
        let active = agents
            .iter()
            .filter(|a| matches!(a.state, AgentState::Working | AgentState::Waiting))
            .count();
        let last = agents.iter().map(|a| a.last_activity).max().unwrap_or(0);
        let (col, cc) = if h.connected {
            (format!("● {}", i.t("collector-ok")), GREEN)
        } else {
            (format!("◐ {}", i.t("collector-down")), AMBER)
        };
        Row::new(vec![
            Cell::from(Span::styled(h.name.clone(), bold().fg(FG))),
            Cell::from(Span::styled(h.region.clone(), Style::new().fg(DIM))),
            Cell::from(Span::styled(col, Style::new().fg(cc))),
            Cell::from(
                h.rtt_ms
                    .map(|r| format!("{r}ms"))
                    .unwrap_or_else(|| "—".into()),
            ),
            Cell::from(Span::styled(
                format!("{} {:>2}%", bar(h.cpu_pct, 10, '█', '░'), h.cpu_pct),
                Style::new().fg(level(h.cpu_pct)),
            )),
            Cell::from(Span::styled(
                format!("{} {:>2}%", bar(h.mem_pct, 10, '█', '░'), h.mem_pct),
                Style::new().fg(level(h.mem_pct)),
            )),
            Cell::from(format!("{active} / {}", agents.len())),
            Cell::from(format!("${:.2}", host_cost(app, h))),
            Cell::from(Span::styled(
                if last > 0 {
                    i.t_args(
                        "ago",
                        &[(
                            "t",
                            duration(((app.now - last) / 1000).max(0) as u64).as_str(),
                        )],
                    )
                } else {
                    "—".into()
                },
                Style::new().fg(DIM),
            )),
        ])
    });
    let widths = [
        Constraint::Length(12),
        Constraint::Length(12),
        Constraint::Length(16),
        Constraint::Length(7),
        Constraint::Length(15),
        Constraint::Length(15),
        Constraint::Length(8),
        Constraint::Length(9),
        Constraint::Min(8),
    ];
    frame.render_widget(
        Table::new(rows, widths)
            .header(header)
            .column_spacing(1)
            .block(pane(i.t("pane-hosts"), true)),
        area,
    );
}

fn draw_alerts(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
    let mut lines = Vec::new();
    for a in &app.snapshot.agents {
        let item = match a.state {
            AgentState::Waiting => Some((
                "▲",
                AMBER,
                i.t_args("alert-waiting", &[("what", now_text(app, a).as_str())]),
            )),
            AgentState::Stalled => Some((
                "✗",
                RED,
                i.t_args("alert-stalled", &[("what", a.tool.as_str())]),
            )),
            _ if a.context_pct() >= 85 => Some((
                "▲",
                AMBER,
                i.t_args(
                    "alert-context",
                    &[("p", a.context_pct().to_string().as_str())],
                ),
            )),
            _ => None,
        };
        if let Some((g, c, text)) = item {
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", clock(a.last_activity)), Style::new().fg(DIM)),
                Span::styled(format!("{g} "), Style::new().fg(c)),
                Span::styled(format!("{:<9}", trunc(&a.host, 8)), Style::new().fg(CYAN)),
                Span::styled(
                    format!("{:<13}", trunc(&a.name, 12)),
                    Style::new().fg(PURPLE),
                ),
                Span::raw(text),
            ]));
        }
    }
    for h in app.snapshot.hosts.iter().filter(|h| !h.connected) {
        lines.push(Line::from(vec![
            Span::styled("         ▲ ", Style::new().fg(AMBER)),
            Span::styled(format!("{:<9}", trunc(&h.name, 8)), Style::new().fg(CYAN)),
            Span::raw(i.t("alert-host-down")),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::styled(i.t("alerts-none"), Style::new().fg(GREEN)));
    }
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("pane-alerts"), false)),
        area,
    );
}

fn draw_info(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
    let s = &app.snapshot;
    let lines = vec![
        Line::from(vec![
            Span::styled(format!("{:<14}", i.t("info-source")), Style::new().fg(DIM)),
            Span::raw(trunc(&s.source, 30)),
        ]),
        Line::from(vec![
            Span::styled(format!("{:<14}", i.t("info-hosts")), Style::new().fg(DIM)),
            Span::raw(s.hosts.len().to_string()),
        ]),
        Line::from(vec![
            Span::styled(format!("{:<14}", i.t("info-agents")), Style::new().fg(DIM)),
            Span::raw(s.agents.len().to_string()),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{:<14}", i.t("info-messages")),
                Style::new().fg(DIM),
            ),
            Span::raw(s.messages.len().to_string()),
        ]),
        Line::from(vec![
            Span::styled(format!("{:<14}", i.t("info-cost")), Style::new().fg(DIM)),
            Span::raw(format!("${:.2}", s.total_cost())),
        ]),
        Line::raw(""),
        Line::styled(i.t("info-connect"), Style::new().fg(DIM)),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("pane-hub"), false)),
        area,
    );
}

fn draw_cards(frame: &mut Frame, area: Rect, app: &App) {
    let hosts = &app.snapshot.hosts;
    if hosts.is_empty() {
        return;
    }
    let constraints: Vec<Constraint> = hosts.iter().take(4).map(|_| Constraint::Fill(1)).collect();
    let areas = Layout::horizontal(constraints).split(area);
    for (h, a) in hosts.iter().zip(areas.iter()) {
        let mut agents: Vec<_> = app
            .snapshot
            .agents
            .iter()
            .filter(|x| x.host == h.name)
            .collect();
        agents.sort_by(|x, y| y.cost_usd.total_cmp(&x.cost_usd));
        let max = agents.first().map(|x| x.cost_usd).unwrap_or(0.0).max(0.01);
        let w = a.width.saturating_sub(26) as usize;
        let lines: Vec<Line> = agents
            .iter()
            .take(a.height.saturating_sub(2) as usize)
            .map(|x| {
                let n = ((x.cost_usd / max) * w as f64).round() as usize;
                Line::from(vec![
                    Span::styled(
                        format!("{:<12}", trunc(&x.name, 11)),
                        Style::new().fg(PURPLE),
                    ),
                    Span::styled("█".repeat(n), Style::new().fg(GREEN)),
                    Span::styled("░".repeat(w.saturating_sub(n)), Style::new().fg(BORDER)),
                    Span::styled(format!(" ${:.2}", x.cost_usd), Style::new().fg(FG)),
                ])
            })
            .collect();
        let title = app
            .i18n
            .t_args("cost-by-agent", &[("host", h.name.as_str())]);
        frame.render_widget(Paragraph::new(lines).block(pane(title, false)), *a);
    }
}
