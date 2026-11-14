//! Application state and input handling.

use std::sync::mpsc::{Receiver, Sender};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::i18n::I18n;
use crate::model::{now_millis, Agent, Message, Millis, MsgKind, Snapshot};
use crate::source::{Channels, Control, Notice};

/// Every screen Argus can show. The order matches the number keys `1`–`0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Overview,
    Sequence,
    AgentDetail,
    Fleet,
    Graph,
    Room,
    Lanes,
    Channel,
    TaskMap,
    Pair,
}

impl View {
    pub const ALL: [View; 10] = [
        View::Overview,
        View::Sequence,
        View::AgentDetail,
        View::Fleet,
        View::Graph,
        View::Room,
        View::Lanes,
        View::Channel,
        View::TaskMap,
        View::Pair,
    ];

    /// Fluent message id for the view's name.
    pub fn key(self) -> &'static str {
        match self {
            View::Overview => "view-overview",
            View::Sequence => "view-sequence",
            View::AgentDetail => "view-agent",
            View::Fleet => "view-fleet",
            View::Graph => "view-graph",
            View::Room => "view-room",
            View::Lanes => "view-lanes",
            View::Channel => "view-channel",
            View::TaskMap => "view-taskmap",
            View::Pair => "view-pair",
        }
    }

    pub fn index(self) -> usize {
        View::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
}

pub struct App {
    pub i18n: I18n,
    pub view: View,
    pub snapshot: Snapshot,
    rx: Receiver<Snapshot>,
    controls: Sender<Control>,
    notices: Receiver<Notice>,
    /// Last command outcome, shown in the footer for a few seconds.
    pub flash: Option<(Millis, Notice)>,
    /// Command bar buffer; starts with `:` (command) or `>` (prompt).
    pub input: Option<String>,
    pub demo: bool,
    /// Id of the selected agent; shared across views.
    pub selected: Option<String>,
    pub paused: bool,
    /// Wall-clock time used for animations (frozen while paused).
    pub now: Millis,
    /// Frame counter for spinners and blinking.
    pub frame: u64,
    pub should_quit: bool,
    /// Index into [`LANE_WINDOWS`].
    pub lanes_zoom: usize,
}

/// Time windows of the Lanes view, in ms (cycled with `z`).
pub const LANE_WINDOWS: [Millis; 4] = [30_000, 60_000, 5 * 60_000, 15 * 60_000];

impl App {
    pub fn new(i18n: I18n, ch: Channels, demo: bool) -> Self {
        Self {
            i18n,
            view: View::Overview,
            snapshot: Snapshot::default(),
            rx: ch.snapshots,
            controls: ch.controls,
            notices: ch.notices,
            flash: None,
            input: None,
            demo,
            selected: None,
            paused: false,
            now: now_millis(),
            frame: 0,
            should_quit: false,
            lanes_zoom: 1,
        }
    }

    pub fn on_tick(&mut self) {
        while let Ok(n) = self.notices.try_recv() {
            self.flash = Some((now_millis(), n));
        }
        if self.paused {
            return;
        }
        self.frame = self.frame.wrapping_add(1);
        self.now = now_millis();
        let mut latest = None;
        while let Ok(s) = self.rx.try_recv() {
            latest = Some(s);
        }
        if let Some(s) = latest {
            self.snapshot = s;
            let valid = self
                .selected
                .as_ref()
                .is_some_and(|id| self.snapshot.agent(id).is_some());
            if !valid {
                self.selected = self.ordered().first().map(|a| a.id.clone());
            }
        }
    }

    /// Agents in tree order (roots followed by their children).
    pub fn ordered(&self) -> Vec<&Agent> {
        self.snapshot
            .tree_order()
            .into_iter()
            .map(|(i, _)| &self.snapshot.agents[i])
            .collect()
    }

    pub fn lanes_window(&self) -> Millis {
        LANE_WINDOWS[self.lanes_zoom % LANE_WINDOWS.len()]
    }

    pub fn selected_agent(&self) -> Option<&Agent> {
        self.selected
            .as_deref()
            .and_then(|id| self.snapshot.agent(id))
    }

    fn move_selection(&mut self, delta: isize) {
        let ids: Vec<String> = self.ordered().iter().map(|a| a.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let cur = self
            .selected
            .as_ref()
            .and_then(|s| ids.iter().position(|i| i == s))
            .unwrap_or(0) as isize;
        let n = ids.len() as isize;
        let next = ((cur + delta) % n + n) % n;
        self.selected = Some(ids[next as usize].clone());
    }

    fn say(&mut self, n: Notice) {
        self.flash = Some((now_millis(), n));
    }

    fn send(&mut self, c: Control) {
        if self.controls.send(c).is_err() {
            self.say(Err("data source stopped".into()));
        }
    }

    /// The approval to answer: the selected agent's, else the oldest pending.
    pub fn pending_approval(&self) -> Option<&Message> {
        let pending = |m: &&Message| {
            m.kind == MsgKind::Perm && m.request.is_some() && m.answered_at.is_none()
        };
        let sel = self.selected.as_deref();
        self.snapshot
            .messages
            .iter()
            .filter(pending)
            .find(|m| Some(m.from.as_str()) == sel)
            .or_else(|| self.snapshot.messages.iter().find(pending))
    }

    fn decide(&mut self, allow: bool) {
        match self
            .pending_approval()
            .map(|m| (m.from.clone(), m.request.clone().unwrap_or_default()))
        {
            Some((agent, request)) => self.send(Control::Decide {
                agent,
                request,
                allow,
            }),
            None => {
                let msg = self.i18n.t("no-approvals");
                self.say(Err(msg))
            }
        }
    }

    fn run_input(&mut self, line: &str) {
        let line = line.trim();
        if let Some(prompt) = line.strip_prefix('>') {
            let prompt = prompt.trim().to_string();
            if prompt.is_empty() {
                return;
            }
            let (host, dir) = match self.selected_agent() {
                Some(a) => (Some(a.host.clone()), a.cwd.clone()),
                None => (None, String::new()),
            };
            self.send(Control::Run { host, dir, prompt });
            return;
        }
        let body = line.trim_start_matches(':');
        let (cmd, rest) = body.split_once(' ').unwrap_or((body, ""));
        let rest = rest.trim();
        match cmd {
            "q" | "quit" => self.should_quit = true,
            "connect" | "bağlan" if !rest.is_empty() => {
                self.send(Control::AddHost(rest.to_string()))
            }
            "msg" | "mesaj" if !rest.is_empty() => match self.selected.clone() {
                Some(agent) => self.send(Control::Resume {
                    agent,
                    text: rest.to_string(),
                }),
                None => {
                    let m = self.i18n.t("agent-none");
                    self.say(Err(m))
                }
            },
            "run" | "çalıştır" if !rest.is_empty() => {
                let (dir, prompt) = rest.split_once(' ').unwrap_or((".", rest));
                self.send(Control::Run {
                    host: None,
                    dir: dir.to_string(),
                    prompt: prompt.to_string(),
                });
            }
            "view" | "görünüm" => {
                let target = View::ALL.iter().position(|v| {
                    let name = self.i18n.t(v.key()).to_lowercase();
                    name == rest.to_lowercase()
                        || format!("{:?}", v).to_lowercase() == rest.to_lowercase()
                });
                let idx = target.or_else(|| {
                    rest.parse::<usize>()
                        .ok()
                        .map(|n| if n == 0 { 9 } else { n.saturating_sub(1) })
                });
                match idx {
                    Some(i) if i < View::ALL.len() => self.view = View::ALL[i],
                    _ => self.say(Err(format!("? {rest}"))),
                }
            }
            "lang" | "dil" if !rest.is_empty() => self.i18n = I18n::new(Some(rest)),
            "approve" | "onayla" => self.decide(true),
            "reject" | "reddet" => self.decide(false),
            _ => {
                let m = self.i18n.t_args("unknown-cmd", &[("cmd", body)]);
                self.say(Err(m))
            }
        }
    }

    fn on_input_key(&mut self, key: KeyEvent) {
        let Some(buf) = self.input.as_mut() else {
            return;
        };
        match key.code {
            KeyCode::Esc => self.input = None,
            KeyCode::Enter => {
                let line = buf.clone();
                self.input = None;
                self.run_input(&line);
            }
            KeyCode::Backspace => {
                buf.pop();
                if buf.is_empty() {
                    self.input = None;
                }
            }
            KeyCode::Char(c) => buf.push(c),
            _ => {}
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        if self.input.is_some() {
            self.on_input_key(key);
            return;
        }
        match key.code {
            KeyCode::Char(':') => self.input = Some(":".into()),
            KeyCode::Char('>') => self.input = Some(">".into()),
            KeyCode::Char('a') => self.decide(true),
            KeyCode::Char('r') => self.decide(false),
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char(' ') => self.paused = !self.paused,
            KeyCode::Char(c @ '0'..='9') => {
                let n = c.to_digit(10).unwrap_or(1) as usize;
                let idx = if n == 0 { 9 } else { n - 1 };
                self.view = View::ALL[idx];
            }
            KeyCode::Tab => self.view = View::ALL[(self.view.index() + 1) % View::ALL.len()],
            KeyCode::BackTab => {
                self.view = View::ALL[(self.view.index() + View::ALL.len() - 1) % View::ALL.len()]
            }
            KeyCode::Enter => {
                if self.view != View::AgentDetail {
                    self.view = View::AgentDetail;
                }
            }
            KeyCode::Esc => self.view = View::Overview,
            KeyCode::Char('z') => self.lanes_zoom = (self.lanes_zoom + 1) % LANE_WINDOWS.len(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            _ => {}
        }
    }
}
