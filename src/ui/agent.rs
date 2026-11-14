//! View 3 — Agent detail: everything about the selected agent.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Bar, BarChart, BarGroup, Cell, Paragraph, Row, Table, Wrap};
use ratatui::Frame;

use super::common::*;
use super::lanes::tool_color;
use crate::app::App;
use crate::model::{Agent, AgentState};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let Some(a) = app.selected_agent() else {
        let p = Paragraph::new(Line::styled(app.i18n.t("agent-none"), Style::new().fg(DIM)))
            .block(pane(app.i18n.t("view-agent"), true));
        frame.render_widget(p, area);
        return;
    };
    let [head, top, mid, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(8),
        Constraint::Min(8),
        Constraint::Length(9),
    ])
    .areas(area);
    draw_head(frame, head, app, a);
    let [ctx, cost, thr] = Layout::horizontal([
        Constraint::Length(44),
        Constraint::Length(38),
        Constraint::Min(20),
    ])
    .areas(top);
    draw_context(frame, ctx, app, a);
    draw_cost(frame, cost, app, a);
    draw_throughput(frame, thr, app, a);
    let [tools, files] =
        Layout::horizontal([Constraint::Min(40), Constraint::Length(48)]).areas(mid);
    draw_tools(frame, tools, app, a);
    draw_files(frame, files, app, a);
    draw_transcript(frame, bottom, app, a);
}

fn draw_head(frame: &mut Frame, area: Rect, app: &App, a: &Agent) {
    let i = &app.i18n;
    let c = pulse_color(app, a.state);
    let parent = a
        .parent
        .as_deref()
        .map(|p| name(app, p))
        .unwrap_or_else(|| "—".into());
    let line = Line::from(vec![
        Span::styled(format!("{} ", a.state.symbol()), Style::new().fg(c)),
        Span::styled(format!("{} ", a.name), bold().fg(PURPLE)),
        Span::styled(format!("@ {}  ", a.host), Style::new().fg(DIM)),
        Span::styled(format!("{} ", i.t("agent-model")), Style::new().fg(DIM)),
        Span::raw(format!(
            "{}  ",
            if a.model.is_empty() { "?" } else { &a.model }
        )),
        Span::styled(format!("{} ", i.t("agent-parent")), Style::new().fg(DIM)),
        Span::raw(format!("{parent}  ")),
        Span::styled(format!("{} ", i.t("agent-project")), Style::new().fg(DIM)),
        Span::raw(format!("{}  ", a.project)),
        Span::styled(format!("{} ", i.t("agent-branch")), Style::new().fg(DIM)),
        Span::raw(format!("{}  ", a.git_branch)),
        Span::styled(
            format!(
                "{} {}",
                i.t(a.state.key()),
                duration(a.elapsed_secs(app.now))
            ),
            Style::new().fg(c),
        ),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn kv<'a>(k: String, v: String) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{k:<14}"), Style::new().fg(DIM)),
        Span::styled(v, Style::new().fg(FG)),
    ])
}

fn draw_context(frame: &mut Frame, area: Rect, app: &App, a: &Agent) {
    let i = &app.i18n;
    let pct = a.context_pct();
    let w = area.width.saturating_sub(8) as usize;
    let lines = vec![
        Line::from(vec![
            Span::styled(bar(pct, w, '█', '░'), Style::new().fg(level(pct))),
            Span::styled(format!(" {pct}%"), bold().fg(FG)),
        ]),
        Line::raw(""),
        kv(
            i.t("agent-ctx-used"),
            format!(
                "{} / {}",
                tokens(a.context_tokens),
                tokens(a.context_window)
            ),
        ),
        kv(
            i.t("agent-ctx-free"),
            tokens(a.context_window.saturating_sub(a.context_tokens)),
        ),
        kv(
            i.t("agent-compact"),
            if pct >= 92 {
                i.t("agent-compact-now")
            } else {
                i.t_args("agent-compact-at", &[("p", "92")])
            },
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("agent-context"), false)),
        area,
    );
}

fn draw_cost(frame: &mut Frame, area: Rect, app: &App, a: &Agent) {
    let i = &app.i18n;
    let mins = (a.elapsed_secs(app.now) as f64 / 60.0).max(1.0);
    let lines = vec![
        kv(i.t("agent-input"), tokens(a.usage.input)),
        kv(i.t("agent-output"), tokens(a.usage.output)),
        kv(
            i.t("agent-cache-read"),
            format!(
                "{} ({}% hit)",
                tokens(a.usage.cache_read),
                a.usage.cache_hit_pct()
            ),
        ),
        kv(i.t("agent-cache-write"), tokens(a.usage.cache_write)),
        kv(
            i.t("agent-cost"),
            format!("${:.2} · ${:.3}/min", a.cost_usd, a.cost_usd / mins),
        ),
        kv(
            i.t("agent-tools"),
            format!("{} · {} {}", a.tool_calls, a.errors, i.t("agent-errors")),
        ),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(pane(i.t("agent-tokens"), false)),
        area,
    );
}

fn draw_throughput(frame: &mut Frame, area: Rect, app: &App, a: &Agent) {
    // Tool calls per minute over the last 30 minutes.
    let buckets = 30usize;
    let mut counts = vec![0u64; buckets];
    for t in app.snapshot.tools.iter().filter(|t| t.agent == a.id) {
        let age_min = ((app.now - t.ts) / 60_000) as usize;
        if age_min < buckets {
            counts[buckets - 1 - age_min] += 1;
        }
    }
    let bars: Vec<Bar> = counts
        .iter()
        .enumerate()
        .map(|(k, v)| {
            Bar::default()
                .value(*v)
                .text_value(String::new())
                .style(Style::new().fg(if k == buckets - 1 { FG } else { GREEN }))
        })
        .collect();
    let chart = BarChart::default()
        .block(pane(app.i18n.t("agent-activity"), false))
        .data(BarGroup::default().bars(&bars))
        .bar_width(1)
        .bar_gap(1);
    frame.render_widget(chart, area);
}

fn draw_tools(frame: &mut Frame, area: Rect, app: &App, a: &Agent) {
    let i = &app.i18n;
    let header = Row::new(
        [
            "col-time",
            "col-tool",
            "col-target",
            "col-dur",
            "col-status",
        ]
        .map(|k| Cell::from(i.t(k))),
    )
    .style(Style::new().fg(DIM));
    let rows = app
        .snapshot
        .tools
        .iter()
        .rev()
        .filter(|t| t.agent == a.id)
        .take(area.height.saturating_sub(3) as usize)
        .map(|t| {
            let (status, sc) = match t.ok {
                Some(true) => ("✓ ok".to_string(), DIM),
                Some(false) => ("✗ error".to_string(), RED),
                None => (format!("{} {}", spinner(app), i.t("status-running")), GREEN),
            };
            Row::new(vec![
                Cell::from(Span::styled(clock(t.ts), Style::new().fg(DIM))),
                Cell::from(Span::styled(
                    t.tool.clone(),
                    Style::new().fg(tool_color(&t.tool)),
                )),
                Cell::from(t.target.clone()),
                Cell::from(Span::styled(
                    t.duration_ms
                        .map(|d| format!("{:.1}s", d as f64 / 1000.0))
                        .unwrap_or_else(|| "…".into()),
                    Style::new().fg(DIM),
                )),
                Cell::from(Span::styled(status, Style::new().fg(sc))),
            ])
        });
    let widths = [
        Constraint::Length(9),
        Constraint::Length(12),
        Constraint::Min(20),
        Constraint::Length(7),
        Constraint::Length(12),
    ];
    let t = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .block(pane(i.t("agent-tool-calls"), true));
    frame.render_widget(t, area);
}

fn draw_files(frame: &mut Frame, area: Rect, app: &App, a: &Agent) {
    let i = &app.i18n;
    let mut lines: Vec<Line> = a
        .files
        .iter()
        .rev()
        .take(area.height.saturating_sub(3) as usize)
        .map(|(p, add, del)| {
            Line::from(vec![
                Span::raw(format!("{:<34}", trunc(p, 33))),
                Span::styled(format!("{:>6}", format!("+{add}")), Style::new().fg(GREEN)),
                Span::styled(format!("{:>6}", format!("−{del}")), Style::new().fg(RED)),
            ])
        })
        .collect();
    if lines.is_empty() {
        lines.push(Line::styled(i.t("agent-no-files"), Style::new().fg(DIM)));
    }
    let (add, del) = a.files.iter().fold((0, 0), |(x, y), f| (x + f.1, y + f.2));
    let block = pane(i.t("agent-files"), false).title_bottom(Line::styled(
        format!(" +{add} −{del} "),
        Style::new().fg(DIM),
    ));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_transcript(frame: &mut Frame, area: Rect, app: &App, a: &Agent) {
    let rows = area.height.saturating_sub(2) as usize;
    let mut lines: Vec<Line> = a
        .transcript
        .iter()
        .rev()
        .take(rows)
        .rev()
        .map(|(ts, role, text)| {
            let (label, color) = match role.as_str() {
                "user" => ("user", FG),
                "assistant" => ("assistant", PURPLE),
                "tool" => ("tool", ACCENT),
                "error" => ("error", RED),
                other => (other, DIM),
            };
            Line::from(vec![
                Span::styled(format!("{} ", clock(*ts)), Style::new().fg(DIM)),
                Span::styled(
                    format!("{label:<10}"),
                    Style::new().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(text.lines().next().unwrap_or("").to_string()),
            ])
        })
        .collect();
    if a.state == AgentState::Working {
        if let Some(last) = lines.last_mut() {
            last.push_span(Span::styled(" ▌", Style::new().fg(FG)));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(pane(app.i18n.t("agent-transcript"), false)),
        area,
    );
}
