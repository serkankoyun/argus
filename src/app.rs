//! Application state and input handling.

use std::sync::mpsc::Receiver;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::i18n::I18n;
use crate::model::{now_millis, Agent, Millis, Snapshot};

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

    /// Roadmap milestone in which the view ships (shown on placeholder screens).
    pub fn milestone(self) -> &'static str {
        match self {
            View::Overview => "v0.1",
            View::Graph | View::Room | View::Lanes => "v0.2",
            View::Fleet => "v0.3",
            View::Sequence | View::AgentDetail => "v0.4",
            View::Channel | View::TaskMap | View::Pair => "v1.0",
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
    pub demo: bool,
    /// Id of the selected agent; shared across views.
    pub selected: Option<String>,
    pub paused: bool,
    /// Wall-clock time used for animations (frozen while paused).
    pub now: Millis,
    /// Frame counter for spinners and blinking.
    pub frame: u64,
    pub should_quit: bool,
}

impl App {
    pub fn new(i18n: I18n, rx: Receiver<Snapshot>, demo: bool) -> Self {
        Self {
            i18n,
            view: View::Overview,
            snapshot: Snapshot::default(),
            rx,
            demo,
            selected: None,
            paused: false,
            now: now_millis(),
            frame: 0,
            should_quit: false,
        }
    }

    pub fn on_tick(&mut self) {
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

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        match key.code {
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
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            _ => {}
        }
    }
}
