//! Shared widgets, colors and formatting helpers.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
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

/// Writes `s` at (x, y) only if that cell lies inside both `clip` and the buffer,
/// truncating at the right edge. Custom-drawn views use this instead of
/// `Buffer::set_string`, which panics outside the buffer.
pub fn put<S: AsRef<str>>(buf: &mut Buffer, clip: Rect, x: u16, y: u16, s: S, style: Style) {
    putn(buf, clip, x, y, s, usize::MAX, style);
}

/// Like [`put`], writing at most `n` columns.
pub fn putn<S: AsRef<str>>(
    buf: &mut Buffer,
    clip: Rect,
    x: u16,
    y: u16,
    s: S,
    n: usize,
    style: Style,
) {
    let r = clip.intersection(buf.area);
    if !r.contains(Position { x, y }) {
        return;
    }
    let room = (r.x + r.width - x) as usize;
    buf.set_stringn(x, y, s, n.min(room), style);
}

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

/// Display name for an agent id; the human operator is localized.
pub fn name(app: &App, id: &str) -> String {
    if id == crate::model::YOU {
        app.i18n.t("you")
    } else {
        let n = app.full.name_of(id);
        if n != id {
            return n.to_string();
        }
        // An agent Argus has not seen yet: name it from the task it was given
        // ("<name> · <run> · <task>" or the description), never by a raw id.
        app.full
            .messages
            .iter()
            .find(|m| m.to == id && m.kind == crate::model::MsgKind::Task && !m.label.is_empty())
            .map(|m| m.label.split(" · ").next().unwrap_or(&m.label).to_string())
            .unwrap_or_else(|| n.to_string())
    }
}

/// What the agent is doing right now, localized.
pub use crate::model::task_text;

pub fn now_text(app: &App, a: &Agent) -> String {
    let i = &app.i18n;
    let target = a.now.as_str();
    match a.state {
        AgentState::Working if target.is_empty() && !a.task.is_empty() => {
            format!("{} · {}", a.tool, task_text(&a.task))
                .trim_start_matches(" · ")
                .to_string()
        }
        AgentState::Working if !a.tool.is_empty() && a.tool != "—" => {
            plain_activity(app, &a.tool, target)
        }
        AgentState::Working => i.t("now-thinking"),
        AgentState::Waiting if target.starts_with("[bg ") => {
            let cmd = target.split_once("] ").map_or(target, |(_, c)| c);
            i.t_args("now-background", &[("what", cmd)])
        }
        AgentState::Waiting if target.starts_with("[wait ") => {
            let who = target.split_once("] ").map_or(target, |(_, c)| c);
            i.t_args("now-children", &[("what", who)])
        }
        AgentState::Waiting => i.t_args(
            "now-waiting",
            &[("what", format!("{} {}", a.tool, target).trim())],
        ),
        AgentState::Idle => i.t("now-idle"),
        AgentState::Done if !a.task.is_empty() => {
            format!("{} · {}", i.t("now-done"), task_text(&a.task))
        }
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

/// Short model label for lists: `claude-opus-5-5` becomes `opus-5.5`. Drops the
/// `claude-` prefix, a date suffix (`-20260301`) and a context tag (`[1m]`), and
/// turns a trailing `-N-M` version into `N.M`. Unknown or empty models show `?`.
pub fn short_model(model: &str) -> String {
    let m = model.trim();
    let m = m.split('[').next().unwrap_or(m);
    let m = m.strip_prefix("claude-").unwrap_or(m);
    let mut parts: Vec<&str> = m.split('-').filter(|p| !p.is_empty()).collect();
    let digits = |p: &str| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit());
    if parts.last().is_some_and(|p| p.len() >= 8 && digits(p)) {
        parts.pop();
    }
    let n = parts.len();
    if n == 0 {
        return "?".into();
    }
    let small = |p: &str| digits(p) && p.len() <= 2;
    if n >= 3 && small(parts[n - 1]) && small(parts[n - 2]) && !digits(parts[n - 3]) {
        return format!(
            "{}-{}.{}",
            parts[..n - 2].join("-"),
            parts[n - 2],
            parts[n - 1]
        );
    }
    parts.join("-")
}

/// What a tool call does, in plain words: "Bash · running tests",
/// "Edit · SetupWizardPage.tsx", "Agent → new tester". The raw command stays in
/// the agent detail view.
pub fn plain_activity(app: &App, tool: &str, target: &str) -> String {
    let i = &app.i18n;
    let base = |p: &str| {
        p.trim_matches('"')
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(p)
            .to_string()
    };
    // Long-running markers like "[6m12s] " or "[bg 3m] " stay in front.
    let (mark, target) = match target.strip_prefix('[').and_then(|t| t.split_once("] ")) {
        Some((m, rest)) => (format!("[{m}] "), rest),
        None => (String::new(), target),
    };
    let what = match tool {
        "Bash" => {
            // The command after any "cd ... &&" and environment assignments.
            let cmd = target.rsplit("&&").next().unwrap_or(target).trim();
            let cmd = cmd.rsplit(';').next().unwrap_or(cmd).trim();
            let words: Vec<&str> = cmd
                .split_whitespace()
                .filter(|w| !w.contains('='))
                .collect();
            let has = |k: &[&str]| words.iter().any(|w| k.contains(&w.trim_matches('"')));
            let key = if has(&["test", "pytest", "vitest", "jest", "playwright", "nextest"]) {
                Some("act-test")
            } else if has(&["build", "tsc", "make", "compile"]) {
                Some("act-build")
            } else if has(&["install", "ci", "add"])
                && has(&["npm", "pnpm", "yarn", "pip", "cargo", "apt"])
            {
                Some("act-install")
            } else if has(&["sleep", "wait", "until"]) {
                Some("act-wait")
            } else if words.first() == Some(&"git") || words.first() == Some(&"gh") {
                None
            } else if has(&[
                "grep", "rg", "find", "ls", "cat", "head", "tail", "sed", "jq",
            ]) {
                Some("act-inspect")
            } else {
                None
            };
            match key {
                Some(k) => i.t(k),
                None => words.iter().take(2).copied().collect::<Vec<_>>().join(" "),
            }
        }
        "Edit" | "Write" | "MultiEdit" | "Read" | "NotebookEdit" => base(target),
        "Agent" | "Task" => format!("→ {}", task_text(target)),
        _ => target.to_string(),
    };
    format!("{mark}{tool} · {what}")
        .trim_end_matches(" · ")
        .to_string()
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
    fn short_model_labels() {
        assert_eq!(short_model("claude-opus-5-5"), "opus-5.5");
        assert_eq!(short_model("claude-sonnet-5-5"), "sonnet-5.5");
        assert_eq!(short_model("claude-haiku-5-5"), "haiku-5.5");
        assert_eq!(short_model("claude-fable-5-2"), "fable-5.2");
        assert_eq!(short_model("claude-sonnet-5-5-20260301"), "sonnet-5.5");
        assert_eq!(short_model("claude-opus-4-1-20250805"), "opus-4.1");
        assert_eq!(short_model("claude-opus-5-5[1m]"), "opus-5.5");
        assert_eq!(short_model("claude-sonnet-4-20250514"), "sonnet-4");
        // Legacy order (version before family) is only stripped, not rewritten.
        assert_eq!(short_model("claude-3-5-sonnet-20241022"), "3-5-sonnet");
        assert_eq!(short_model(""), "?");
        assert_eq!(short_model("  "), "?");
        assert_eq!(short_model("claude-"), "?");
        assert_eq!(short_model("gpt-x"), "gpt-x");
    }

    #[test]
    fn put_ignores_cells_outside_the_buffer() {
        let mut buf = Buffer::empty(Rect::new(0, 0, 10, 3));
        let clip = Rect::new(0, 0, 10, 3);
        put(&mut buf, clip, 17, 122, "x", Style::new());
        put(&mut buf, clip, 8, 1, "abcdef", Style::new());
        assert_eq!(buf[(8, 1)].symbol(), "a");
        assert_eq!(buf[(9, 1)].symbol(), "b");
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
