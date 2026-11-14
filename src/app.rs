//! Application state and input handling.

use std::sync::mpsc::{Receiver, Sender};

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::i18n::I18n;
use crate::model::{now_millis, Agent, AgentState, Message, Millis, MsgKind, Snapshot};
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
    Teams,
    History,
}

impl View {
    pub const ALL: [View; 12] = [
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
        View::Teams,
        View::History,
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
            View::Teams => "view-teams",
            View::History => "view-history",
        }
    }

    pub fn index(self) -> usize {
        View::ALL.iter().position(|v| *v == self).unwrap_or(0)
    }
}

pub struct App {
    pub i18n: I18n,
    pub view: View,
    /// What the views show: `full`, filtered to the selected team if any.
    pub snapshot: Snapshot,
    /// Everything the data source reported.
    pub full: Snapshot,
    /// Team filter (`t` cycles through teams).
    pub team: Option<String>,
    /// Host filter (`s` cycles through hosts; Enter on the Fleet view).
    pub host: Option<String>,
    /// Show finished agents in the views (`f`); they are always in History.
    pub show_finished: bool,
    /// Highlighted host on the Fleet view.
    pub host_cursor: usize,
    /// Group Graph/Room by team instead of host (`g`).
    pub group_by_team: bool,
    /// Highlighted card in the Teams view.
    pub team_cursor: usize,
    /// `--debug-keys`: show the last key event in the footer.
    pub debug_keys: bool,
    pub last_key: String,
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
            full: Snapshot::default(),
            team: None,
            host: None,
            show_finished: false,
            host_cursor: 0,
            group_by_team: false,
            team_cursor: 0,
            debug_keys: false,
            last_key: String::new(),
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
            // Setups with configured teams or a coordinator start on the Teams view.
            let first = self.full.taken_at == 0;
            let teams_set_up = !s.team_labels.is_empty() || s.agents.iter().any(|a| a.coordinator);
            self.full = s;
            if first && teams_set_up && self.view == View::Overview {
                self.view = View::Teams;
            }
            self.refilter();
            let valid = self
                .selected
                .as_ref()
                .is_some_and(|id| self.snapshot.agent(id).is_some());
            if !valid {
                self.selected = self.ordered().first().map(|a| a.id.clone());
            }
        }
    }

    fn refilter(&mut self) {
        if let Some(t) = &self.team {
            if !self.full.teams().contains(t) {
                self.team = None;
            }
        }
        if let Some(h) = &self.host {
            if !self.full.host_names().contains(h) {
                self.host = None;
            }
        }
        let mut s = if self.team.is_none() && self.host.is_none() {
            self.full.clone()
        } else {
            self.full.scoped(self.team.as_deref(), self.host.as_deref())
        };
        if !self.show_finished {
            s.hide_finished();
        }
        self.snapshot = s;
        let valid = self
            .selected
            .as_ref()
            .is_some_and(|id| self.snapshot.agent(id).is_some());
        if !valid {
            self.selected = self.ordered().first().map(|a| a.id.clone());
        }
    }

    /// Finished agents within the current team/host filter (for History).
    pub fn finished(&self) -> Vec<&Agent> {
        let mut v: Vec<&Agent> = self
            .full
            .agents
            .iter()
            .filter(|a| a.state == crate::model::AgentState::Done)
            .filter(|a| self.team.as_deref().is_none_or(|t| a.team_key() == t))
            .filter(|a| self.host.as_deref().is_none_or(|h| a.host == h))
            .collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.last_activity));
        v
    }

    fn cycle_host(&mut self) {
        let hosts = self.full.host_names();
        self.host = match &self.host {
            None => hosts.first().cloned(),
            Some(h) => hosts
                .iter()
                .position(|x| x == h)
                .and_then(|i| hosts.get(i + 1).cloned()),
        };
        self.refilter();
    }

    fn on_fleet_key(&mut self, key: KeyEvent) -> bool {
        let hosts = self.full.host_names();
        if hosts.is_empty() {
            return false;
        }
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.host_cursor = (self.host_cursor + 1) % hosts.len()
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.host_cursor = (self.host_cursor + hosts.len() - 1) % hosts.len()
            }
            KeyCode::Enter => {
                self.host = Some(hosts[self.host_cursor.min(hosts.len() - 1)].clone());
                self.refilter();
                self.view = View::Overview;
            }
            _ => return false,
        }
        true
    }

    fn cycle_team(&mut self) {
        let teams = self.full.teams();
        self.team = match &self.team {
            None => teams.first().cloned(),
            Some(t) => teams
                .iter()
                .position(|x| x == t)
                .and_then(|i| teams.get(i + 1).cloned()),
        };
        self.refilter();
        let valid = self
            .selected
            .as_ref()
            .is_some_and(|id| self.snapshot.agent(id).is_some());
        if !valid {
            self.selected = self.ordered().first().map(|a| a.id.clone());
        }
    }

    pub fn set_view_index(&mut self, i: usize) {
        if let Some(v) = View::ALL.get(i) {
            self.view = *v;
        }
    }

    /// Mouse wheel: moves the selection (or the team cursor on the Teams view).
    pub fn scroll(&mut self, delta: isize) {
        if self.view == View::Teams {
            let n = self.team_order().len().max(1);
            self.team_cursor = (self.team_cursor as isize + delta).rem_euclid(n as isize) as usize;
        } else {
            self.move_selection(delta);
        }
    }

    /// Opens a team: every view is filtered to it, starting from the Overview.
    pub fn open_team(&mut self, team: String) {
        self.team = Some(team);
        self.refilter();
        self.selected = self.ordered().first().map(|a| a.id.clone());
        self.view = View::Overview;
    }

    /// Teams as the Teams view lists them: busiest first, then teams whose
    /// agents all finished; groups made only of coordinators are left out.
    pub fn team_order(&self) -> Vec<String> {
        let scope = self.host_scope();
        let mut teams: Vec<(String, usize, Millis)> = scope
            .teams()
            .into_iter()
            .filter_map(|t| {
                let members: Vec<_> = scope
                    .agents
                    .iter()
                    .filter(|a| a.team_key() == t && !a.coordinator)
                    .collect();
                if members.is_empty() {
                    return None;
                }
                let live = members
                    .iter()
                    .filter(|a| a.state != AgentState::Done)
                    .count();
                let last = members.iter().map(|a| a.last_activity).max().unwrap_or(0);
                Some((t, live, last))
            })
            .collect();
        teams.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)));
        teams.into_iter().map(|(t, _, _)| t).collect()
    }

    /// Everything on the chosen host (all hosts when none is chosen), whatever
    /// the team filter: what the Teams view shows.
    pub fn host_scope(&self) -> Snapshot {
        match &self.host {
            Some(h) => self.full.scoped(None, Some(h)),
            None => self.full.clone(),
        }
    }

    fn on_teams_key(&mut self, key: KeyEvent) -> bool {
        let teams = self.team_order();
        let n = teams.len();
        if n == 0 {
            return false;
        }
        match key.code {
            KeyCode::Right | KeyCode::Down | KeyCode::Char('l') | KeyCode::Char('j') => {
                self.team_cursor = (self.team_cursor + 1) % n;
            }
            KeyCode::Left | KeyCode::Up | KeyCode::Char('h') | KeyCode::Char('k') => {
                self.team_cursor = (self.team_cursor + n - 1) % n;
            }
            KeyCode::Enter => {
                let team = teams[self.team_cursor.min(n - 1)].clone();
                self.open_team(team);
            }
            _ => return false,
        }
        true
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
        if self.debug_keys {
            self.last_key = format!("{:?} {:?} {:?}", key.code, key.modifiers, key.kind);
        }
        // Some Windows terminals report Shift+t as `t` + SHIFT instead of `T`.
        let mut key = key;
        if let KeyCode::Char(c) = key.code {
            if key.modifiers.contains(KeyModifiers::SHIFT) && c.is_lowercase() {
                key.code = KeyCode::Char(c.to_uppercase().next().unwrap_or(c));
            }
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        if self.input.is_some() {
            self.on_input_key(key);
            return;
        }
        if self.view == View::Teams && self.on_teams_key(key) {
            return;
        }
        if self.view == View::Fleet && self.on_fleet_key(key) {
            return;
        }
        match key.code {
            KeyCode::Char(':') => self.input = Some(":".into()),
            KeyCode::Char('>') => self.input = Some(">".into()),
            // `t` opens Teams (pick one with Enter); `T` cycles the team filter.
            KeyCode::Char('t') => self.view = View::Teams,
            KeyCode::Char('s') => self.cycle_host(),
            KeyCode::Char('f') => {
                self.show_finished = !self.show_finished;
                self.refilter();
            }
            KeyCode::Char('H') => self.view = View::History,
            KeyCode::F(12) => self.view = View::History,
            KeyCode::Char('T') => self.cycle_team(),
            KeyCode::Char('g') => self.group_by_team = !self.group_by_team,
            KeyCode::Char('a') => self.decide(true),
            KeyCode::Char('r') => self.decide(false),
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char(' ') => self.paused = !self.paused,
            KeyCode::F(n @ 1..=10) => self.set_view_index(n as usize - 1),
            KeyCode::F(11) => self.view = View::Teams,
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
            KeyCode::Esc => {
                if self.view == View::Overview && (self.team.is_some() || self.host.is_some()) {
                    // Leave the team/host: back to everything, where it was chosen.
                    let back = if self.team.is_some() {
                        View::Teams
                    } else {
                        View::Fleet
                    };
                    self.team = None;
                    self.host = None;
                    self.refilter();
                    self.view = back;
                } else {
                    self.view = View::Overview;
                }
            }
            KeyCode::Char('z') => self.lanes_zoom = (self.lanes_zoom + 1) % LANE_WINDOWS.len(),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            _ => {}
        }
    }
}
