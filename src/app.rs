//! Application state and input handling.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::i18n::I18n;
use crate::model::Snapshot;

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
    /// Index of the selected agent; shared across views.
    pub selected: usize,
    pub paused: bool,
    pub tick: u64,
    pub should_quit: bool,
}

impl App {
    pub fn new(i18n: I18n, demo: bool) -> Self {
        // Until the collector lands, the demo snapshot is the only data source.
        let _ = demo;
        Self {
            i18n,
            view: View::Overview,
            snapshot: Snapshot::demo(),
            selected: 0,
            paused: false,
            tick: 0,
            should_quit: false,
        }
    }

    pub fn on_tick(&mut self) {
        if !self.paused {
            self.tick = self.tick.wrapping_add(1);
        }
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
            KeyCode::Down | KeyCode::Char('j') => {
                let n = self.snapshot.agents.len();
                if n > 0 {
                    self.selected = (self.selected + 1) % n;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let n = self.snapshot.agents.len();
                if n > 0 {
                    self.selected = (self.selected + n - 1) % n;
                }
            }
            _ => {}
        }
    }
}
