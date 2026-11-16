//! Backlog view (`b`): per owner what runs now, what is queued and what
//! waits (and on whom), then what waits on the user, then the unassigned pool.
//! The model comes from [`crate::queue::build`]; this module only draws it.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::common::*;
use crate::app::App;
use crate::model::Millis;
use crate::queue::{Lane, Origin, QueueItem, Reason};

/// One drawn line: styled pieces, and the item it belongs to.
struct Row {
    parts: Vec<(String, Style)>,
    item: Option<usize>,
}

fn row(parts: Vec<(String, Style)>) -> Row {
    Row { parts, item: None }
}

fn age(app: &App, since: Millis) -> String {
    if since <= 0 {
        "—".into()
    } else {
        duration(((app.now - since).max(0) / 1000) as u64)
    }
}

/// The reason in plain words.
pub fn reason_text(app: &App, r: &Reason) -> String {
    let i = &app.i18n;
    match r {
        Reason::Working => i.t("queue-reason-now"),
        Reason::Turn { pos, of } => i.t_args(
            "queue-reason-turn",
            &[("pos", &pos.to_string()), ("n", &of.to_string())],
        ),
        Reason::WaitsTasks(ids) => {
            let ids: Vec<String> = ids.iter().map(|x| format!("#{x}")).collect();
            i.t_args("queue-reason-tasks", &[("ids", &ids.join(", "))])
        }
        Reason::WaitsStage(st) => i.t_args("queue-reason-stage", &[("stage", st)]),
        Reason::Rejected { stage, decision } => i.t_args(
            "queue-reason-rejected",
            &[
                ("stage", stage.as_str()),
                ("decision", if decision.is_empty() { "—" } else { decision }),
            ],
        ),
        Reason::Blocked(st) if st.is_empty() => i.t("task-blocked"),
        Reason::Blocked(st) => i.t_args("queue-reason-blocked", &[("stage", st)]),
        Reason::WaitsAgent(who) => i.t_args("queue-reason-agent", &[("who", who)]),
        Reason::WaitsYou => i.t("queue-reason-you"),
        Reason::Approval => i.t("queue-reason-approval"),
        Reason::Unassigned => i.t("queue-reason-pool"),
        Reason::WaitsOn(what) => i.t_args("queue-reason-waits-on", &[("what", what)]),
    }
}

/// The source tag in the user's language.
pub fn source_text(app: &App, it: &QueueItem) -> String {
    let i = &app.i18n;
    match &it.origin {
        Origin::Task(id) => i.t_args("queue-src-task", &[("id", id)]),
        Origin::Run(id) => i.t_args("queue-src-run", &[("id", id)]),
        Origin::Request => i.t("queue-src-request"),
        Origin::Agent => i.t("queue-src-agent"),
        Origin::Todo(_) => it.source.clone(),
    }
}

fn lane_text(app: &App, l: Lane) -> String {
    app.i18n.t(match l {
        Lane::Now => "queue-now",
        Lane::Queued => "queue-queued",
        Lane::Waiting => "queue-waiting",
    })
}

fn team_text(app: &App, team: &str) -> String {
    app.snapshot
        .team_label(team)
        .map(str::to_string)
        .unwrap_or_else(|| team.to_string())
}

/// The two lines of an item.
fn item_rows(app: &App, it: &QueueItem, idx: usize, glyph: (&str, Style), rows: &mut Vec<Row>) {
    let mut first = vec![
        (format!("  {} ", glyph.0), glyph.1),
        (it.title.clone(), Style::new().fg(FG)),
    ];
    if it.holds > 0 {
        first.push((
            format!(
                " · {}",
                app.i18n
                    .t_args("queue-holds", &[("n", &it.holds.to_string())])
            ),
            Style::new().fg(AMBER),
        ));
    }
    rows.push(Row {
        parts: first,
        item: Some(idx),
    });
    let owner = if it.owner_label.is_empty() {
        "—".to_string()
    } else {
        it.owner_label.clone()
    };
    let mut second = vec![(
        format!(
            "    {} · {} · {} · {}",
            reason_text(app, &it.reason),
            age(app, it.since),
            source_text(app, it),
            owner
        ),
        Style::new().fg(DIM),
    )];
    // The root, when the chain goes further than the direct reason.
    if let Some(root) = it.root_link().filter(|_| it.chain.len() > 1) {
        let (mark, color) = if root.reason.on_you() {
            ("▲ ", AMBER)
        } else {
            ("", DIM)
        };
        second.push((
            format!(
                " · {mark}{}",
                app.i18n.t_args("queue-root", &[("what", &root.label)])
            ),
            Style::new().fg(color),
        ));
    }
    rows.push(Row {
        parts: second,
        item: Some(idx),
    });
}

fn rows(app: &App) -> Vec<Row> {
    let i = &app.i18n;
    let q = &app.queue;
    let mut out = Vec::new();
    let mut idx = 0;
    // What waits on the user comes first: it is what the user can act on.
    if !q.you.is_empty() {
        out.push(row(vec![(i.t("queue-you"), bold().fg(AMBER))]));
        for it in &q.you {
            item_rows(app, it, idx, ("▲", Style::new().fg(AMBER)), &mut out);
            idx += 1;
        }
    }
    for g in &q.owners {
        let mut head = vec![(g.label.clone(), bold().fg(ACCENT))];
        if !g.team.is_empty() {
            head.push((
                format!(" ({})", team_text(app, &g.team)),
                Style::new().fg(PURPLE),
            ));
        }
        head.push((
            format!(
                " — {}",
                i.t_args(
                    "queue-group",
                    &[
                        ("n", &g.pending().to_string()),
                        ("wait", &age(app, g.longest_since)),
                    ],
                )
            ),
            Style::new().fg(DIM),
        ));
        if g.bottleneck {
            head.push((format!(" [{}]", i.t("queue-bottleneck")), bold().fg(AMBER)));
        }
        out.push(row(head));
        let mut lane = None;
        for it in &g.items {
            if lane != Some(it.lane) {
                lane = Some(it.lane);
                out.push(row(vec![(
                    format!(" {}", lane_text(app, it.lane)),
                    Style::new().fg(DIM).add_modifier(Modifier::BOLD),
                )]));
            }
            let glyph = match it.lane {
                Lane::Now => ("●", Style::new().fg(GREEN)),
                Lane::Queued => ("○", Style::new().fg(ACCENT)),
                Lane::Waiting => ("◐", Style::new().fg(AMBER)),
            };
            item_rows(app, it, idx, glyph, &mut out);
            idx += 1;
        }
    }
    if !q.pool.is_empty() {
        out.push(row(vec![(i.t("queue-pool"), bold().fg(DIM))]));
        for p in &q.pool {
            let label = if !p.tasks {
                p.label.clone()
            } else if p.label.trim().is_empty() {
                i.t("queue-pool-tasks-plain")
            } else {
                i.t_args("queue-pool-tasks", &[("group", p.label.trim())])
            };
            out.push(row(vec![(format!(" {label}"), bold().fg(FG))]));
            for it in &p.items {
                item_rows(app, it, idx, ("·", Style::new().fg(DIM)), &mut out);
                idx += 1;
            }
            if p.omitted > 0 {
                out.push(row(vec![(
                    format!(
                        "    {}",
                        i.t_args(
                            "queue-more",
                            &[("n", &p.omitted.to_string()), ("file", &p.file)],
                        )
                    ),
                    Style::new().fg(DIM),
                )]));
            }
        }
    }
    out
}

pub fn draw(frame: &mut Frame, area: Rect, app: &App) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let (list, detail) = if area.width >= 100 {
        let [l, d] = Layout::horizontal([Constraint::Min(50), Constraint::Length(44)]).areas(area);
        (l, Some(d))
    } else if area.height >= 18 {
        let [l, d] = Layout::vertical([Constraint::Min(6), Constraint::Length(9)]).areas(area);
        (l, Some(d))
    } else {
        (area, None)
    };
    let sel = app.queue_index();
    draw_list(frame, list, app, sel);
    if let Some(d) = detail {
        let items = app.queue.items();
        draw_detail(frame, d, app, sel.and_then(|k| items.get(k).copied()));
    }
}

fn draw_list(frame: &mut Frame, area: Rect, app: &App, sel: Option<usize>) {
    let i = &app.i18n;
    let title = format!(
        "{} · {}",
        i.t("pane-queue"),
        i.t_args(
            "queue-totals",
            &[
                ("n", &app.queue.items().len().to_string()),
                ("you", &app.queue.you.len().to_string()),
            ],
        )
    );
    let block = pane(title, true);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let buf = frame.buffer_mut();
    if app.queue.is_empty() {
        put(
            buf,
            inner,
            inner.x,
            inner.y,
            i.t("queue-empty"),
            Style::new().fg(DIM),
        );
        if !app.todos_enabled {
            put(
                buf,
                inner,
                inner.x,
                inner.y.saturating_add(1),
                i.t("queue-todos-off"),
                Style::new().fg(DIM),
            );
        }
        return;
    }
    let rows = rows(app);
    let h = inner.height as usize;
    // Keep the selected item (both of its lines) on screen.
    let first = sel
        .and_then(|s| rows.iter().position(|r| r.item == Some(s)))
        .unwrap_or(0);
    let offset = (first + 2).saturating_sub(h);
    for (k, r) in rows.iter().skip(offset).take(h).enumerate() {
        let y = inner.y.saturating_add(k as u16);
        let selected = r.item.is_some() && r.item == sel;
        if selected {
            put(
                buf,
                inner,
                inner.x,
                y,
                " ".repeat(inner.width as usize),
                Style::new().bg(SELECTED_BG),
            );
        }
        let mut x = inner.x;
        for (n, (text, style)) in r.parts.iter().enumerate() {
            let mut text = text.clone();
            if n == 0 && selected && r.parts.len() > 1 && text.starts_with("  ") {
                text.replace_range(0..1, "►");
            }
            let style = if selected {
                style.bg(SELECTED_BG)
            } else {
                *style
            };
            let room = (inner.x + inner.width).saturating_sub(x) as usize;
            if room == 0 {
                break;
            }
            putn(buf, inner, x, y, &text, room, style);
            x = x.saturating_add(Span::raw(text.as_str()).width() as u16);
        }
    }
}

fn draw_detail(frame: &mut Frame, area: Rect, app: &App, it: Option<&QueueItem>) {
    let i = &app.i18n;
    let block = pane(i.t("pane-queue-detail"), false);
    let Some(it) = it else {
        frame.render_widget(block, area);
        return;
    };
    let label = |k: &str| Span::styled(format!("{}: ", i.t(k)), Style::new().fg(DIM));
    // Each entry is wrapped with its continuation lines indented by two.
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(it.title.clone(), bold().fg(FG)))];
    let owner = if it.owner_label.is_empty() {
        "—".to_string()
    } else if it.team.is_empty() {
        it.owner_label.clone()
    } else {
        format!("{} ({})", it.owner_label, team_text(app, &it.team))
    };
    lines.push(Line::from(vec![
        label("queue-detail-owner"),
        Span::raw(owner),
    ]));
    lines.push(Line::from(vec![
        label("queue-detail-state"),
        Span::raw(format!(
            "{} · {}",
            lane_text(app, it.lane),
            reason_text(app, &it.reason)
        )),
    ]));
    let age_text = if matches!(it.origin, Origin::Todo(_)) {
        i.t_args("queue-age-file", &[("ago", &age(app, it.since))])
    } else if it.since > 0 {
        format!("{} ({})", age(app, it.since), clock(it.since))
    } else {
        "—".into()
    };
    lines.push(Line::from(vec![
        label("queue-detail-age"),
        Span::raw(age_text),
    ]));
    if !it.chain.is_empty() {
        let mut spans = vec![label("queue-detail-chain")];
        let n = it.chain.len();
        spans.push(Span::raw(trunc(&it.title, 30)));
        for (k, l) in it.chain.iter().enumerate() {
            spans.push(Span::styled(" → ", Style::new().fg(DIM)));
            let root = k + 1 == n;
            let text = if root && l.since > 0 {
                format!("{} ({})", trunc(&l.label, 40), age(app, l.since))
            } else {
                trunc(&l.label, 40)
            };
            if root && l.reason.on_you() {
                spans.push(Span::styled(format!("▲ {text}"), bold().fg(AMBER)));
            } else if root {
                spans.push(Span::styled(text, bold()));
            } else {
                spans.push(Span::raw(text));
            }
        }
        lines.push(Line::from(spans));
    }
    if !it.blockers.is_empty() {
        lines.push(Line::from(label("queue-detail-blockers")));
        for (id, title, status) in &it.blockers {
            let status = if status.is_empty() {
                String::new()
            } else {
                format!(" ({})", i.t(status))
            };
            lines.push(Line::raw(format!("  #{id} {title}{status}")));
        }
    }
    if !it.detail.is_empty() {
        lines.push(Line::from(label("queue-detail-description")));
        for l in it.detail.lines() {
            lines.push(Line::raw(format!("  {l}")));
        }
    }
    if !it.context.is_empty() {
        lines.push(Line::from(vec![
            label("queue-detail-context"),
            Span::raw(it.context.clone()),
        ]));
    }
    if !it.location.is_empty() {
        lines.push(Line::from(vec![
            label("queue-detail-source"),
            Span::styled(it.location.clone(), Style::new().fg(CYAN)),
        ]));
    }
    let width = block.inner(area).width as usize;
    let first = lines.remove(0);
    let mut wrapped = wrap_line(first, width, 0);
    for l in lines {
        wrapped.extend(wrap_line(l, width, 2));
    }
    frame.render_widget(Paragraph::new(wrapped).block(block), area);
}

/// Splits a line into lines of at most `width` columns at spaces (inside
/// words only when a word is longer than a line); continuation lines start
/// with `indent` spaces so they stay under their field.
pub fn wrap_line(line: Line<'_>, width: usize, indent: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let indent = indent.min(width.saturating_sub(1));
    // Words and the spaces between them, each with its style.
    let mut tokens: Vec<(String, Style)> = Vec::new();
    for span in &line.spans {
        let mut cur = String::new();
        for c in span.content.chars() {
            if (c == ' ') != cur.ends_with(' ') && !cur.is_empty() {
                tokens.push((std::mem::take(&mut cur), span.style));
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            tokens.push((cur, span.style));
        }
    }
    let w = |t: &str| Span::raw(t).width();
    let mut out: Vec<Line<'static>> = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    let mut used = 0;
    let mut start = 0;
    let mut finish = |cur: &mut Vec<Span<'static>>, used: &mut usize| {
        // No trailing space at a break.
        if cur.len() > 1 && cur.last().is_some_and(|s| s.content.trim().is_empty()) {
            cur.pop();
        }
        out.push(Line::from(std::mem::take(cur)));
        cur.push(Span::raw(" ".repeat(indent)));
        *used = indent;
    };
    for (text, style) in tokens {
        let tw = w(&text);
        if text.starts_with(' ') {
            // Spaces at a break vanish; leading spaces of a line stay.
            if used + tw <= width {
                cur.push(Span::styled(text, style));
                used += tw;
            } else if used > start {
                finish(&mut cur, &mut used);
                start = indent;
            }
            continue;
        }
        if used + tw > width && used > start {
            finish(&mut cur, &mut used);
            start = indent;
        }
        // A word longer than a whole line is cut into pieces.
        let mut piece = String::new();
        for c in text.chars() {
            let cw = w(c.encode_utf8(&mut [0; 4]));
            if used + w(&piece) + cw > width && (used > start || !piece.is_empty()) {
                if !piece.is_empty() {
                    used += w(&piece);
                    cur.push(Span::styled(std::mem::take(&mut piece), style));
                }
                finish(&mut cur, &mut used);
                start = indent;
            }
            piece.push(c);
        }
        used += w(&piece);
        cur.push(Span::styled(piece, style));
    }
    out.push(Line::from(cur));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(lines: &[Line]) -> Vec<String> {
        lines.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn wrapped_lines_keep_the_field_indent() {
        let l = Line::from(vec![
            Span::raw("Description: "),
            Span::raw("one two three four five six"),
        ]);
        assert_eq!(
            texts(&wrap_line(l, 16, 2)),
            ["Description: one", "  two three four", "  five six"]
        );
        let body = Line::raw("  alpha beta gamma delta");
        assert_eq!(
            texts(&wrap_line(body, 12, 2)),
            ["  alpha beta", "  gamma", "  delta"]
        );
        // Long words are cut; nothing is ever wider than the width.
        let long = Line::raw("x".repeat(25));
        let out = wrap_line(long, 10, 2);
        assert!(out.iter().all(|l| l.width() <= 10), "{:?}", texts(&out));
        assert_eq!(
            out.iter()
                .map(|l| l.to_string().trim().len())
                .sum::<usize>(),
            25
        );
        assert_eq!(
            texts(&wrap_line(Line::raw("short"), 0, 2)),
            ["s", "h", "o", "r", "t"]
        );
    }
}
