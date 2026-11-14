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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AgentState {
    Working,
    Waiting,
    Done,
    #[default]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Agent {
    /// Unique id: the session id, or `session/agent-id` for subagents.
    pub id: String,
    pub name: String,
    pub host: String,
    pub project: String,
    /// Working directory of the session.
    #[serde(default)]
    pub cwd: String,
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
    /// The task it was given (the spawn description), when known.
    #[serde(default)]
    pub task: String,
    /// Readable role ("Developer"), when team rules know it.
    #[serde(default)]
    pub role: String,
    /// Display name of its team, when known.
    #[serde(default)]
    pub team_label: String,
    /// Starts and steers other agents.
    #[serde(default)]
    pub coordinator: bool,
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

    /// The group an agent belongs to: its Agent Teams team, else its project.
    pub fn team_key(&self) -> String {
        match &self.team {
            Some(t) if !t.is_empty() => t.clone(),
            _ => self.project.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MsgKind {
    /// A task handed to another agent.
    Task,
    /// A plain message or question.
    #[default]
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

/// A task without a leading run id such as `R-20261114-1538-setup-wizard`
/// (orchestrators prefix assignments with them; the detail view keeps it).
/// Empty when the task is nothing but such an id.
pub fn task_text(task: &str) -> &str {
    let t = task.trim();
    let token_end = t.find([' ', ':']).unwrap_or(t.len());
    let token = &t[..token_end];
    let digits = token.chars().filter(|c| c.is_ascii_digit()).count();
    let looks_like_id = token.len() >= 12 && digits >= 8 && token.contains('-');
    if looks_like_id {
        t[token_end..].trim_start_matches([' ', ':', '-', '·'])
    } else {
        t
    }
}

/// Id used for the human operator in messages.
pub const YOU: &str = "@you";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
    /// For approvals raised by `argus hook`: the request id to answer.
    #[serde(default)]
    pub request: Option<String>,
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
    /// Display names of teams: (team code, label).
    #[serde(default)]
    pub team_labels: Vec<(String, String)>,
    /// Problems worth showing to the user: (message id, argument).
    #[serde(default)]
    pub warnings: Vec<(String, String)>,
}

impl Snapshot {
    /// Display name of a team, if one is known.
    pub fn team_label(&self, team: &str) -> Option<&str> {
        self.team_labels
            .iter()
            .find(|(t, _)| t == team)
            .map(|(_, l)| l.as_str())
    }

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

    /// Team keys in a stable order (busiest first).
    pub fn teams(&self) -> Vec<String> {
        let mut out: Vec<(String, usize)> = Vec::new();
        for a in &self.agents {
            let k = a.team_key();
            match out.iter_mut().find(|(t, _)| *t == k) {
                Some((_, n)) => *n += 1,
                None => out.push((k, 1)),
            }
        }
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out.into_iter().map(|(t, _)| t).collect()
    }

    /// Host names in a stable order (busiest first).
    pub fn host_names(&self) -> Vec<String> {
        let mut out: Vec<(String, usize)> =
            self.hosts.iter().map(|h| (h.name.clone(), 0)).collect();
        for a in &self.agents {
            match out.iter_mut().find(|(h, _)| *h == a.host) {
                Some((_, n)) => *n += 1,
                None => out.push((a.host.clone(), 1)),
            }
        }
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out.into_iter().map(|(h, _)| h).collect()
    }

    /// Only the agents of `team`, their tools and tasks, and every message
    /// with at least one end in the team (cross-team messages included).
    pub fn filtered(&self, team: &str) -> Snapshot {
        self.scoped(Some(team), None)
    }

    /// Agents matching the optional team and host, with their tools, tasks and
    /// every message that has at least one end among them. Hosts are kept when
    /// they match the host filter and (with a team filter) run one of the agents.
    pub fn scoped(&self, team: Option<&str>, host: Option<&str>) -> Snapshot {
        let keep =
            |a: &Agent| team.is_none_or(|t| a.team_key() == t) && host.is_none_or(|h| a.host == h);
        let ids: Vec<&str> = self
            .agents
            .iter()
            .filter(|a| keep(a))
            .map(|a| a.id.as_str())
            .collect();
        let inside = |id: &str| ids.contains(&id);
        let hosts: Vec<&str> = self
            .agents
            .iter()
            .filter(|a| inside(&a.id))
            .map(|a| a.host.as_str())
            .collect();
        Snapshot {
            taken_at: self.taken_at,
            hosts: self
                .hosts
                .iter()
                .filter(|h| host.is_none_or(|x| h.name == x))
                .filter(|h| team.is_none() || hosts.contains(&h.name.as_str()))
                .cloned()
                .collect(),
            agents: self
                .agents
                .iter()
                .filter(|a| inside(&a.id))
                .cloned()
                .collect(),
            messages: self
                .messages
                .iter()
                .filter(|m| inside(&m.from) || inside(&m.to))
                .cloned()
                .collect(),
            tools: self
                .tools
                .iter()
                .filter(|t| inside(&t.agent))
                .cloned()
                .collect(),
            tasks: self
                .tasks
                .iter()
                .filter(|t| {
                    t.owner.as_deref().is_none_or(|o| {
                        inside(o) || self.agents.iter().any(|a| a.name == o && keep(a))
                    })
                })
                .cloned()
                .collect(),
            metrics: self.metrics.clone(),
            source: self.source.clone(),
            team_labels: self.team_labels.clone(),
            warnings: self.warnings.clone(),
        }
    }

    /// Removes finished agents, their tool calls and messages exchanged only among
    /// finished agents. Hosts are kept.
    pub fn hide_finished(&mut self) {
        let done: Vec<String> = self
            .agents
            .iter()
            .filter(|a| a.state == AgentState::Done)
            .map(|a| a.id.clone())
            .collect();
        if done.is_empty() {
            return;
        }
        let gone = |id: &str| done.iter().any(|d| d == id);
        self.agents.retain(|a| a.state != AgentState::Done);
        self.tools.retain(|t| !gone(&t.agent));
        self.messages
            .retain(|m| !(gone(&m.from) && (gone(&m.to) || m.to == YOU)));
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
            cwd: String::new(),
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
            ..Agent::default()
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
    fn run_ids_are_dropped_from_tasks() {
        assert_eq!(
            task_text("R-20261114-1538-kurulum-si liste adimi"),
            "liste adimi"
        );
        assert_eq!(task_text("R-20261114-1538-kurulum-si"), "");
        assert_eq!(
            task_text("R-20261114-1538-x · Gezinme testi"),
            "Gezinme testi"
        );
        assert_eq!(task_text("Fix the login form"), "Fix the login form");
    }

    #[test]
    fn scoping_and_hiding_finished() {
        let mut a = agent("a", None);
        a.host = "h1".into();
        let mut b = agent("b", Some("a"));
        b.host = "h2".into();
        b.state = AgentState::Done;
        let mut s = Snapshot {
            agents: vec![a, b],
            ..Default::default()
        };
        s.hosts = ["h1", "h2"]
            .iter()
            .map(|n| Host {
                name: n.to_string(),
                region: String::new(),
                connected: true,
                cpu_pct: 0,
                mem_pct: 0,
                rtt_ms: None,
            })
            .collect();
        let h2 = s.scoped(None, Some("h2"));
        assert_eq!(h2.agents.len(), 1);
        assert_eq!(h2.hosts.len(), 1);
        s.hide_finished();
        assert_eq!(s.agents.len(), 1);
        assert_eq!(s.hosts.len(), 2);
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
