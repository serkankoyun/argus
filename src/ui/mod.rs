//! Rendering. Every view shares the header, the view bar and the key hints.

mod agent;
mod channel;
mod common;
mod fleet;
mod graph;
mod history;
mod lanes;
mod layout;
mod overview;
mod pair;
mod queue;
mod room;
mod sequence;
mod taskmap;
mod teams;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, View};
use crate::model::AgentState;
use common::*;

pub fn draw(frame: &mut Frame, app: &App) {
    let [header, tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, header, app);
    draw_tabs(frame, tabs, app);
    match app.view {
        View::Overview => overview::draw(frame, body, app),
        View::AgentDetail => agent::draw(frame, body, app),
        View::Graph => graph::draw(frame, body, app),
        View::Room => room::draw(frame, body, app),
        View::Lanes => lanes::draw(frame, body, app),
        View::Sequence => sequence::draw(frame, body, app),
        View::Fleet => fleet::draw(frame, body, app),
        View::Channel => channel::draw(frame, body, app),
        View::TaskMap => taskmap::draw(frame, body, app),
        View::Pair => pair::draw(frame, body, app),
        View::Teams => teams::draw(frame, body, app),
        View::History => history::draw(frame, body, app),
        View::Queue => queue::draw(frame, body, app),
    }
    draw_footer(frame, footer, app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let s = &app.snapshot;
    let count = |st: AgentState| s.agents.iter().filter(|a| a.state == st).count();
    let status = if app.paused {
        Span::styled(
            format!("● {}", app.i18n.t("status-paused")),
            Style::new().fg(AMBER),
        )
    } else if app.demo {
        Span::styled(
            format!("● {}", app.i18n.t("status-demo")),
            Style::new().fg(PURPLE),
        )
    } else {
        Span::styled(
            format!("● {}", app.i18n.t("status-live")),
            Style::new().fg(GREEN),
        )
    };
    let summary = app.i18n.t_args(
        "header-summary",
        &[
            ("hosts", s.hosts.len().to_string().as_str()),
            ("agents", s.agents.len().to_string().as_str()),
            ("active", count(AgentState::Working).to_string().as_str()),
        ],
    );
    let mut spans = vec![
        Span::styled("argus ", Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(env!("CARGO_PKG_VERSION"), Style::new().fg(DIM)),
        Span::styled("  │  ", Style::new().fg(DIM)),
        Span::raw(summary),
    ];
    if let Some(h) = &app.host {
        spans.push(Span::styled(
            format!(" · {} {h}", app.i18n.t("host-label")),
            Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
        ));
    }
    if let Some(t) = &app.team {
        spans.push(Span::styled(
            format!(" · {} {t}", app.i18n.t("team-label")),
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    }
    let waiting = count(AgentState::Waiting);
    if waiting > 0 {
        spans.push(Span::styled(
            format!(
                " · {}",
                app.i18n
                    .t_args("header-waiting", &[("n", waiting.to_string().as_str())])
            ),
            Style::new().fg(AMBER),
        ));
    }
    let stalled = count(AgentState::Stalled);
    if stalled > 0 {
        spans.push(Span::styled(
            format!(
                " · {}",
                app.i18n
                    .t_args("header-stalled", &[("n", stalled.to_string().as_str())])
            ),
            Style::new().fg(RED),
        ));
    }
    spans.extend([
        Span::styled("  │  ", Style::new().fg(DIM)),
        Span::raw(format!("${:.2}  ", s.total_cost())),
        status,
        Span::styled(
            format!("  {}  {}", clock(app.now), app.i18n.lang.to_uppercase()),
            Style::new().fg(DIM),
        ),
    ]);
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Columns the view bar needs for these titles (" title " plus dividers).
fn tabs_width(titles: &[String]) -> usize {
    titles
        .iter()
        .map(|t| Line::raw(t.as_str()).width() + 3)
        .sum::<usize>()
        - 1
}

/// Tab titles as drawn in a view bar `width` columns wide. When the full
/// names do not fit, the other names are shortened (the selected view and
/// the Backlog keep theirs), down to the bare keys.
fn tab_titles(app: &App, width: u16) -> Vec<String> {
    let keys: Vec<String> = (0..View::ALL.len())
        .map(|i| match i {
            9 => "0".to_string(),
            10 => "t".to_string(),
            11 => "H".to_string(),
            12 => "b".to_string(),
            _ => (i + 1).to_string(),
        })
        .collect();
    let names: Vec<String> = View::ALL.iter().map(|v| app.i18n.t(v.key())).collect();
    let keep = |i: usize| i == app.view.index() || View::ALL[i] == View::Queue;
    let titles = |cap: Option<usize>| -> Vec<String> {
        keys.iter()
            .zip(&names)
            .enumerate()
            .map(|(i, (k, n))| match cap {
                Some(0) if !keep(i) => k.clone(),
                Some(c) if !keep(i) => format!("{k} {}", trunc(n, c)),
                _ => format!("{k} {n}"),
            })
            .collect()
    };
    let full = titles(None);
    let width = width as usize;
    if tabs_width(&full) <= width {
        return full;
    }
    let longest = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    (3..longest)
        .rev()
        .map(|c| titles(Some(c)))
        .find(|t| tabs_width(t) <= width)
        .unwrap_or_else(|| titles(Some(0)))
}

/// Which tab is at terminal column `col` of row `row` (the view bar is row 1).
pub fn tab_at(app: &App, width: u16, row: u16, col: u16) -> Option<usize> {
    if row != 1 {
        return None;
    }
    let mut x = 0u16;
    for (i, t) in tab_titles(app, width).iter().enumerate() {
        // Each tab is " title " followed by a one-column divider.
        let w = Line::raw(t.as_str()).width() as u16 + 2;
        if col >= x && col < x + w {
            return Some(i);
        }
        x += w + 1;
    }
    None
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let titles = tab_titles(app, area.width).into_iter().map(Line::from);
    let tabs = Tabs::new(titles)
        .select(app.view.index())
        .style(Style::new().fg(DIM))
        .highlight_style(Style::new().fg(ACCENT).add_modifier(Modifier::BOLD))
        .divider(Span::styled("│", Style::new().fg(BORDER)));
    frame.render_widget(tabs, area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(buf) = &app.input {
        let cursor = if (app.frame / 8) % 2 == 0 { "▌" } else { " " };
        let hint = if buf.starts_with('>') {
            app.i18n.t("input-prompt-hint")
        } else {
            app.i18n.t("input-cmd-hint")
        };
        let line = Line::from(vec![
            Span::styled(
                buf.clone(),
                Style::new().fg(FG).add_modifier(Modifier::BOLD),
            ),
            Span::styled(cursor, Style::new().fg(GREEN)),
            Span::styled(format!("   {hint}"), Style::new().fg(DIM)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }
    let key =
        |k: &'static str| Span::styled(format!(" {k} "), Style::new().bg(BORDER).fg(Color::White));
    let label = |id: &str| Span::styled(format!(" {}  ", app.i18n.t(id)), Style::new().fg(DIM));
    let mut spans = if app.view == View::Queue {
        vec![
            key("↑↓"),
            label("key-queue-move"),
            key("enter"),
            label("key-open-owner"),
            key("1-0"),
            label("key-views"),
            key("b"),
            label("key-queue"),
        ]
    } else if app.view == View::Teams {
        vec![
            key("↑↓"),
            label("key-team-move"),
            key("enter"),
            label("teams-open"),
            key("1-0"),
            label("key-views"),
        ]
    } else {
        let mut v = vec![
            key("1-0"),
            label("key-views"),
            key("b"),
            label("key-queue"),
            key("↑↓"),
            label("key-select"),
            key("enter"),
            label("key-detail"),
            key("s"),
            label("key-host"),
            key("f"),
            label(if app.show_finished {
                "key-hide-done"
            } else {
                "key-show-done"
            }),
        ];
        if (app.team.is_some() || app.host.is_some()) && app.view != View::AgentDetail {
            v.extend([key("esc"), label("key-leave-team")]);
        }
        v
    };
    if app.pending_approval().is_some() {
        spans.extend([
            key("a"),
            Span::styled(
                format!(" {}  ", app.i18n.t("key-approve")),
                Style::new().fg(AMBER),
            ),
            key("r"),
            label("key-reject"),
        ]);
    }
    if let Some((id, arg)) = app.snapshot.warnings.first() {
        spans.push(Span::styled(
            format!(" ! {}  ", app.i18n.t_args(id, &[("arg", arg.as_str())])),
            Style::new().fg(AMBER),
        ));
    }
    spans.extend([
        key(":"),
        label("key-cmd"),
        key(">"),
        label("key-prompt"),
        key("space"),
        label("key-pause"),
        key("q"),
        label("key-quit"),
    ]);
    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(48)]).areas(area);
    frame.render_widget(Paragraph::new(Line::from(spans)), left);
    if app.debug_keys && !app.last_key.is_empty() {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!("key: {}", app.last_key),
                Style::new().fg(CYAN),
            ))
            .right_aligned(),
            right,
        );
        return;
    }
    if let Some((at, n)) = &app.flash {
        if app.now - at < 5_000 {
            let (text, color) = match n {
                Ok(t) => (format!("✓ {t}"), GREEN),
                Err(e) => (format!("✗ {e}"), RED),
            };
            frame.render_widget(
                Paragraph::new(Line::styled(text, Style::new().fg(color))).right_aligned(),
                right,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use super::*;
    use crate::i18n::I18n;
    use crate::model::YOU;
    use crate::source::demo::DemoSource;
    use crate::source::{Channels, Source};

    /// A snapshot far bigger than any screen: the demo team copied many times.
    fn crowded() -> crate::model::Snapshot {
        let mut s = DemoSource::new("en").poll();
        let base_agents = s.agents.clone();
        let base_msgs = s.messages.clone();
        let base_tools = s.tools.clone();
        for k in 1..8 {
            let tag = |id: &str| {
                if id == YOU {
                    id.to_string()
                } else {
                    format!("{id}-{k}")
                }
            };
            for a in &base_agents {
                let mut a = a.clone();
                a.id = tag(&a.id);
                a.name = format!("{}-{k}", a.name);
                a.parent = a.parent.as_deref().map(tag);
                a.host = format!("host-{}", k % 3);
                a.team = Some(format!("team-{}", k % 4));
                s.agents.push(a);
            }
            for m in &base_msgs {
                let mut m = m.clone();
                m.from = tag(&m.from);
                m.to = tag(&m.to);
                s.messages.push(m);
            }
            for t in &base_tools {
                let mut t = t.clone();
                t.agent = tag(&t.agent);
                s.tools.push(t);
            }
        }
        // Hundreds of backlog items: todo files and queued tasks.
        let base_todos = s.todos.clone();
        for k in 0..60 {
            for f in &base_todos {
                let mut f = f.clone();
                f.path = format!("{}-{k}", f.path);
                f.project = format!("{}-{k}", f.project);
                s.todos.push(f);
            }
        }
        for k in 0..50 {
            s.tasks.push(crate::model::Task {
                id: format!("{}", 100 + k),
                title: format!("queued task {k} with a fairly long title to overflow narrow panes"),
                owner: Some(format!("api-dev-{}", 1 + k % 7)),
                since: s.taken_at - 1000 * k as i64,
                source: format!("~/.claude/tasks/demo/{}.json", 100 + k),
                ..Default::default()
            });
        }
        s
    }

    /// Rendered text of one frame.
    fn screen(app: &App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    fn press(app: &mut App, code: ratatui::crossterm::event::KeyCode) {
        use ratatui::crossterm::event::{KeyEvent, KeyModifiers};
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    #[test]
    fn b_opens_backlog_and_tab_reaches_it() {
        use ratatui::crossterm::event::KeyCode;
        let mut app = app_with(DemoSource::new("en").poll());
        press(&mut app, KeyCode::Char('b'));
        assert_eq!(app.view, View::Queue);
        app.view = View::History;
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.view, View::Queue);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.view, View::Overview);
        press(&mut app, KeyCode::BackTab);
        assert_eq!(app.view, View::Queue);
        // Digits still map to the first ten views.
        press(&mut app, KeyCode::Char('0'));
        assert_eq!(app.view, View::Pair);
        app.input = Some(":view backlog".into());
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, View::Queue);
    }

    #[test]
    fn tabs_fit_and_keep_backlog_at_120_columns() {
        use ratatui::crossterm::event::KeyCode;
        for lang in ["en", "tr"] {
            let mut app = app_with(DemoSource::new(lang).poll());
            app.view = View::Overview;
            let name = app.i18n.t("view-queue");
            for w in [120u16, 140, 200] {
                let s = screen(&app, w, 4);
                let bar = s.lines().nth(1).unwrap();
                assert!(bar.contains(&format!("b {name}")), "{lang} {w}: {bar}");
                let selected = app.i18n.t("view-overview");
                assert!(bar.contains(&format!("1 {selected}")), "{lang} {w}: {bar}");
                // Every tab is still there, and clicks land on the drawn tabs.
                let titles = tab_titles(&app, w);
                assert_eq!(titles.len(), View::ALL.len());
                assert!(tabs_width(&titles) <= w as usize, "{lang} {w}");
                let col = bar.find(&format!("b {name}")).unwrap();
                let col = bar[..col].chars().count() as u16;
                assert_eq!(tab_at(&app, w, 1, col), Some(12), "{lang} {w}");
            }
            // Full names when they fit.
            assert!(screen(&app, 220, 4).contains(&app.i18n.t("view-taskmap")));
            press(&mut app, KeyCode::Char('9'));
            let bar = screen(&app, 120, 4);
            assert!(bar.contains(&app.i18n.t("view-taskmap")), "{lang}: {bar}");
        }
    }

    #[test]
    fn footer_and_tabs_list_backlog() {
        let mut app = app_with(DemoSource::new("en").poll());
        app.view = View::Overview;
        let s = screen(&app, 220, 5);
        assert!(s.contains("b Backlog"), "{s}");
        assert!(s.contains(" b  backlog"), "{s}");
        app.i18n = I18n::new(Some("tr"));
        let s = screen(&app, 220, 5);
        assert!(s.contains("b Kuyruk"), "{s}");
        assert!(s.contains(" b  kuyruk"), "{s}");
        app.view = View::Queue;
        let s = screen(&app, 220, 5);
        assert!(s.contains("madde seç"), "{s}");
    }

    #[test]
    fn backlog_keys_move_and_open_the_owner() {
        use ratatui::crossterm::event::KeyCode;
        let mut app = app_with(DemoSource::new("en").poll());
        app.view = View::Queue;
        let keys: Vec<String> = app.queue.items().iter().map(|i| i.key.clone()).collect();
        assert_eq!(app.queue_index(), Some(0));
        press(&mut app, KeyCode::Down);
        assert_eq!(app.queue_sel.as_deref(), Some(keys[1].as_str()));
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Up);
        assert_eq!(app.queue_index(), Some(keys.len() - 1), "wraps");
        press(&mut app, KeyCode::Home);
        assert_eq!(app.queue_index(), Some(0));
        press(&mut app, KeyCode::PageDown);
        assert_eq!(app.queue_index(), Some(10.min(keys.len() - 1)));
        press(&mut app, KeyCode::End);
        assert_eq!(app.queue_index(), Some(keys.len() - 1));
        // The last items are unowned todo items: Enter says so and stays.
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, View::Queue);
        assert!(app.flash.is_some());
        press(&mut app, KeyCode::Home);
        let owner = app.queue.items()[0].owner_id.clone().unwrap();
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, View::AgentDetail);
        assert_eq!(app.selected.as_deref(), Some(owner.as_str()));
    }

    #[test]
    fn backlog_detail_shows_description_and_source() {
        let mut app = app_with(DemoSource::new("en").poll());
        app.view = View::Queue;
        let it = app
            .queue
            .you
            .iter()
            .find(|i| !i.detail.is_empty() && i.location.contains("todo.md"))
            .expect("a todo item with a description")
            .clone();
        app.queue_sel = Some(it.key.clone());
        let s = screen(&app, 140, 40);
        assert!(s.contains("Description"), "{s}");
        let start: String = it.detail.chars().take(20).collect();
        assert!(s.contains(&start), "{s}");
        assert!(
            s.contains(&format!(
                "tasks/todo.md:{}",
                it.location.rsplit(':').next().unwrap()
            )),
            "{s}"
        );
        assert!(s.contains("► ▲"), "{s}");
    }

    #[test]
    fn backlog_detail_shows_the_waiting_chain() {
        let mut app = app_with(DemoSource::new("en").poll());
        app.view = View::Queue;
        let it = app
            .queue
            .items()
            .into_iter()
            .find(|i| i.chain.len() > 1)
            .expect("an item with a chain")
            .clone();
        app.queue_sel = Some(it.key.clone());
        let s = screen(&app, 160, 50);
        assert!(s.contains("Why it waits"), "{s}");
        assert!(s.contains(" → "), "{s}");
        assert!(s.contains("root: "), "{s}");
    }

    #[test]
    fn backlog_empty_state_mentions_todos_off() {
        let mut app = app_with(crate::model::Snapshot::default());
        app.view = View::Queue;
        app.todos_enabled = false;
        let s = screen(&app, 120, 20);
        assert!(s.contains("Nothing queued"), "{s}");
        assert!(s.contains("--no-todos"), "{s}");
    }

    fn app_with(snapshot: crate::model::Snapshot) -> App {
        let (snap_tx, snap_rx) = mpsc::channel();
        let (ctl_tx, _ctl_rx) = mpsc::channel();
        let (_note_tx, note_rx) = mpsc::channel();
        snap_tx.send(snapshot).unwrap();
        let mut app = App::new(
            I18n::new(Some("en")),
            Channels {
                snapshots: snap_rx,
                controls: ctl_tx,
                notices: note_rx,
            },
            true,
        );
        app.on_tick();
        app
    }

    #[test]
    fn every_view_renders_crowded_and_tiny_without_panicking() {
        let mut app = app_with(crowded());
        assert!(app.snapshot.agents.len() > 60);
        assert!(
            app.queue.items().len() >= 300,
            "{}",
            app.queue.items().len()
        );
        for (w, h) in [
            (187, 54),
            (120, 36),
            (99, 30),
            (80, 24),
            (40, 12),
            (40, 10),
            (10, 5),
            (1, 1),
        ] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            for v in View::ALL {
                app.view = v;
                for grouped in [false, true] {
                    app.group_by_team = grouped;
                    terminal
                        .draw(|f| draw(f, &app))
                        .unwrap_or_else(|e| panic!("{v:?} at {w}x{h}: {e}"));
                }
            }
            // The Backlog with the selection on its last item.
            app.view = View::Queue;
            app.queue_sel = app.queue.items().last().map(|i| i.key.clone());
            terminal.draw(|f| draw(f, &app)).unwrap();
            app.queue_sel = None;
        }
    }

    /// Maintainer, Nov 14: after picking a team, views 5, 6, 7 stayed on that
    /// team and one Esc did not bring all teams back.
    #[test]
    fn one_esc_leaves_the_team_from_any_view() {
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let mut app = app_with(crowded());
        app.team = Some("auth-refactor".into());
        app.view = View::Graph;
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.team, None);
        assert_eq!(app.view, View::Teams);
        // Leaving an agent's detail stays a plain "back".
        app.team = Some("auth-refactor".into());
        app.view = View::AgentDetail;
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(app.team.is_some());
    }

    #[test]
    fn overview_and_agent_view_show_short_models() {
        let mut app = app_with(DemoSource::new("en").poll());
        app.view = View::Overview;
        let s = screen(&app, 187, 40);
        assert!(s.contains("MODEL"), "{s}");
        assert!(s.contains("opus-5.5") && s.contains("sonnet-5.5"), "{s}");
        assert!(!s.contains("claude-opus"), "{s}");
        app.selected = app.snapshot.agents.first().map(|a| a.id.clone());
        app.view = View::AgentDetail;
        let s = screen(&app, 187, 40);
        assert!(!s.contains("claude-"), "{s}");
    }

    #[test]
    fn every_view_renders_empty() {
        let mut app = app_with(crate::model::Snapshot::default());
        let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
        for v in View::ALL {
            app.view = v;
            terminal.draw(|f| draw(f, &app)).unwrap();
        }
    }
}
