//! Independent tests for the Backlog view (`b`), its waiting chains and the
//! `tasks/todo.md` reader, written against the acceptance criteria (AC1-AC9)
//! and Addendum A, not against the implementation. All data is synthetic.

use std::path::PathBuf;
use std::sync::mpsc;

use clap::Parser;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;

use crate::app::{App, View};
use crate::i18n::I18n;
use crate::model::{
    path_within, Agent, AgentState, Message, Millis, MsgKind, Run, RunStage, Snapshot, StageStatus,
    Task, TaskStatus, TodoFile, TodoItem, STAGES, YOU,
};
use crate::queue::{self, Lane, Reason, MAX_CHAIN, MAX_QUEUE_ITEMS};
use crate::source::todos::{
    self, find_todo_files, fold, parse, parse_bytes, split_wait, waits_on_you_heading, TodoReader,
    MAX_FILE_BYTES, MAX_ITEMS_PER_FILE, MAX_TODO_FILES, MAX_TODO_ITEMS,
};
use crate::source::{Channels, Source};

const NOW: Millis = 1_800_000_000_000;

// ---------------------------------------------------------------- helpers

fn agent(id: &str, team: &str, state: AgentState, cwd: &str) -> Agent {
    Agent {
        id: id.into(),
        name: id.into(),
        team: Some(team.into()),
        state,
        cwd: cwd.into(),
        task: format!("synthetic work of {id}"),
        last_activity: NOW - 1_000,
        ..Agent::default()
    }
}

fn task(group: &str, id: &str, owner: Option<&str>, status: TaskStatus, deps: &[&str]) -> Task {
    Task {
        id: id.into(),
        title: format!("synthetic task {id} of {group}"),
        owner: owner.map(Into::into),
        status,
        blocked_by: deps.iter().map(|d| d.to_string()).collect(),
        source: format!("~/.claude/tasks/{group}/{id}.json"),
        since: NOW - 60_000,
        ..Task::default()
    }
}

fn tkey(group: &str, id: &str) -> String {
    format!("task:~/.claude/tasks/{group}/{id}.json#{id}")
}

fn perm(from: &str, ts: Millis, req: &str) -> Message {
    Message {
        ts,
        from: from.into(),
        to: YOU.into(),
        kind: MsgKind::Perm,
        label: "Bash: synthetic deploy".into(),
        text: "May I run the synthetic deploy?".into(),
        answered_at: None,
        request: Some(req.into()),
    }
}

fn run(id: &str, team: &str, stages: &[(&str, StageStatus, &str, &str)]) -> Run {
    Run {
        id: id.into(),
        goal: format!("{team}: synthetic goal"),
        team: Some(team.into()),
        created: NOW - 3_600_000,
        stages: STAGES
            .iter()
            .map(|k| {
                let (status, agent, owner) = stages
                    .iter()
                    .find(|s| s.0 == *k)
                    .map(|s| (s.1, s.2, s.3))
                    .unwrap_or((StageStatus::Skipped, "", ""));
                RunStage {
                    key: k.to_string(),
                    status,
                    agent: agent.into(),
                    owners: if owner.is_empty() {
                        vec![]
                    } else {
                        vec![owner.into()]
                    },
                    since: NOW - 600_000,
                    ..RunStage::default()
                }
            })
            .collect(),
        ..Run::default()
    }
}

fn todo_file(root: &str, items: Vec<TodoItem>, omitted: u32) -> TodoFile {
    TodoFile {
        path: format!("{root}/tasks/todo.md"),
        root: root.into(),
        project: root.rsplit('/').next().unwrap_or("").into(),
        host: String::new(),
        modified: NOW - 7_200_000,
        items,
        omitted,
    }
}

fn todo(line: u32, title: &str) -> TodoItem {
    TodoItem {
        line,
        title: title.into(),
        section: "Backlog".into(),
        ..TodoItem::default()
    }
}

fn app_with(snapshot: Snapshot, lang: &str) -> App {
    let (snap_tx, snap_rx) = mpsc::channel();
    let (ctl_tx, _ctl_rx) = mpsc::channel();
    let (_note_tx, note_rx) = mpsc::channel();
    snap_tx.send(snapshot).unwrap();
    let mut app = App::new(
        I18n::new(Some(lang)),
        Channels {
            snapshots: snap_rx,
            controls: ctl_tx,
            notices: note_rx,
        },
        true,
    );
    app.on_tick();
    app.now = NOW;
    app
}

fn screen(app: &App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| crate::ui::draw(f, app)).unwrap();
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

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn typed(app: &mut App, cmd: &str) {
    app.input = Some(cmd.into());
    press(app, KeyCode::Enter);
}

/// Ages as the Backlog writes them (`45s`, `5m`, `2h00m`).
fn dur(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}h{m:02}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{s}s")
    }
}

fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "argus-qv-{tag}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A snapshot with one item of every kind.
fn every_kind() -> Snapshot {
    use StageStatus::*;
    Snapshot {
        taken_at: NOW,
        agents: vec![
            agent("dev-a", "shop", AgentState::Working, "/w/shop"),
            agent("rev-b", "shop", AgentState::Waiting, "/w/shop"),
            agent("db-c", "shop", AgentState::Waiting, "/w/shop/db"),
            agent("ops-d", "shop", AgentState::Working, "/w/shop"),
        ],
        tasks: vec![
            task("grp", "1", Some("dev-a"), TaskStatus::InProgress, &[]),
            task("grp", "2", Some("dev-a"), TaskStatus::Pending, &[]),
            task("grp", "3", Some("dev-a"), TaskStatus::Pending, &["1"]),
            task("grp", "4", None, TaskStatus::Pending, &[]),
        ],
        runs: vec![run(
            "R-20261116-0900-shop-sample",
            "shop",
            &[
                ("dev", Running, "orchestra-developer", "ops-d"),
                ("test", Pending, "orchestra-test-writer", ""),
            ],
        )],
        messages: vec![
            perm("rev-b", NOW - 300_000, "req-1"),
            Message {
                ts: NOW - 120_000,
                from: "db-c".into(),
                to: "ops-d".into(),
                kind: MsgKind::Msg,
                label: "schema ready?".into(),
                ..Message::default()
            },
        ],
        todos: vec![todo_file(
            "/w/shop",
            vec![
                TodoItem {
                    detail: "synthetic continuation line".into(),
                    ..todo(3, "pool item one")
                },
                TodoItem {
                    waits_on_you: true,
                    ..todo(9, "pick a synthetic option")
                },
            ],
            0,
        )],
        ..Snapshot::default()
    }
}

// ---------------------------------------------------------------- AC1

#[test]
fn ac1_b_opens_backlog_from_every_view() {
    let mut app = app_with(every_kind(), "en");
    for v in View::ALL {
        app.view = v;
        press(&mut app, KeyCode::Char('b'));
        assert_eq!(app.view, View::Queue, "b from {v:?}");
    }
}

#[test]
fn ac1_tab_and_backtab_cycles_reach_backlog() {
    let mut app = app_with(every_kind(), "en");
    for v in View::ALL {
        app.view = v;
        let mut seen = false;
        for _ in 0..View::ALL.len() {
            press(&mut app, KeyCode::Tab);
            seen |= app.view == View::Queue;
        }
        assert!(seen, "Tab from {v:?} never reaches the Backlog");
        assert_eq!(app.view, v, "a full Tab cycle returns to {v:?}");
        let mut seen = false;
        for _ in 0..View::ALL.len() {
            press(&mut app, KeyCode::BackTab);
            seen |= app.view == View::Queue;
        }
        assert!(seen, "BackTab from {v:?} never reaches the Backlog");
    }
}

#[test]
fn ac1_view_command_names_in_both_languages() {
    let mut app = app_with(every_kind(), "en");
    for name in ["backlog", "Backlog", "queue"] {
        app.view = View::Overview;
        typed(&mut app, &format!(":view {name}"));
        assert_eq!(app.view, View::Queue, ":view {name}");
    }
    let mut app = app_with(every_kind(), "tr");
    for cmd in [":view kuyruk", ":görünüm Kuyruk"] {
        app.view = View::Overview;
        typed(&mut app, cmd);
        assert_eq!(app.view, View::Queue, "{cmd}");
    }
}

#[test]
fn ac1_tabs_and_footer_list_backlog_en_tr() {
    for (lang, tab, footer, mv, open) in [
        (
            "en",
            "b Backlog",
            "backlog",
            "choose item",
            "owner's detail",
        ),
        ("tr", "b Kuyruk", "kuyruk", "madde seç", "sahibin ayrıntısı"),
    ] {
        let mut app = app_with(every_kind(), lang);
        app.view = View::Overview;
        let s = screen(&app, 260, 6);
        assert!(s.contains(tab), "{lang} tabs: {s}");
        assert!(s.contains(footer), "{lang} footer: {s}");
        app.view = View::Queue;
        let s = screen(&app, 260, 30);
        let last = s.lines().rfind(|l| !l.trim().is_empty()).unwrap();
        assert!(last.contains(mv), "{lang} Backlog footer: {last}");
        assert!(last.contains(open), "{lang} Backlog footer: {last}");
    }
}

// ---------------------------------------------------------------- i18n

const QUEUE_KEYS: &[&str] = &[
    "view-queue",
    "pane-queue",
    "pane-queue-detail",
    "queue-now",
    "queue-queued",
    "queue-waiting",
    "queue-you",
    "queue-pool",
    "queue-group",
    "queue-bottleneck",
    "queue-totals",
    "queue-reason-now",
    "queue-reason-turn",
    "queue-reason-tasks",
    "queue-reason-stage",
    "queue-reason-rejected",
    "queue-reason-blocked",
    "queue-reason-agent",
    "queue-reason-you",
    "queue-reason-approval",
    "queue-reason-pool",
    "queue-reason-waits-on",
    "queue-src-task",
    "queue-src-run",
    "queue-src-request",
    "queue-src-agent",
    "queue-more",
    "queue-root",
    "queue-holds",
    "queue-detail-owner",
    "queue-detail-state",
    "queue-detail-age",
    "queue-detail-description",
    "queue-detail-context",
    "queue-detail-source",
    "queue-detail-blockers",
    "queue-detail-chain",
    "queue-age-file",
    "queue-empty",
    "queue-todos-off",
    "queue-no-owner",
    "key-queue",
    "key-queue-move",
    "key-open-owner",
    "task-blocked",
];

#[test]
fn i18n_every_backlog_key_is_in_en_and_tr() {
    let files = [
        ("en", include_str!("../locales/en.ftl")),
        ("tr", include_str!("../locales/tr.ftl")),
    ];
    for (lang, src) in files {
        for k in QUEUE_KEYS {
            let defined = src.lines().any(|l| l.starts_with(&format!("{k} =")));
            assert!(defined, "{lang}.ftl lacks `{k}`");
        }
        let i = I18n::new(Some(lang));
        let args: &[(&str, &str)] = &[
            ("n", "3"),
            ("pos", "1"),
            ("ids", "#3"),
            ("stage", "dev"),
            ("decision", "RET"),
            ("who", "db-c"),
            ("what", "legal"),
            ("id", "7"),
            ("file", "/w/x/tasks/todo.md"),
            ("wait", "5m"),
            ("you", "2"),
            ("ago", "1h"),
        ];
        for k in QUEUE_KEYS {
            let s = i.t_args(k, args);
            assert_ne!(s, *k, "{lang}: `{k}` falls back to its id");
            assert!(!s.contains('{') && !s.contains('$'), "{lang} `{k}`: {s}");
            assert!(!s.contains('\u{2068}'), "{lang} `{k}` has isolation marks");
        }
    }
    // The Turkish words differ from the English ones (really translated).
    let (en, tr) = (I18n::new(Some("en")), I18n::new(Some("tr")));
    for k in ["view-queue", "queue-you", "queue-pool", "queue-no-owner"] {
        assert_ne!(en.t(k), tr.t(k), "`{k}` is not translated");
    }
}

// ---------------------------------------------------------------- AC2

#[test]
fn ac2_owner_by_name_positions_now_and_waiting() {
    let mut a = agent("agent-0001", "t", AgentState::Working, "/w/t");
    a.name = "builder".into();
    let s = Snapshot {
        taken_at: NOW,
        agents: vec![a],
        tasks: vec![
            task("g", "b", Some("builder"), TaskStatus::Pending, &[]),
            task("g", "10", Some("builder"), TaskStatus::Pending, &[]),
            task("g", "2", Some("builder"), TaskStatus::Pending, &[]),
            task("g", "5", Some("builder"), TaskStatus::Pending, &["3"]),
            task("g", "6", Some("builder"), TaskStatus::InProgress, &[]),
        ],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    assert_eq!(q.owners.len(), 1);
    let g = &q.owners[0];
    assert_eq!(g.label, "builder");
    let rows: Vec<(String, Lane, Reason)> = g
        .items
        .iter()
        .map(|i| (i.source.clone(), i.lane, i.reason.clone()))
        .collect();
    assert_eq!(
        rows,
        vec![
            ("task #6".into(), Lane::Now, Reason::Working),
            (
                "task #2".into(),
                Lane::Queued,
                Reason::Turn { pos: 1, of: 3 }
            ),
            (
                "task #10".into(),
                Lane::Queued,
                Reason::Turn { pos: 2, of: 3 }
            ),
            (
                "task #b".into(),
                Lane::Queued,
                Reason::Turn { pos: 3, of: 3 }
            ),
            (
                "task #5".into(),
                Lane::Waiting,
                Reason::WaitsTasks(vec!["3".into()])
            ),
        ]
    );
    assert!(g
        .items
        .iter()
        .all(|i| i.owner_id.as_deref() == Some("agent-0001")));
    // Rendered: the waiting row names #3, the queued rows their position.
    let mut app = app_with(s, "en");
    app.view = View::Queue;
    app.queue_sel = Some(tkey("g", "5"));
    let scr = screen(&app, 160, 40);
    assert!(scr.contains("waits for #3"), "{scr}");
    assert!(scr.contains("its turn: 1 of 3"), "{scr}");
    assert!(scr.contains("working on it"), "{scr}");
}

#[test]
fn ac2_done_blockers_release_unknown_and_open_ones_hold() {
    let s = Snapshot {
        agents: vec![agent("x", "t", AgentState::Working, "/w/t")],
        tasks: vec![
            task("g", "1", Some("x"), TaskStatus::Done, &[]),
            task("g", "2", Some("x"), TaskStatus::Pending, &[]),
            // 1 is done: 2 is the open blocker and the chain follows it.
            task("g", "3", Some("x"), TaskStatus::Pending, &["1", "2"]),
            // An id nobody knows still blocks.
            task("g", "4", Some("x"), TaskStatus::Pending, &["404"]),
            // Only done blockers: its turn.
            task("g", "5", Some("x"), TaskStatus::Pending, &["1"]),
        ],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let three = q.find(&tkey("g", "3")).unwrap();
    assert_eq!(three.reason, Reason::WaitsTasks(vec!["2".into()]));
    assert_eq!(
        three.blockers.len(),
        2,
        "every blocker is listed in the detail"
    );
    assert_eq!(three.chain[0].key.as_deref(), Some(tkey("g", "2").as_str()));
    let four = q.find(&tkey("g", "4")).unwrap();
    assert_eq!(four.lane, Lane::Waiting);
    assert_eq!(four.chain.len(), 1);
    assert_eq!(four.chain[0].label, "#404");
    assert_eq!(four.chain[0].key, None);
    let five = q.find(&tkey("g", "5")).unwrap();
    assert_eq!(five.lane, Lane::Queued);
    assert!(matches!(five.reason, Reason::Turn { .. }));
    assert!(q.find(&tkey("g", "1")).is_none(), "done tasks never show");
}

/// Agent Teams numbers tasks per team list (`~/.claude/tasks/<group>/`), so two
/// groups both have a task "3". A blocker id refers to the task of its own group.
#[test]
fn ac2_blocker_ids_resolve_within_their_own_task_group() {
    let s = Snapshot {
        agents: vec![
            agent("x", "alpha", AgentState::Working, "/w/a"),
            agent("y", "beta", AgentState::Working, "/w/b"),
        ],
        tasks: vec![
            task("alpha", "3", Some("x"), TaskStatus::Done, &[]),
            task("alpha", "5", Some("x"), TaskStatus::Pending, &["3"]),
            task("beta", "3", Some("y"), TaskStatus::Pending, &[]),
        ],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let five = q.find(&tkey("alpha", "5")).unwrap();
    assert!(
        matches!(five.reason, Reason::Turn { .. }),
        "alpha #5 is blocked only by alpha #3, which is done; got {:?} chain {:?}",
        five.reason,
        five.chain
    );
}

// ---------------------------------------------------------------- AC3

#[test]
fn ac3_test_stage_waits_for_dev_for_the_test_role() {
    use StageStatus::*;
    let s = Snapshot {
        taken_at: NOW,
        agents: vec![agent("dev-a", "shop", AgentState::Working, "/w/shop")],
        runs: vec![
            run(
                "R-20261116-0900-shop-one",
                "shop",
                &[
                    ("dev", Running, "orchestra-developer", "dev-a"),
                    ("test", Pending, "orchestra-test-writer", ""),
                    ("qa", Pending, "orchestra-qa-reviewer", ""),
                ],
            ),
            // No run.json: no planned agent type, the stage names the role.
            run(
                "R-20261116-0901-shop-two",
                "shop",
                &[("dev", Running, "", "dev-a"), ("test", Pending, "", "")],
            ),
        ],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let t = q.find("run:R-20261116-0900-shop-one:test").unwrap();
    assert_eq!(t.lane, Lane::Queued);
    assert_eq!(t.owner_label, "orchestra-test-writer");
    assert_eq!(t.reason, Reason::WaitsStage("dev".into()));
    let t2 = q.find("run:R-20261116-0901-shop-two:test").unwrap();
    assert_eq!(t2.owner_label, "stage-test");
    assert_eq!(t2.reason, Reason::WaitsStage("dev".into()));
    for (lang, text) in [("en", "waits for dev"), ("tr", "dev aşaması bekleniyor")] {
        let mut app = app_with(s.clone(), lang);
        app.view = View::Queue;
        app.queue_sel = Some(t.key.clone());
        let scr = screen(&app, 180, 50);
        assert!(scr.contains(text), "{lang}: {scr}");
        assert!(scr.contains("orchestra-test-writer"), "{lang}: {scr}");
    }
}

// ---------------------------------------------------------------- AC4

#[test]
fn ac4_open_approval_is_waiting_on_you_with_age() {
    let mut s = Snapshot {
        taken_at: NOW,
        agents: vec![
            agent("rev-b", "t", AgentState::Waiting, "/w/t"),
            agent("old-z", "t", AgentState::Done, "/w/t"),
        ],
        messages: vec![
            perm("rev-b", NOW - 300_000, "req-open"),
            Message {
                answered_at: Some(NOW - 1_000),
                ..perm("rev-b", NOW - 900_000, "req-answered")
            },
            perm("old-z", NOW - 200_000, "req-of-finished"),
        ],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let keys: Vec<&str> = q.you.iter().map(|i| i.key.as_str()).collect();
    assert_eq!(keys, ["req:req-open"]);
    assert_eq!(q.you[0].reason, Reason::Approval);
    assert_eq!(q.you[0].since, NOW - 300_000);
    s.agents.retain(|a| a.state != AgentState::Done);
    for (lang, head, reason) in [
        ("en", "Waiting on you", "waits for your approval · 5m"),
        ("tr", "Senden beklenenler", "onayını bekliyor · 5m"),
    ] {
        let mut app = app_with(s.clone(), lang);
        app.view = View::Queue;
        let scr = screen(&app, 140, 30);
        assert!(scr.contains(head), "{lang}: {scr}");
        assert!(scr.contains(reason), "{lang}: {scr}");
    }
}

// ---------------------------------------------------------------- AC5 parsing

fn titles(items: &[TodoItem]) -> Vec<&str> {
    items.iter().map(|i| i.title.as_str()).collect()
}

#[test]
fn ac5_crlf_tabs_and_mixed_indentation() {
    let text = "## Plan\r\n- [ ] first\r\n\tdetail by tab\r\n    detail by spaces\r\n\t- [ ] child by tab\r\n\t\tchild detail\r\n- [x] closed\r\n\tclosed detail\r\n* [ ] second\r\n";
    let (items, omitted) = parse(text);
    assert_eq!(omitted, 0);
    assert_eq!(titles(&items), ["first", "child by tab", "second"]);
    assert_eq!(items[0].detail, "detail by tab\ndetail by spaces");
    assert_eq!(items[1].parent, "first");
    assert_eq!(items[1].detail, "child detail");
    assert!(items
        .iter()
        .all(|i| !i.title.contains('\r') && !i.detail.contains('\r')));
    assert_eq!(items[2].line, 9);
    assert_eq!(items[0].section, "Plan");
}

#[test]
fn ac5_checked_boxes_never_appear_even_under_waiting_headings() {
    let text = "## Waiting on you\n- [x] decided already\n- [X] also decided\n- [ ] still open\n## Backlog\n- [x] shipped\n  - [x] shipped child\n";
    let (items, _) = parse(text);
    assert_eq!(titles(&items), ["still open"]);
    assert!(items[0].waits_on_you);
    let s = Snapshot {
        todos: vec![todo_file("/w/p", items, 0)],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let all: Vec<&str> = q.items().iter().map(|i| i.title.as_str()).collect();
    assert_eq!(all, ["still open"]);
}

#[test]
fn ac5_waiting_heading_scope_and_turkish_folding() {
    let text = "\
# Project
## KULLANICIDAN BEKLENENLER
- [ ] one
#### deep under it
- [ ] two
### Sibling below level two
- [ ] three
## Waiting for user
- [ ] four
# Top level ends it
- [ ] five
## **Onay bekleyenler** (2)
- [ ] six
    # indented heading is text, not a heading
- [ ] seven
## Onay
- [ ] eight
## Kullanıcı arayüzü kararları
- [ ] nine
## Decision log
- [ ] ten
";
    let (items, _) = parse(text);
    let got: Vec<(&str, bool)> = items
        .iter()
        .map(|i| (i.title.as_str(), i.waits_on_you))
        .collect();
    assert_eq!(
        got,
        [
            ("one", true),
            ("two", true),
            ("three", true),
            ("four", true),
            ("five", false),
            ("six", true),
            ("seven", true),
            ("eight", false),
            ("nine", false),
            ("ten", false),
        ]
    );
    assert_eq!(items[1].section, "KULLANICIDAN BEKLENENLER › deep under it");
    for h in [
        "Senden Beklenenler",
        "SİZDEN BEKLENENLER",
        "Karar Verilecekler",
        "AÇIK SORULAR",
        "Karar gerekenler",
        "decisions   needed",
        "AWAITING APPROVAL",
        "Questions for you",
        "needs your review",
    ] {
        assert!(waits_on_you_heading(h), "{h}");
    }
    for h in [
        "Karar",
        "Kararlar",
        "Onay",
        "Kullanıcı",
        "Decision",
        "Approved",
        "Questions",
    ] {
        assert!(!waits_on_you_heading(h), "{h}");
    }
    assert_eq!(fold("ĞÜŞİÖÇ ığüşöç\tIİ"), "gusioc igusoc ii");
}

#[test]
fn ac5_huge_and_malformed_inputs_hold_the_caps() {
    // 300 000 open boxes.
    let mut text = String::with_capacity(4 << 20);
    for k in 0..300_000 {
        text.push_str("- [ ] box ");
        text.push_str(&k.to_string());
        text.push('\n');
    }
    let (items, omitted) = parse(&text);
    assert_eq!(items.len(), MAX_ITEMS_PER_FILE);
    assert_eq!(omitted as usize, 300_000 - MAX_ITEMS_PER_FILE);
    assert_eq!(items[0].title, "box 0", "kept in file order");
    // Deep nesting: 5 000 levels.
    let mut deep = String::new();
    for k in 0..5_000 {
        deep.push_str(&" ".repeat(k));
        deep.push_str("- [ ] level\n");
    }
    let (items, _) = parse(&deep);
    assert_eq!(items.len(), MAX_ITEMS_PER_FILE);
    assert_eq!(items[1].parent, "level");
    // Odd bytes and shapes never panic.
    let garbage: Vec<u8> = (0..200_000u32)
        .map(|k| (k.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let _ = parse_bytes(&garbage);
    for t in [
        "#".repeat(100_000),
        format!("- [ ] {}", "ş".repeat(100_000)),
        format!("- [ ] (waiting on: {})", "ğ".repeat(10_000)),
        "- [ ] (waiting on: unclosed".into(),
        "- [ ] )(".into(),
        "- [ ] ((((((((".into(),
        "\u{feff}- [ ] bom first".into(),
        "- [\u{a0}] nbsp box".into(),
        "- [ ] \u{0}\u{1}\u{7f} control".into(),
        "\r\r\r- [ ] lone cr".into(),
        "   \n\t\n- [ ] after blanks\n\n\n   continued after blanks".into(),
    ] {
        let _ = parse(&t);
    }
}

#[test]
fn ac5_blank_lines_keep_and_margin_ends_continuation() {
    let (items, _) = parse("- [ ] a\n\n\n   after blanks\n- [ ] b\nmargin text\n   orphan\n");
    assert_eq!(items[0].detail, "after blanks");
    assert_eq!(items[1].detail, "", "a line at the margin ends the item");
}

#[test]
fn ac5_reader_caps_files_and_keeps_omitted_stable() {
    let base = temp_dir("reader");
    // Three projects with 200 open items each: 600 > MAX_TODO_ITEMS.
    let mut paths = Vec::new();
    for p in ["p1", "p2", "p3"] {
        let dir = base.join(p).join("tasks");
        std::fs::create_dir_all(&dir).unwrap();
        let mut t = String::new();
        for k in 0..MAX_ITEMS_PER_FILE {
            t.push_str(&format!("- [ ] item {k}\n"));
        }
        std::fs::write(dir.join("todo.md"), t).unwrap();
        paths.push(dir.join("todo.md"));
    }
    let now = crate::model::now_millis();
    let mut r = TodoReader::default();
    let first = r.read(&paths, now);
    let total: usize = first.iter().map(|f| f.items.len()).sum();
    assert_eq!(total, MAX_TODO_ITEMS);
    let omitted: u32 = first.iter().map(|f| f.omitted).sum();
    assert_eq!(omitted as usize, 3 * MAX_ITEMS_PER_FILE - MAX_TODO_ITEMS);
    // Cached reads do not add the total cap's cut again.
    for _ in 0..3 {
        let again = r.read(&paths, now);
        let o: u32 = again.iter().map(|f| f.omitted).sum();
        assert_eq!(o, omitted);
    }
    assert_eq!(r.parses, 3);
    // A file that disappears is dropped.
    std::fs::remove_file(&paths[0]).unwrap();
    let after = r.read(&paths, now);
    assert_eq!(after.len(), 2);
    // A 2 MiB file with a single line and no newline: read up to the byte cap.
    let one = base.join("p2").join("tasks").join("todo.md");
    std::fs::write(&one, format!("- [ ] {}", "y".repeat(2 * MAX_FILE_BYTES))).unwrap();
    let files = r.read(&paths[1..2], now);
    assert_eq!(files[0].items.len(), 1);
    assert_eq!(files[0].items[0].title.chars().count(), 300);
    assert_eq!(files[0].omitted, 1, "a truncated file counts one omitted");
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn ac5_discovery_respects_folder_boundaries_depth_and_count() {
    let base = temp_dir("discovery");
    let b = base.join("a").join("b");
    let bc = base.join("a").join("bc");
    std::fs::create_dir_all(b.join("tasks")).unwrap();
    std::fs::create_dir_all(&bc).unwrap();
    std::fs::write(b.join("tasks").join("todo.md"), "- [ ] x\n").unwrap();
    // `/a/bc` is not inside `/a/b`.
    assert!(find_todo_files([bc.to_str().unwrap()].into_iter()).is_empty());
    // Three levels up is found, four is not.
    let three = b.join("1").join("2").join("3");
    let four = three.join("4");
    std::fs::create_dir_all(&four).unwrap();
    assert_eq!(
        find_todo_files([three.to_str().unwrap()].into_iter()).len(),
        1
    );
    assert!(find_todo_files([four.to_str().unwrap()].into_iter()).is_empty());
    // Many sessions in the same project: one file.
    let cwds: Vec<String> = (0..10).map(|_| b.display().to_string()).collect();
    assert_eq!(find_todo_files(cwds.iter().map(String::as_str)).len(), 1);
    // More projects than the cap.
    let mut many = Vec::new();
    for k in 0..(MAX_TODO_FILES + 5) {
        let p = base.join(format!("proj{k}"));
        std::fs::create_dir_all(p.join("tasks")).unwrap();
        std::fs::write(p.join("tasks").join("todo.md"), "- [ ] x\n").unwrap();
        many.push(p.display().to_string());
    }
    assert_eq!(
        find_todo_files(many.iter().map(String::as_str)).len(),
        MAX_TODO_FILES
    );
    // A folder named todo.md is not a file.
    let odd = base.join("odd");
    std::fs::create_dir_all(odd.join("tasks").join("todo.md")).unwrap();
    assert!(find_todo_files([odd.to_str().unwrap()].into_iter()).is_empty());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn ac5_projects_with_the_same_folder_name_stay_apart() {
    let s = Snapshot {
        todos: vec![
            todo_file("/w/client-a/shop", vec![todo(1, "a item")], 4),
            todo_file("/w/client-b/shop", vec![todo(1, "b item")], 0),
        ],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    assert_eq!(
        q.pool.len(),
        2,
        "two project folders, two pool groups: {:#?}",
        q.pool
    );
    // Whatever the labels, the `+N more` file must be the file that omitted.
    for p in &q.pool {
        if p.omitted > 0 {
            assert_eq!(p.file, "/w/client-a/shop/tasks/todo.md");
        }
    }
}

#[test]
fn ac5_todo_items_land_in_pool_and_you_with_section_and_parent() {
    let text = "## Waiting on you\n- [ ] pick\n## Work\n- [ ] parent\n  - [ ] child\n    note\n";
    let (items, _) = parse(text);
    let s = Snapshot {
        todos: vec![todo_file("/w/proj", items, 0)],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    assert_eq!(q.you.len(), 1);
    assert_eq!(q.you[0].title, "pick");
    assert_eq!(q.pool.len(), 1);
    assert_eq!(q.pool[0].label, "proj");
    let child = &q.pool[0].items[1];
    assert_eq!(child.context, "Work › parent");
    assert_eq!(child.detail, "note");
    assert_eq!(child.location, "/w/proj/tasks/todo.md:5");
    assert_eq!(child.reason, Reason::Unassigned);
}

// ---------------------------------------------------------------- Addendum A2

#[test]
fn a2_markers_case_folding_and_positions() {
    let cases: &[(&str, &str, &str)] = &[
        ("Ship (WAITING ON: legal)", "Ship", "legal"),
        ("Ship (Waiting for vendor)", "Ship", "vendor"),
        ("Ship (blocked by:   #12  )", "Ship", "#12"),
        ("Rapor (BEKLİYOR: tasarım)", "Rapor", "tasarım"),
        ("Rapor (Beklenen: onay)", "Rapor", "onay"),
        ("Erişim — ENGEL: VPN", "Erişim", "VPN"),
        ("Fix (a) bug (waiting on: ops)", "Fix (a) bug", "ops"),
        ("Fix; blocked by: #3", "Fix", "#3"),
        ("Fix - waiting on: ops", "Fix", "ops"),
        // Not markers.
        ("Ship (blocked by:)", "Ship (blocked by:)", ""),
        ("Engelli kullanıcı ekranı", "Engelli kullanıcı ekranı", ""),
        (
            "Waiting on the build server",
            "Waiting on the build server",
            "",
        ),
        ("Bekliyoruz - not a marker", "Bekliyoruz - not a marker", ""),
        ("Engel kaldır - deploy", "Engel kaldır - deploy", ""),
    ];
    for (input, title, what) in cases {
        let (t, w) = split_wait(input);
        assert_eq!((t.as_str(), w.as_str()), (*title, *what), "{input}");
    }
    let (_, long) = split_wait(&format!("x (beklenen: {})", "ç".repeat(500)));
    assert_eq!(long.chars().count(), 120);
}

#[test]
fn a2_markers_on_continuation_lines() {
    let text = "\
- [ ] bullet marker
  - blocked by: #7
- [ ] second line only
  plain first line
  waiting on: vendor
- [ ] title wins (waiting on: legal)
  bekliyor: someone else
";
    let (items, _) = parse(text);
    let got: Vec<(&str, &str)> = items
        .iter()
        .map(|i| (i.title.as_str(), i.waits_on.as_str()))
        .collect();
    assert_eq!(
        got,
        [
            ("bullet marker", "#7"),
            ("second line only", ""),
            ("title wins", "legal"),
        ]
    );
}

#[test]
fn a2_marker_only_item_is_not_lost() {
    let (items, _) = parse("- [ ] (blocked by: #12)\n- [ ] **(waiting on: legal)**\n");
    assert_eq!(items.len(), 2, "both open boxes must appear: {items:?}");
    assert_eq!(items[0].waits_on, "#12");
}

#[test]
fn a2_queue_routes_markers_to_you_pool_and_chain() {
    let mk = |line, waits: &str| TodoItem {
        waits_on: waits.into(),
        ..todo(line, &format!("item {line}"))
    };
    let s = Snapshot {
        taken_at: NOW,
        agents: vec![
            agent("x", "t", AgentState::Working, "/w/p"),
            agent("w", "t", AgentState::Waiting, "/w/p"),
        ],
        tasks: vec![
            task("g", "8", Some("x"), TaskStatus::Pending, &["7"]),
            task("g", "7", Some("w"), TaskStatus::Blocked, &[]),
        ],
        messages: vec![perm("w", NOW - 60_000, "req-w")],
        todos: vec![todo_file(
            "/w/p",
            vec![
                mk(1, "SEN"),
                mk(2, "the user"),
                mk(3, "Kullanıcıdan onay"),
                mk(4, "vendor reply"),
                mk(5, "#99"),
                mk(6, "#8"),
            ],
            0,
        )],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let you: Vec<&str> = q
        .you
        .iter()
        .filter(|i| i.key.starts_with("todo:"))
        .map(|i| i.title.as_str())
        .collect();
    assert_eq!(you.len(), 3, "{you:?}");
    for t in ["item 1", "item 2", "item 3"] {
        assert!(you.contains(&t), "{t} waits on the user: {you:?}");
    }
    let four = q.find("todo:/w/p/tasks/todo.md:4").unwrap();
    assert_eq!(four.reason, Reason::WaitsOn("vendor reply".into()));
    assert_eq!(four.lane, Lane::Waiting);
    assert_eq!(four.chain.len(), 1);
    assert_eq!(four.chain[0].key, None);
    let five = q.find("todo:/w/p/tasks/todo.md:5").unwrap();
    assert_eq!(five.chain[0].label, "#99");
    assert_eq!(five.chain[0].key, None, "#99 is no known task");
    // #8 → task 8 → task 7 → agent w's approval: the root waits on you.
    let six = q.find("todo:/w/p/tasks/todo.md:6").unwrap();
    let keys: Vec<Option<&str>> = six.chain.iter().map(|l| l.key.as_deref()).collect();
    assert_eq!(
        keys,
        [
            Some(tkey("g", "8").as_str()),
            Some(tkey("g", "7").as_str()),
            Some("req:req-w"),
        ]
    );
    assert!(six.root_link().unwrap().reason.on_you());
    let req = q.you.iter().find(|i| i.key == "req:req-w").unwrap();
    // Held: task 7 (w's row), task 8, todo 6.
    assert_eq!(req.holds, 3);
}

// ---------------------------------------------------------------- Addendum A1

#[test]
fn a1_self_block_and_three_cycle_end_with_a_cut() {
    let s = Snapshot {
        agents: vec![agent("x", "t", AgentState::Working, "/w/t")],
        tasks: vec![
            task("g", "1", Some("x"), TaskStatus::Pending, &["1"]),
            task("g", "2", Some("x"), TaskStatus::Pending, &["3"]),
            task("g", "3", Some("x"), TaskStatus::Pending, &["4"]),
            task("g", "4", Some("x"), TaskStatus::Pending, &["2"]),
        ],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let one = q.find(&tkey("g", "1")).unwrap();
    assert_eq!(one.chain.last().unwrap().label, "…");
    assert!(one.chain.len() <= 2);
    for id in ["2", "3", "4"] {
        let it = q.find(&tkey("g", id)).unwrap();
        assert_eq!(it.chain.len(), 3, "#{id}: {:?}", it.chain);
        assert_eq!(it.chain[2].label, "…");
        assert_eq!(it.root, Some(2));
    }
}

#[test]
fn a1_chain_cap_boundary() {
    // 0 → 1 → ... → MAX_CHAIN (in progress): exactly MAX_CHAIN links, no cut.
    let mut tasks = vec![task(
        "g",
        &MAX_CHAIN.to_string(),
        Some("x"),
        TaskStatus::InProgress,
        &[],
    )];
    for k in 0..MAX_CHAIN {
        let dep = (k + 1).to_string();
        tasks.push(task(
            "g",
            &k.to_string(),
            Some("x"),
            TaskStatus::Pending,
            &[&dep],
        ));
    }
    let s = Snapshot {
        agents: vec![agent("x", "t", AgentState::Working, "/w/t")],
        tasks,
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let zero = q.find(&tkey("g", "0")).unwrap();
    assert_eq!(zero.chain.len(), MAX_CHAIN);
    assert!(zero.chain.iter().all(|l| l.label != "…"));
    assert_eq!(zero.root_link().unwrap().reason, Reason::Working);
    // One more link: cut with `…` after MAX_CHAIN links.
    let mut s2 = s.clone();
    s2.tasks
        .push(task("g", "x0", Some("x"), TaskStatus::Pending, &["0"]));
    let q = queue::build(&s2);
    let x0 = q.find(&tkey("g", "x0")).unwrap();
    assert_eq!(x0.chain.len(), MAX_CHAIN + 1);
    assert_eq!(x0.chain[MAX_CHAIN].label, "…");
}

#[test]
fn a1_holds_order_and_unknown_agent_label() {
    let s = Snapshot {
        taken_at: NOW,
        agents: vec![
            agent("a", "t", AgentState::Waiting, "/w/t"),
            agent("b", "t", AgentState::Waiting, "/w/t"),
            agent("c", "t", AgentState::Waiting, "/w/t"),
            agent("d", "t", AgentState::Waiting, "/w/t"),
            agent("e", "t", AgentState::Waiting, "/w/t"),
        ],
        messages: vec![
            // Oldest approval holds only its own agent.
            perm("a", NOW - 900_000, "req-a"),
            // Newer approval holds b, c and d.
            perm("b", NOW - 10_000, "req-b"),
            Message {
                ts: NOW - 5_000,
                from: "c".into(),
                to: "b".into(),
                kind: MsgKind::Msg,
                label: "ready?".into(),
                ..Message::default()
            },
            Message {
                ts: NOW - 4_000,
                from: "d".into(),
                to: "c".into(),
                kind: MsgKind::Msg,
                label: "and you?".into(),
                ..Message::default()
            },
            // e waits on an agent Argus does not see.
            Message {
                ts: NOW - 3_000,
                from: "e".into(),
                to: "ghost-agent".into(),
                kind: MsgKind::Msg,
                label: "ping".into(),
                ..Message::default()
            },
        ],
        todos: vec![todo_file(
            "/w/t",
            vec![TodoItem {
                waits_on_you: true,
                ..todo(1, "todo for you, very old")
            }],
            0,
        )],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let order: Vec<(&str, usize)> = q.you.iter().map(|i| (i.key.as_str(), i.holds)).collect();
    assert_eq!(
        order,
        [
            ("req:req-b", 3),
            ("req:req-a", 1),
            ("todo:/w/t/tasks/todo.md:1", 0),
        ]
    );
    let d = q.find("agent:d").unwrap();
    assert_eq!(d.chain.last().unwrap().key.as_deref(), Some("req:req-b"));
    let e = q.find("agent:e").unwrap();
    assert_eq!(e.reason, Reason::WaitsAgent("ghost-agent".into()));
    assert_eq!(e.chain.len(), 1);
    assert_eq!(e.chain[0].label, "ghost-agent");
    assert_eq!(e.chain[0].key, None);
    // Rendered: d's row names the root with the ▲ mark, the approval its holds.
    let mut app = app_with(s, "en");
    app.view = View::Queue;
    app.queue_sel = Some("agent:d".into());
    let scr = screen(&app, 200, 60);
    assert!(scr.contains("▲ root: "), "{scr}");
    assert!(scr.contains("holds 3 items"), "{scr}");
    assert!(scr.contains("Why it waits"), "{scr}");
    let mut app = app_with(app.full.clone(), "tr");
    app.view = View::Queue;
    app.queue_sel = Some("agent:d".into());
    let scr = screen(&app, 200, 60);
    assert!(scr.contains("Neden bekliyor"), "{scr}");
    assert!(scr.contains("3 işi tutuyor"), "{scr}");
    assert!(scr.contains("kök: "), "{scr}");
}

// ---------------------------------------------------------------- AC6

#[test]
fn ac6_every_item_is_descriptive_and_row_matches_detail() {
    let s = every_kind();
    let mut app = app_with(s, "en");
    app.view = View::Queue;
    let items: Vec<queue::QueueItem> = app.queue.items().into_iter().cloned().collect();
    assert!(items.len() >= 8, "{}", items.len());
    let kinds = ["task:", "run:", "req:", "agent:", "todo:"];
    for k in kinds {
        assert!(items.iter().any(|i| i.key.starts_with(k)), "no {k} item");
    }
    for it in &items {
        assert!(!it.title.trim().is_empty(), "{it:?}");
        assert!(!it.source.is_empty(), "{it:?}");
        assert!(!it.location.is_empty(), "{it:?}");
        assert!(it.since > 0, "{it:?}");
        app.queue_sel = Some(it.key.clone());
        let scr = screen(&app, 200, 60);
        let head: String = it.title.chars().take(20).collect();
        let shown = scr.lines().filter(|l| l.contains(&head)).count();
        assert!(shown >= 2, "title in the row and the detail: {head}\n{scr}");
        let loc: String = it.location.chars().take(30).collect();
        assert!(scr.contains("Source: "), "{scr}");
        assert!(scr.contains(&loc), "{loc}\n{scr}");
        assert!(scr.contains("State: "), "{scr}");
        if !it.detail.is_empty() {
            let d: String = it.detail.lines().next().unwrap().chars().take(20).collect();
            assert!(scr.contains(&d), "description {d}\n{scr}");
        }
        // The same age in the row and in the detail (one source of truth).
        let age = dur(((NOW - it.since) / 1000) as u64);
        let rows_with_age = scr.lines().filter(|l| l.contains(&age)).count();
        assert!(rows_with_age >= 2, "age {age} for {}\n{scr}", it.key);
    }
}

#[test]
fn ac6_waiting_agent_without_task_text_still_has_a_title() {
    let mut a = agent("solo-a", "t", AgentState::Waiting, "/w/t");
    a.task = String::new();
    a.now = String::new();
    let s = Snapshot {
        agents: vec![a, agent("solo-b", "t", AgentState::Working, "/w/t")],
        messages: vec![Message {
            ts: NOW - 5_000,
            from: "solo-a".into(),
            to: "solo-b".into(),
            kind: MsgKind::Msg,
            label: "synthetic question".into(),
            ..Message::default()
        }],
        ..Snapshot::default()
    };
    let q = queue::build(&s);
    let it = q.find("agent:solo-a").expect("a waiting agent row");
    assert!(!it.title.trim().is_empty(), "row without a title: {it:?}");
}

// ---------------------------------------------------------------- AC7

#[test]
fn ac7_cli_never_opts_in_by_default() {
    let p = |args: &[&str]| crate::Cli::try_parse_from(args).unwrap();
    assert!(matches!(
        p(&["argus", "collect"]).command,
        Some(crate::Cmd::Collect { todos: false })
    ));
    // The viewer's flag does not turn a collector on by itself.
    assert!(matches!(
        p(&["argus", "--remote-todos", "collect"]).command,
        Some(crate::Cmd::Collect { todos: false })
    ));
    let c = p(&["argus", "--host", "me@box-1"]);
    assert!(!c.remote_todos, "remote todos are off unless asked");
    assert!(!c.no_todos, "local todos are on by default");
    let c = p(&["argus", "--host", "me@box-1", "--remote-todos"]);
    assert!(c.remote_todos);
    use crate::source::remote::collect_command;
    for bin in ["argus", "~/.cache/argus/argus", "/opt/my tools/argus"] {
        let off = collect_command(bin, false, true);
        assert!(!off.contains("--todos"), "{off}");
        assert!(off.ends_with(" collect"), "{off}");
        assert!(collect_command(bin, true, true).ends_with(" collect --todos"));
    }
}

#[test]
fn ac7_received_frames_without_opt_in_carry_no_todos() {
    use crate::source::remote::{strip_todos, Frame, PROTOCOL};
    let snap = Snapshot {
        todos: vec![todo_file("/srv/p", vec![todo(1, "remote title")], 2)],
        ..Snapshot::default()
    };
    let line = serde_json::to_string(&Frame {
        v: PROTOCOL,
        snapshot: snap,
    })
    .unwrap();
    let mut got: Frame = serde_json::from_str(&line).unwrap();
    assert_eq!(got.snapshot.todos.len(), 1, "the wire carries them");
    let mut kept = got.snapshot.clone();
    strip_todos(&mut got.snapshot, false);
    assert!(got.snapshot.todos.is_empty());
    assert!(queue::build(&got.snapshot).is_empty());
    strip_todos(&mut kept, true);
    assert_eq!(kept.todos.len(), 1);
}

struct Fixed(Snapshot);

impl Source for Fixed {
    fn poll(&mut self) -> Snapshot {
        self.0.clone()
    }
}

#[test]
fn ac7_multi_source_keeps_local_todos() {
    let local = Snapshot {
        todos: vec![todo_file("/w/local", vec![todo(1, "local item")], 0)],
        ..Snapshot::default()
    };
    let mut m = crate::source::multi::MultiSource::new(vec![Box::new(Fixed(local))], false, true);
    let out = m.poll();
    assert_eq!(
        out.todos.len(),
        1,
        "local todos stay on without --remote-todos"
    );
}

#[test]
fn ac7_claude_source_reads_todos_only_when_asked() {
    let base = temp_dir("claude-src");
    let proj = base.join("proj");
    std::fs::create_dir_all(proj.join("tasks")).unwrap();
    std::fs::write(
        proj.join("tasks").join("todo.md"),
        "- [ ] synthetic open item\n",
    )
    .unwrap();
    let claude = base.join("claude");
    let pdir = claude.join("projects").join("-synthetic-proj");
    std::fs::create_dir_all(&pdir).unwrap();
    let ts = chrono::Utc::now().to_rfc3339();
    let line = serde_json::json!({
        "type": "user",
        "cwd": proj.display().to_string(),
        "sessionId": "sess-synthetic",
        "timestamp": ts,
        "message": {"role": "user", "content": "synthetic prompt"}
    });
    std::fs::write(pdir.join("sess-synthetic.jsonl"), format!("{line}\n")).unwrap();
    use crate::source::claude::ClaudeSource;
    let mut off = ClaudeSource::new(claude.clone());
    let snap = off.poll();
    assert!(
        snap.agents
            .iter()
            .any(|a| a.cwd == proj.display().to_string()),
        "fixture session is seen: {:?}",
        snap.agents.iter().map(|a| &a.cwd).collect::<Vec<_>>()
    );
    assert!(snap.todos.is_empty(), "ClaudeSource::new reads no todos");
    let mut on = ClaudeSource::new(claude.clone()).with_todos(true);
    let snap = on.poll();
    assert_eq!(snap.todos.len(), 1);
    assert_eq!(snap.todos[0].items[0].title, "synthetic open item");
    assert!(
        !snap.todos[0].host.is_empty(),
        "local files carry the host name"
    );
    let mut off = ClaudeSource::new(claude).with_todos(false);
    assert!(off.poll().todos.is_empty());
    let _ = std::fs::remove_dir_all(&base);
}

// ---------------------------------------------------------------- team filter

#[test]
fn team_filter_keeps_todos_by_folder_prefix() {
    let s = Snapshot {
        agents: vec![
            agent("x", "t1", AgentState::Working, "/w/a/b/sub"),
            agent("y", "t2", AgentState::Working, "/w/a/bc"),
        ],
        todos: vec![
            todo_file("/w/a/b", vec![todo(1, "in b")], 0),
            todo_file("/w/a/bc", vec![todo(1, "in bc")], 0),
            todo_file("/w/a/b/sub/deeper", vec![todo(1, "below the cwd")], 0),
        ],
        ..Snapshot::default()
    };
    let t1 = s.scoped(Some("t1"), None);
    let roots: Vec<&str> = t1.todos.iter().map(|f| f.root.as_str()).collect();
    assert_eq!(roots, ["/w/a/b"]);
    let t2 = s.scoped(Some("t2"), None);
    let roots: Vec<&str> = t2.todos.iter().map(|f| f.root.as_str()).collect();
    assert_eq!(roots, ["/w/a/bc"]);
    assert!(!path_within("/w/a/bc", "/w/a/b"));
    assert!(path_within("/w/a/b", "/w/a/b"));
    assert!(path_within("C:\\w\\a\\b\\x", "C:\\w\\a\\b\\"));
    assert!(!path_within("C:\\w\\a\\bc", "C:\\w\\a\\b"));
    // Through the app: the team filter applies to the Backlog.
    let mut app = app_with(s, "en");
    app.team = Some("t1".into());
    // `f` twice refilters with the team set.
    press(&mut app, KeyCode::Char('f'));
    press(&mut app, KeyCode::Char('f'));
    let titles: Vec<&str> = app.queue.items().iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, ["in b"]);
}

#[test]
fn host_filter_keeps_todos_of_that_host() {
    let mut a = todo_file("/w/a", vec![todo(1, "on h1")], 0);
    a.host = "h1".into();
    let mut b = todo_file("/w/b", vec![todo(1, "on h2")], 0);
    b.host = "h2".into();
    let s = Snapshot {
        todos: vec![a, b],
        ..Snapshot::default()
    };
    let h1 = s.scoped(None, Some("h1"));
    assert_eq!(h1.todos.len(), 1);
    assert_eq!(h1.todos[0].host, "h1");
    let q = queue::build(&s);
    let labels: Vec<&str> = q.pool.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["a @h1", "b @h2"]);
}

// ---------------------------------------------------------------- AC8

fn crowded() -> Snapshot {
    let mut s = every_kind();
    let mut items = Vec::new();
    for k in 0..MAX_ITEMS_PER_FILE as u32 {
        items.push(TodoItem {
            detail: format!("detail line for {k}\nsecond line"),
            ..todo(
                k + 1,
                &format!(
                    "crowded todo {k} 这是宽字符 e\u{301} {}",
                    "x".repeat(k as usize % 90)
                ),
            )
        });
    }
    for p in 0..3 {
        s.todos
            .push(todo_file(&format!("/w/crowd{p}"), items.clone(), 50));
    }
    for k in 0..120 {
        s.tasks.push(task(
            "crowd",
            &(100 + k).to_string(),
            Some("dev-a"),
            TaskStatus::Pending,
            &[],
        ));
    }
    s
}

#[test]
fn ac8_tiny_terminals_with_hundreds_of_items_and_last_selection() {
    let mut app = app_with(crowded(), "en");
    app.view = View::Queue;
    assert!(
        app.queue.items().len() >= 700,
        "{}",
        app.queue.items().len()
    );
    let last = app.queue.items().last().unwrap().key.clone();
    for (w, h) in [
        (0, 0),
        (1, 1),
        (0, 30),
        (30, 0),
        (2, 40),
        (40, 10),
        (41, 11),
        (99, 17),
        (99, 18),
        (100, 5),
        (100, 18),
        (120, 40),
        (500, 4),
        (u16::MAX / 64, 7),
    ] {
        for sel in [None, Some(last.clone())] {
            app.queue_sel = sel;
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            terminal
                .draw(|f| crate::ui::draw(f, &app))
                .unwrap_or_else(|e| panic!("{w}x{h}: {e}"));
        }
    }
    // The selected last item stays visible on a small screen.
    app.queue_sel = Some(last.clone());
    let scr = screen(&app, 40, 10);
    assert!(
        scr.contains('►'),
        "selection marker visible at 40x10:\n{scr}"
    );
    let scr = screen(&app, 120, 40);
    assert!(scr.contains('►'), "{scr}");
    let it = app.queue.find(&last).unwrap();
    let head: String = it.title.chars().take(15).collect();
    assert!(scr.contains(&head), "{}\n{scr}", it.title);
}

#[test]
fn ac8_navigation_ends_wraps_and_survives_vanishing_items() {
    let mut app = app_with(crowded(), "en");
    app.view = View::Queue;
    let n = app.queue.items().len();
    press(&mut app, KeyCode::End);
    assert_eq!(app.queue_index(), Some(n - 1));
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.queue_index(), Some(n - 1), "PageDown stops at the end");
    press(&mut app, KeyCode::Down);
    assert_eq!(app.queue_index(), Some(0), "Down wraps to the first item");
    press(&mut app, KeyCode::PageUp);
    assert_eq!(app.queue_index(), Some(0), "PageUp stops at the start");
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.queue_index(), Some(n - 1), "k wraps to the last item");
    app.scroll(1);
    assert_eq!(app.queue_index(), Some(0), "the wheel moves the selection");
    // A selected key that is gone falls back to the first item.
    app.queue_sel = Some("todo:/nowhere:1".into());
    assert_eq!(app.queue_index(), Some(0));
    let _ = screen(&app, 80, 24);
    // An empty Backlog: every key is harmless.
    let mut empty = app_with(Snapshot::default(), "en");
    empty.view = View::Queue;
    for k in [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Enter,
    ] {
        press(&mut empty, k);
    }
    assert_eq!(empty.view, View::Queue);
    assert_eq!(empty.queue_index(), None);
}

#[test]
fn ac8_enter_without_owner_says_so_and_stays() {
    for (lang, msg) in [
        ("en", "This item has no owner yet"),
        ("tr", "Bu maddenin henüz sahibi yok"),
    ] {
        let mut app = app_with(every_kind(), lang);
        app.view = View::Queue;
        // A todo pool item, an unowned task and a run stage queued for a role.
        for key in [
            "todo:/w/shop/tasks/todo.md:3".to_string(),
            tkey("grp", "4"),
            "run:R-20261116-0900-shop-sample:test".to_string(),
        ] {
            app.flash = None;
            app.view = View::Queue;
            app.queue_sel = Some(key.clone());
            assert_eq!(app.queue.find(&key).unwrap().owner_id, None, "{key}");
            press(&mut app, KeyCode::Enter);
            assert_eq!(app.view, View::Queue, "{lang} {key}");
            match &app.flash {
                Some((_, Err(m))) => assert_eq!(m, msg),
                other => panic!("{lang} {key}: {other:?}"),
            }
        }
        // An owned item jumps to its owner.
        app.queue_sel = Some(tkey("grp", "2"));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.view, View::AgentDetail);
        assert_eq!(app.selected.as_deref(), Some("dev-a"));
    }
}

// ---------------------------------------------------------------- caps

/// The queue cap must not hide what waits on the user behind a long list of
/// owned work: approvals are the items the user acts on.
#[test]
fn cap_keeps_items_waiting_on_you() {
    let mut s = Snapshot {
        taken_at: NOW,
        agents: vec![
            agent("x", "t", AgentState::Working, "/w/t"),
            agent("rev", "t", AgentState::Waiting, "/w/t"),
        ],
        messages: vec![perm("rev", NOW - 1_000, "req-cap")],
        ..Snapshot::default()
    };
    s.tasks = (0..(MAX_QUEUE_ITEMS + 50))
        .map(|k| task("g", &k.to_string(), Some("x"), TaskStatus::Pending, &[]))
        .collect();
    let q = queue::build(&s);
    assert!(q.items().len() <= MAX_QUEUE_ITEMS);
    assert!(
        q.you.iter().any(|i| i.key == "req:req-cap"),
        "the open approval was cut by MAX_QUEUE_ITEMS"
    );
}

#[test]
fn caps_constants_are_as_documented() {
    assert_eq!(MAX_TODO_FILES, 20);
    assert_eq!(MAX_FILE_BYTES, 1 << 20);
    assert_eq!(MAX_ITEMS_PER_FILE, 200);
    assert_eq!(MAX_TODO_ITEMS, 500);
    assert_eq!(todos::TODO_MAX_AGE, 30 * 24 * 3600 * 1000);
    assert_eq!(MAX_CHAIN, 8);
    assert_eq!(MAX_QUEUE_ITEMS, 1000);
}
