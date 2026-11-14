//! View 1 — Overview: hosts and the agent tree with live metrics.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use super::{ACCENT, AMBER, BORDER, DIM, GREEN, RED};
use crate::app::App;
use crate::model::{Agent, AgentState};

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    let [hosts, agents] =
        Layout::horizontal([Constraint::Length(30), Constraint::Min(0)]).areas(area);
    draw_hosts(frame, hosts, app);
    draw_agents(frame, agents, app);
}

fn pane<'a>(title: String, focused: bool) -> Block<'a> {
    Block::bordered()
        .border_style(Style::new().fg(if focused { ACCENT } else { BORDER }))
        .title(Span::styled(
            format!(" {title} "),
            Style::new().fg(ACCENT).bold(),
        ))
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
            Span::styled(
                format!("{:<8}", h.name),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(format!("{:<11}", h.region), Style::new().fg(DIM)),
            Span::raw(format!("{count}")),
        ]));
        lines.push(Line::from(vec![
            Span::styled("  cpu ", Style::new().fg(DIM)),
            Span::styled(
                bar(h.cpu_pct, 10, '█', '░'),
                Style::new().fg(level(h.cpu_pct)),
            ),
            Span::raw(format!(" {}%", h.cpu_pct)),
        ]));
        lines.push(Line::raw(""));
    }
    let p = Paragraph::new(lines).block(pane(app.i18n.t("pane-hosts"), false));
    frame.render_widget(p, area);
}

fn draw_agents(frame: &mut Frame, area: Rect, app: &App) {
    let i = &app.i18n;
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

    let agents = &app.snapshot.agents;
    let rows = agents.iter().enumerate().map(|(idx, a)| {
        let name = tree_label(a, agents, idx);
        let mut row = Row::new(vec![
            Cell::from(name),
            Cell::from(Span::styled(a.host.clone(), Style::new().fg(DIM))),
            Cell::from(Span::styled(
                format!("{} {}", a.state.symbol(), i.t(a.state.key())),
                Style::new().fg(state_color(a.state)),
            )),
            Cell::from(a.tool.clone()),
            Cell::from(format!("{:>6.1}k", a.tokens as f64 / 1000.0)),
            Cell::from(Span::styled(
                format!("{} {:>2}%", bar(a.context_pct, 8, '▰', '▱'), a.context_pct),
                Style::new().fg(level(a.context_pct)),
            )),
            Cell::from(format!("${:.2}", a.cost_usd)),
            Cell::from(Span::styled(duration(a.elapsed_secs), Style::new().fg(DIM))),
        ]);
        if idx == app.selected {
            row = row.style(Style::new().bg(ratatui::style::Color::Rgb(31, 42, 55)));
        }
        row
    });

    let widths = [
        Constraint::Length(18),
        Constraint::Length(8),
        Constraint::Length(13),
        Constraint::Length(10),
        Constraint::Length(9),
        Constraint::Length(14),
        Constraint::Length(8),
        Constraint::Length(7),
    ];
    let table = Table::new(rows, widths)
        .header(header)
        .column_spacing(1)
        .block(pane(i.t("pane-agents"), true));
    frame.render_widget(table, area);
}

/// Renders `▼ lead` for roots and `├─ child` / `└─ child` for children.
fn tree_label(a: &Agent, all: &[Agent], idx: usize) -> Line<'static> {
    match &a.parent {
        None => Line::from(Span::styled(
            format!("▼ {}", a.name),
            Style::new().add_modifier(Modifier::BOLD),
        )),
        Some(parent) => {
            let is_last = !all[idx + 1..]
                .iter()
                .any(|b| b.parent.as_deref() == Some(parent.as_str()));
            let branch = if is_last { "└─" } else { "├─" };
            Line::raw(format!("  {branch} {}", a.name))
        }
    }
}

fn bar(pct: u8, width: usize, full: char, empty: char) -> String {
    let filled = (pct as usize * width + 50) / 100;
    let filled = filled.min(width);
    std::iter::repeat(full)
        .take(filled)
        .chain(std::iter::repeat(empty).take(width - filled))
        .collect()
}

fn level(pct: u8) -> ratatui::style::Color {
    match pct {
        85.. => RED,
        70.. => AMBER,
        _ => GREEN,
    }
}

fn state_color(s: AgentState) -> ratatui::style::Color {
    match s {
        AgentState::Working => GREEN,
        AgentState::Waiting => AMBER,
        AgentState::Done => ACCENT,
        AgentState::Idle => DIM,
        AgentState::Stalled => RED,
    }
}

fn duration(secs: u64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_fills_proportionally() {
        assert_eq!(bar(0, 4, '█', '░'), "░░░░");
        assert_eq!(bar(50, 4, '█', '░'), "██░░");
        assert_eq!(bar(100, 4, '█', '░'), "████");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(duration(540), "9m");
        assert_eq!(duration(4320), "1h12m");
    }
}
