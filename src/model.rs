//! Core data model shared by every view. The collector (v0.1+) will produce
//! these from `~/.claude`; for now `Snapshot::demo()` feeds the UI.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone)]
pub struct Agent {
    pub name: String,
    pub host: String,
    /// Name of the parent agent, if this is a subagent / teammate.
    pub parent: Option<String>,
    pub state: AgentState,
    pub tool: String,
    pub tokens: u64,
    /// Context window usage, 0–100.
    pub context_pct: u8,
    pub cost_usd: f64,
    pub elapsed_secs: u64,
}

#[derive(Debug, Clone)]
pub struct Host {
    pub name: String,
    pub region: String,
    pub connected: bool,
    pub cpu_pct: u8,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub hosts: Vec<Host>,
    pub agents: Vec<Agent>,
}

impl Snapshot {
    pub fn demo() -> Self {
        let a = |name: &str,
                 host: &str,
                 parent: Option<&str>,
                 state: AgentState,
                 tool: &str,
                 tokens: u64,
                 ctx: u8,
                 cost: f64,
                 secs: u64| Agent {
            name: name.into(),
            host: host.into(),
            parent: parent.map(Into::into),
            state,
            tool: tool.into(),
            tokens,
            context_pct: ctx,
            cost_usd: cost,
            elapsed_secs: secs,
        };
        use AgentState::*;
        Snapshot {
            hosts: vec![
                Host {
                    name: "prod-1".into(),
                    region: "eu-central".into(),
                    connected: true,
                    cpu_pct: 58,
                },
                Host {
                    name: "gpu-2".into(),
                    region: "eu-west".into(),
                    connected: true,
                    cpu_pct: 81,
                },
                Host {
                    name: "dev-3".into(),
                    region: "tr-ist".into(),
                    connected: false,
                    cpu_pct: 17,
                },
            ],
            agents: vec![
                a(
                    "lead", "prod-1", None, Working, "Task", 84_200, 74, 3.12, 2460,
                ),
                a(
                    "api-dev",
                    "prod-1",
                    Some("lead"),
                    Working,
                    "Edit",
                    38_900,
                    61,
                    1.47,
                    1680,
                ),
                a(
                    "db-dev",
                    "prod-1",
                    Some("lead"),
                    Waiting,
                    "Bash",
                    22_100,
                    38,
                    0.81,
                    1680,
                ),
                a(
                    "test-runner",
                    "prod-1",
                    Some("lead"),
                    Working,
                    "Bash",
                    12_600,
                    22,
                    0.44,
                    540,
                ),
                a(
                    "reviewer", "gpu-2", None, Working, "Read", 51_300, 66, 2.05, 1980,
                ),
                a(
                    "sec-audit",
                    "gpu-2",
                    Some("reviewer"),
                    Working,
                    "Grep",
                    19_400,
                    35,
                    0.73,
                    840,
                ),
                a(
                    "perf-probe",
                    "gpu-2",
                    Some("reviewer"),
                    Done,
                    "—",
                    9_800,
                    18,
                    0.36,
                    360,
                ),
                a(
                    "docs-writer",
                    "gpu-2",
                    None,
                    Idle,
                    "—",
                    30_000,
                    47,
                    1.10,
                    3120,
                ),
                a(
                    "migrator", "dev-3", None, Stalled, "WebFetch", 61_700, 89, 2.66, 4320,
                ),
            ],
        }
    }

    pub fn total_cost(&self) -> f64 {
        self.agents.iter().map(|a| a.cost_usd).sum()
    }
}
