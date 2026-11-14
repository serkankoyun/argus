//! Shared widgets, colors and formatting helpers.

use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Span;
use ratatui::widgets::Block;

use crate::app::App;
use crate::model::{Agent, AgentState, Millis, MsgKind, Series};

pub const ACCENT: Color = Color::Rgb(88, 166, 255);
pub const DIM: Color = Color::Rgb(139, 148, 158);
pub const FG: Color = Color::Rgb(230, 237, 243);
pub const GREEN: Color = Color::Rgb(63, 185, 80);
pub const GREEN_DIM: Color = Color::Rgb(35, 134, 54);
pub const AMBER: Color = Color::Rgb(210, 153, 34);
pub const AMBER_DIM: Color = Color::Rgb(125, 91, 18);
pub const RED: Color = Color::Rgb(248, 81, 73);
pub const CYAN: Color = Color::Rgb(57, 197, 207);
pub const PURPLE: Color = Color::Rgb(188, 140, 255);
pub const BORDER: Color = Color::Rgb(48, 54, 61);
pub const SELECTED_BG: Color = Color::Rgb(31, 42, 55);

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// A bordered pane with a title, highlighted when focused.
pub fn pane<'a>(title: impl Into<String>, focused: bool) -> Block<'a> {
    Block::bordered()
        .border_style(Style::new().fg(if focused { ACCENT } else { BORDER }))
        .title(Span::styled(
            format!(" {} ", title.into()),
            Style::new().fg(ACCENT).bold(),
        ))
}

pub fn bar(pct: u8, width: usize, full: char, empty: char) -> String {
    let filled = ((pct as usize * width + 50) / 100).min(width);
    std::iter::repeat_n(full, filled)
        .chain(std::iter::repeat_n(empty, width - filled))
        .collect()
}

pub fn level(pct: u8) -> Color {
    match pct {
        85.. => RED,
        70.. => AMBER,
        _ => GREEN,
    }
}

pub fn state_color(s: AgentState) -> Color {
    match s {
        AgentState::Working => GREEN,
        AgentState::Waiting => AMBER,
        AgentState::Done => ACCENT,
        AgentState::Idle => DIM,
        AgentState::Stalled => RED,
    }
}

/// State color that pulses for active states, like the mockups.
pub fn pulse_color(app: &App, s: AgentState) -> Color {
    let on = (app.frame / 8) % 2 == 0;
    match s {
        AgentState::Working => {
            if on {
                GREEN
            } else {
                GREEN_DIM
            }
        }
        AgentState::Waiting => {
            if on {
                AMBER
            } else {
                AMBER_DIM
            }
        }
        other => state_color(other),
    }
}

pub fn kind_color(k: MsgKind) -> Color {
    match k {
        MsgKind::Task => ACCENT,
        MsgKind::Msg => CYAN,
        MsgKind::Result => GREEN,
        MsgKind::Perm => AMBER,
        MsgKind::Finding => RED,
    }
}

pub fn spinner(app: &App) -> &'static str {
    SPINNER[(app.frame / 2) as usize % SPINNER.len()]
}

pub fn duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}h{m:02}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{s}s")
    }
}

/// Local wall-clock time, `HH:MM:SS`.
pub fn clock(ts: Millis) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ts)
        .single()
        .map(|d| d.format("%H:%M:%S").to_string())
        .unwrap_or_else(|| "--:--:--".into())
}

pub fn tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1e6)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1e3)
    } else {
        n.to_string()
    }
}

/// Unicode block sparkline of the last `width` samples.
pub fn sparkline(series: &Series, width: usize) -> String {
    const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let data: Vec<f64> = series
        .samples
        .iter()
        .rev()
        .take(width)
        .rev()
        .copied()
        .collect();
    let max = data.iter().copied().fold(0.0_f64, f64::max);
    let min = data.iter().copied().fold(f64::INFINITY, f64::min).min(max);
    let span = (max - min).max(1e-9);
    let mut s: String = data
        .iter()
        .map(|v| BLOCKS[(((v - min) / span) * 7.0).round().clamp(0.0, 7.0) as usize])
        .collect();
    while s.chars().count() < width {
        s.insert(0, ' ');
    }
    s
}

/// What the agent is doing right now, localized.
pub fn now_text(app: &App, a: &Agent) -> String {
    let i = &app.i18n;
    let target = a.now.as_str();
    match a.state {
        AgentState::Working if !a.tool.is_empty() && a.tool != "—" => {
            format!("{} {}", a.tool, target).trim().to_string()
        }
        AgentState::Working => i.t("now-thinking"),
        AgentState::Waiting => i.t_args(
            "now-waiting",
            &[("what", format!("{} {}", a.tool, target).trim())],
        ),
        AgentState::Idle => i.t("now-idle"),
        AgentState::Done => i.t("now-done"),
        AgentState::Stalled => i.t_args("now-stalled", &[("what", a.tool.as_str())]),
    }
}

pub fn bold() -> Style {
    Style::new().add_modifier(Modifier::BOLD)
}

/// Truncates to `n` characters with an ellipsis.
pub fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() > n {
        let mut t: String = s.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        s.to_string()
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
    fn formats() {
        assert_eq!(duration(540), "9m");
        assert_eq!(duration(4320), "1h12m");
        assert_eq!(duration(42), "42s");
        assert_eq!(tokens(84_200), "84.2k");
        assert_eq!(trunc("abcdef", 4), "abc…");
    }

    #[test]
    fn sparkline_scales() {
        let mut s = Series::default();
        for v in [0.0, 1.0, 2.0, 3.0] {
            s.push(v);
        }
        assert_eq!(sparkline(&s, 6), "  ▁▃▆█");
    }
}
