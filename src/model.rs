//! Core data model shared by every view. A data source (the live collector or
//! the demo simulation) produces a [`Snapshot`]; views only read it.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Milliseconds since the Unix epoch.
pub type Millis = i64;

pub fn now_millis() -> Millis {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as Millis)
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentState {
    Working,
    Waiting,
    Done,
    Idle,
    Stalled,
}

impl AgentState {
    /// Fluent message id.
    pub fn key(self) -> &'static str {
        match self {
            AgentState::Working => "state-working",
            AgentState::Waiting => "state-waiting",
            AgentState::Done => "state-done",
            AgentState::Idle => "state-idle",
            AgentState::Stalled => "state-stalled",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            AgentState::Working => "●",
            AgentState::Waiting => "◐",
            AgentState::Done => "✓",
            AgentState::Idle => "○",
            AgentState::Stalled => "✗",
        }
    }
}

/// Token usage, as reported by the API for each assistant turn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    pub cache_write: u64,
}

impl Usage {
    pub fn add(&mut self, other: &Usage) {
        self.input += other.input;
        self.output += other.output;
        self.cache_read += other.cache_read;
        self.cache_write += other.cache_write;
    }

    /// Tokens that count towards throughput (everything except cache reads).
    pub fn billed(&self) -> u64 {
        self.input + self.output + self.cache_write
    }

    /// Share of prompt tokens served from cache, 0–100.
    pub fn cache_hit_pct(&self) -> u8 {
        let prompt = self.input + self.cache_read + self.cache_write;
        (self.cache_read * 100).checked_div(prompt).unwrap_or(0) as u8
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub ts: Millis,
    pub agent: String,
    pub tool: String,
    /// Short human-readable target, e.g. a file path or command.
    pub target: String,
    pub duration_ms: Option<i64>,
    pub ok: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique id: the session id, or `session/agent-id` for subagents.
    pub id: String,
    pub name: String,
    pub host: String,
    pub project: String,
    pub model: String,
    /// Id of the parent agent, if this is a subagent / teammate.
    pub parent: Option<String>,
    pub team: Option<String>,
    pub state: AgentState,
    /// Tool currently running (or last used).
    pub tool: String,
    /// What the agent is doing right now, in plain words (not translated).
    pub now: String,
    pub usage: Usage,
    pub context_tokens: u64,
    pub context_window: u64,
    pub cost_usd: f64,
    pub started: Millis,
    pub last_activity: Millis,
    pub tool_calls: u32,
    pub errors: u32,
    pub git_branch: String,
    /// Files touched with (added, removed) line counts when known.
    pub files: Vec<(String, i64, i64)>,
    /// Last few lines of the transcript: (timestamp, role, text).
    pub transcript: Vec<(Millis, String, String)>,
}

impl Agent {
    pub fn context_pct(&self) -> u8 {
        (self.context_tokens * 100)
            .checked_div(self.context_window)
            .unwrap_or(0)
            .min(100) as u8
    }

    pub fn elapsed_secs(&self, now: Millis) -> u64 {
        ((now - self.started).max(0) / 1000) as u64
    }

    pub fn tokens(&self) -> u64 {
        self.usage.billed()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MsgKind {
    /// A task handed to another agent.
    Task,
    /// A plain message or question.
    Msg,
    /// A result / completion report.
    Result,
    /// A permission request addressed to the human.
    Perm,
    /// A finding or warning (e.g. a security issue).
    Finding,
}

impl MsgKind {
    pub fn key(self) -> &'static str {
        match self {
            MsgKind::Task => "kind-task",
            MsgKind::Msg => "kind-msg",
            MsgKind::Result => "kind-result",
            MsgKind::Perm => "kind-perm",
            MsgKind::Finding => "kind-finding",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            MsgKind::Task => "●",
            MsgKind::Msg => "◆",
            MsgKind::Result => "✓",
            MsgKind::Perm => "▲",
            MsgKind::Finding => "!",
        }
    }
}

/// Id used for the human operator in messages.
pub const YOU: &str = "@you";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub ts: Millis,
    pub from: String,
    pub to: String,
    pub kind: MsgKind,
    /// Short label shown on arrows and chips.
    pub label: String,
    /// Full text, when available.
    pub text: String,
    /// Set when the message has been answered / completed.
    pub answered_at: Option<Millis>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Blocked,
    Done,
}

impl TaskStatus {
    pub fn key(self) -> &'static str {
        match self {
            TaskStatus::Pending => "task-pending",
            TaskStatus::InProgress => "task-in-progress",
            TaskStatus::Blocked => "task-blocked",
            TaskStatus::Done => "task-done",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub owner: Option<String>,
    pub status: TaskStatus,
    pub blocked_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    pub name: String,
    pub region: String,
    pub connected: bool,
    pub cpu_pct: u8,
    pub mem_pct: u8,
    pub rtt_ms: Option<u32>,
}

/// Fixed-size history of a metric, one sample per collector tick.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Series {
    pub samples: VecDeque<f64>,
}

impl Series {
    pub const CAP: usize = 120;

    pub fn push(&mut self, v: f64) {
        if self.samples.len() == Self::CAP {
            self.samples.pop_front();
        }
        self.samples.push_back(v);
    }

    pub fn last(&self) -> f64 {
        self.samples.back().copied().unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metrics {
    pub tokens_per_sec: Series,
    pub cost_per_hour: Series,
    pub tools_per_min: Series,
    pub msgs_per_min: Series,
    pub errors: Series,
    pub ctx_avg: Series,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub taken_at: Millis,
    pub hosts: Vec<Host>,
    pub agents: Vec<Agent>,
    /// Most recent messages, oldest first.
    pub messages: Vec<Message>,
    /// Most recent tool calls, oldest first.
    pub tools: Vec<ToolCall>,
    pub tasks: Vec<Task>,
    pub metrics: Metrics,
    /// Human-readable note about the data source (e.g. "demo", "~/.claude").
    pub source: String,
}

impl Snapshot {
    pub fn total_cost(&self) -> f64 {
        self.agents.iter().map(|a| a.cost_usd).sum()
    }

    pub fn agent(&self, id: &str) -> Option<&Agent> {
        self.agents.iter().find(|a| a.id == id)
    }

    /// Display name for an agent id (or the human operator).
    pub fn name_of<'a>(&'a self, id: &'a str) -> &'a str {
        if id == YOU {
            return "you";
        }
        self.agent(id).map(|a| a.name.as_str()).unwrap_or(id)
    }

    /// Agents ordered as a tree: each root followed by its descendants,
    /// as `(index into agents, depth)`.
    pub fn tree_order(&self) -> Vec<(usize, usize)> {
        fn walk(s: &Snapshot, parent: Option<&str>, depth: usize, out: &mut Vec<(usize, usize)>) {
            for (i, a) in s.agents.iter().enumerate() {
                if a.parent.as_deref() == parent && !out.iter().any(|(j, _)| *j == i) {
                    out.push((i, depth));
                    walk(s, Some(a.id.as_str()), depth + 1, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(self, None, 0, &mut out);
        // Agents whose parent is unknown are shown as roots.
        for (i, a) in self.agents.iter().enumerate() {
            if !out.iter().any(|(j, _)| *j == i) {
                out.push((i, 0));
                walk(self, Some(a.id.as_str()), 1, &mut out);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, parent: Option<&str>) -> Agent {
        Agent {
            id: id.into(),
            name: id.into(),
            host: "local".into(),
            project: String::new(),
            model: String::new(),
            parent: parent.map(Into::into),
            team: None,
            state: AgentState::Idle,
            tool: String::new(),
            now: String::new(),
            usage: Usage::default(),
            context_tokens: 50,
            context_window: 200,
            cost_usd: 0.0,
            started: 0,
            last_activity: 0,
            tool_calls: 0,
            errors: 0,
            git_branch: String::new(),
            files: vec![],
            transcript: vec![],
        }
    }

    #[test]
    fn tree_order_nests_children_after_parents() {
        let s = Snapshot {
            agents: vec![
                agent("b", Some("a")),
                agent("a", None),
                agent("c", Some("zz")),
            ],
            ..Default::default()
        };
        assert_eq!(s.tree_order(), vec![(1, 0), (0, 1), (2, 0)]);
    }

    #[test]
    fn usage_math() {
        let u = Usage {
            input: 10,
            output: 5,
            cache_read: 80,
            cache_write: 10,
        };
        assert_eq!(u.billed(), 25);
        assert_eq!(u.cache_hit_pct(), 80);
        assert_eq!(agent("a", None).context_pct(), 25);
    }
}
