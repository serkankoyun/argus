//! The Backlog view's model: every piece of work that waits, derived from a
//! [`Snapshot`] by one pure function, [`build`].
//!
//! Sources: Agent Teams tasks (or subagent assignments), orchestrator run
//! stages, open requests to the user and waiting agents, and `tasks/todo.md`
//! items. Work with an owner is grouped per owner as NOW → QUEUED → WAITING;
//! then come the items waiting on the user, then the unassigned pool. Each
//! waiting item also carries the chain of what it waits on, down to the root.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use crate::model::{
    task_text, AgentState, Millis, MsgKind, Snapshot, StageStatus, Task, TaskStatus, YOU,
};
use crate::source::todos::fold;

/// Items kept at most (in display order; the pool is cut first).
pub const MAX_QUEUE_ITEMS: usize = 1000;
/// Links followed at most when explaining why an item waits.
pub const MAX_CHAIN: usize = 8;
/// Longest description kept for a request.
const MAX_DETAIL: usize = 1200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lane {
    Now,
    Queued,
    Waiting,
}

/// Why an item is where it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    Working,
    /// Its turn in the owner's queue.
    Turn {
        pos: usize,
        of: usize,
    },
    /// Unfinished tasks it is blocked by.
    WaitsTasks(Vec<String>),
    /// The run stage before it.
    WaitsStage(String),
    Rejected {
        stage: String,
        decision: String,
    },
    Blocked(String),
    /// Another agent (by name).
    WaitsAgent(String),
    WaitsYou,
    Approval,
    Unassigned,
    /// Something a todo item names (`(waiting on: X)`).
    WaitsOn(String),
}

impl Reason {
    pub fn on_you(&self) -> bool {
        matches!(self, Reason::WaitsYou | Reason::Approval)
    }
}

/// Where an item comes from, for its source tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    Task(String),
    Run(String),
    Request,
    Agent,
    Todo(u32),
}

/// One step of the "why it waits" chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainLink {
    pub label: String,
    /// Key of the item it points at; `None` for something Argus does not see
    /// (an unknown task id, a todo marker) or for a cut (`…`).
    pub key: Option<String>,
    pub reason: Reason,
    pub since: Millis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
    /// Stable key that keeps the selection across refreshes.
    pub key: String,
    pub title: String,
    pub detail: String,
    pub owner_id: Option<String>,
    pub owner_label: String,
    pub team: String,
    pub lane: Lane,
    pub reason: Reason,
    pub since: Millis,
    /// Short tag, not translated (`task #18`, `todo.md:12`).
    pub source: String,
    pub origin: Origin,
    /// Full location: `path:line`, run id or task file.
    pub location: String,
    /// Run goal and stage, todo section and parent, or task group.
    pub context: String,
    /// Tasks it is blocked by: (id, title, status message id).
    pub blockers: Vec<(String, String, String)>,
    /// What it waits on, link by link; the last link is the root.
    pub chain: Vec<ChainLink>,
    /// Index of the root link in `chain`.
    pub root: Option<usize>,
    /// For items waiting on the user: how many items have it as their root.
    pub holds: usize,
}

impl QueueItem {
    pub fn root_link(&self) -> Option<&ChainLink> {
        self.root.and_then(|i| self.chain.get(i))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnerGroup {
    pub key: String,
    pub label: String,
    /// Team code.
    pub team: String,
    pub items: Vec<QueueItem>,
    /// Oldest `since` of its queued and waiting items (0 when none).
    pub longest_since: Millis,
    pub bottleneck: bool,
}

impl OwnerGroup {
    /// Queued and waiting items.
    pub fn pending(&self) -> usize {
        self.items.iter().filter(|i| i.lane != Lane::Now).count()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoolGroup {
    pub label: String,
    pub host: String,
    pub items: Vec<QueueItem>,
    /// Items the todo caps left out.
    pub omitted: u32,
    pub file: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Queue {
    pub owners: Vec<OwnerGroup>,
    pub you: Vec<QueueItem>,
    pub pool: Vec<PoolGroup>,
}

impl Queue {
    /// Every item in display order.
    pub fn items(&self) -> Vec<&QueueItem> {
        self.owners
            .iter()
            .flat_map(|g| &g.items)
            .chain(&self.you)
            .chain(self.pool.iter().flat_map(|p| &p.items))
            .collect()
    }

    pub fn find(&self, key: &str) -> Option<&QueueItem> {
        self.items().into_iter().find(|i| i.key == key)
    }

    pub fn is_empty(&self) -> bool {
        self.owners.is_empty() && self.you.is_empty() && self.pool.is_empty()
    }
}

/// What an item waits on, before chains are resolved.
enum Next {
    Item(String),
    End(String),
}

/// Where an item goes while building.
enum Place {
    Owner(String),
    You,
    Pool(String),
}

struct Builder<'a> {
    s: &'a Snapshot,
    owners: Vec<OwnerGroup>,
    you: Vec<QueueItem>,
    pool: Vec<PoolGroup>,
    next: HashMap<String, Next>,
    /// Key of each agent's own waiting item.
    agent_wait: HashMap<String, String>,
    /// Key of each agent's NOW item.
    agent_now: HashMap<String, String>,
    /// Key of each task's item.
    task_item: HashMap<String, String>,
    /// Keys of run stages shown by another item (the owner's task at work).
    alias: HashMap<String, String>,
}

/// "2" before "10"; other ids by text.
fn id_order(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        (Ok(_), Err(_)) => Ordering::Less,
        (Err(_), Ok(_)) => Ordering::Greater,
        _ => a.cmp(b),
    }
}

fn cap(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

/// Whether a todo marker names the user.
fn names_you(what: &str) -> bool {
    let f = fold(what);
    f.split(|c: char| !c.is_alphanumeric())
        .any(|w| matches!(w, "you" | "user" | "me" | "sen" | "senden" | "kullanici"))
        || f.contains("kullanicidan")
}

impl<'a> Builder<'a> {
    fn new(s: &'a Snapshot) -> Self {
        Self {
            s,
            owners: Vec::new(),
            you: Vec::new(),
            pool: Vec::new(),
            next: HashMap::new(),
            agent_wait: HashMap::new(),
            agent_now: HashMap::new(),
            task_item: HashMap::new(),
            alias: HashMap::new(),
        }
    }

    /// (agent id, label, team) of an owner written as an agent id or name.
    fn resolve(&self, owner: &str) -> (Option<String>, String, String) {
        match self
            .s
            .agents
            .iter()
            .find(|a| a.id == owner)
            .or_else(|| self.s.agents.iter().find(|a| a.name == owner))
        {
            Some(a) => (Some(a.id.clone()), a.name.clone(), a.team_key()),
            None => (None, owner.to_string(), String::new()),
        }
    }

    fn group_key(id: &Option<String>, label: &str) -> String {
        match id {
            Some(id) => format!("agent:{id}"),
            None => format!("name:{label}"),
        }
    }

    fn put(&mut self, place: Place, item: QueueItem, next: Option<Next>) {
        if let Some(n) = next {
            self.next.insert(item.key.clone(), n);
        }
        match place {
            Place::You => self.you.push(item),
            Place::Owner(key) => match self.owners.iter_mut().find(|g| g.key == key) {
                Some(g) => g.items.push(item),
                None => self.owners.push(OwnerGroup {
                    key,
                    label: item.owner_label.clone(),
                    team: item.team.clone(),
                    items: vec![item],
                    ..OwnerGroup::default()
                }),
            },
            Place::Pool(label) => match self.pool.iter_mut().find(|p| p.label == label) {
                Some(p) => p.items.push(item),
                None => self.pool.push(PoolGroup {
                    label,
                    items: vec![item],
                    ..PoolGroup::default()
                }),
            },
        }
    }

    fn tasks(&mut self) {
        let s = self.s;
        let by_id: HashMap<&str, &Task> = s.tasks.iter().map(|t| (t.id.as_str(), t)).collect();
        for t in s.tasks.iter().filter(|t| t.status != TaskStatus::Done) {
            let open: Vec<String> = t
                .blocked_by
                .iter()
                .filter(|b| {
                    by_id
                        .get(b.as_str())
                        .is_none_or(|x| x.status != TaskStatus::Done)
                })
                .cloned()
                .collect();
            let blockers = t
                .blocked_by
                .iter()
                .map(|b| match by_id.get(b.as_str()) {
                    Some(x) => (b.clone(), x.title.clone(), x.status.key().to_string()),
                    None => (b.clone(), String::new(), String::new()),
                })
                .collect();
            let key = format!("task:{}#{}", t.source, t.id);
            self.task_item.insert(t.id.clone(), key.clone());
            let group = t
                .source
                .rsplit(['/', '\\'])
                .nth(1)
                .unwrap_or(t.source.as_str())
                .to_string();
            let mut item = QueueItem {
                key,
                title: t.title.clone(),
                detail: t.detail.clone(),
                owner_id: None,
                owner_label: String::new(),
                team: String::new(),
                lane: Lane::Queued,
                reason: Reason::Unassigned,
                since: t.since,
                source: format!("task #{}", t.id),
                origin: Origin::Task(t.id.clone()),
                location: t.source.clone(),
                context: if group.is_empty() {
                    String::new()
                } else {
                    format!("tasks: {group}")
                },
                blockers,
                chain: Vec::new(),
                root: None,
                holds: 0,
            };
            let next = open.first().map(|b| Next::Item(format!("@task:{b}")));
            let Some(owner) = t.owner.as_deref().filter(|o| !o.is_empty()) else {
                if !open.is_empty() {
                    item.lane = Lane::Waiting;
                    item.reason = Reason::WaitsTasks(open);
                }
                self.put(Place::Pool(format!("tasks: {group}")), item, next);
                continue;
            };
            let (id, label, team) = self.resolve(owner);
            item.owner_id = id.clone();
            item.owner_label = label.clone();
            item.team = team;
            let gkey = Self::group_key(&id, &label);
            if t.status == TaskStatus::InProgress {
                item.lane = Lane::Now;
                item.reason = Reason::Working;
                if !t.active_form.is_empty() {
                    item.title = t.active_form.clone();
                    item.context = format!("#{} {} · {}", t.id, t.title, item.context)
                        .trim_end_matches(" · ")
                        .to_string();
                }
                if let Some(id) = &id {
                    self.agent_now.insert(id.clone(), item.key.clone());
                }
                // At work: nothing holds it, whatever its list says.
                self.put(Place::Owner(gkey), item, None);
            } else if !open.is_empty() {
                item.lane = Lane::Waiting;
                item.reason = Reason::WaitsTasks(open);
                self.put(Place::Owner(gkey), item, next);
            } else if t.status == TaskStatus::Blocked && t.blocked_by.is_empty() {
                // A subagent's assignment while it waits: why comes from the agent.
                item.lane = Lane::Waiting;
                item.reason = Reason::Blocked(String::new());
                if let Some(id) = &id {
                    self.agent_wait.insert(id.clone(), item.key.clone());
                }
                self.put(Place::Owner(gkey), item, None);
            } else {
                item.reason = Reason::Turn { pos: 0, of: 0 };
                self.put(Place::Owner(gkey), item, None);
            }
        }
    }

    fn runs(&mut self) {
        let s = self.s;
        for r in &s.runs {
            if !r.active() && r.current().is_none() {
                continue;
            }
            let team = r.team.clone().unwrap_or_default();
            let title = match r.team.as_deref() {
                Some(t) => r
                    .goal
                    .strip_prefix(t)
                    .and_then(|g| g.strip_prefix(':'))
                    .map(str::trim)
                    .filter(|g| !g.is_empty())
                    .unwrap_or(&r.goal),
                None => &r.goal,
            }
            .to_string();
            // Only the next pending stage is queued: later ones wait on it in
            // turn, and listing the whole pipeline of every run buries the rest.
            let planned = r.stages.iter().any(|st| !st.agent.is_empty());
            let mut queued = false;
            for (i, st) in r.stages.iter().enumerate() {
                if matches!(st.status, StageStatus::Done | StageStatus::Skipped) {
                    continue;
                }
                let key = format!("run:{}:{}", r.id, st.key);
                let role = if st.agent.is_empty() {
                    format!("stage-{}", st.key)
                } else {
                    st.agent.clone()
                };
                let mut item = QueueItem {
                    key: key.clone(),
                    title: title.clone(),
                    detail: r.goal.clone(),
                    owner_id: None,
                    owner_label: role.clone(),
                    team: team.clone(),
                    lane: Lane::Queued,
                    reason: Reason::Working,
                    since: st.since,
                    source: format!("run {}", r.id),
                    origin: Origin::Run(r.id.clone()),
                    location: r.id.clone(),
                    context: format!("{} · {}", r.id, st.key),
                    blockers: Vec::new(),
                    chain: Vec::new(),
                    root: None,
                    holds: 0,
                };
                let owner = st.owners.first().map(|o| self.resolve(o));
                // The stage's agent owns it when known, else its planned role.
                let owner_place = |item: &mut QueueItem| match &owner {
                    Some((id, label, t)) => {
                        item.owner_id = id.clone();
                        item.owner_label = label.clone();
                        if !t.is_empty() && item.team.is_empty() {
                            item.team = t.clone();
                        }
                        Self::group_key(id, label)
                    }
                    None => format!("role:{role}"),
                };
                match st.status {
                    StageStatus::Running | StageStatus::Waiting => {
                        item.lane = Lane::Now;
                        let gkey = owner_place(&mut item);
                        let shown = self
                            .owners
                            .iter()
                            .find(|g| g.key == gkey)
                            .and_then(|g| g.items.iter().find(|x| x.lane == Lane::Now))
                            .map(|x| x.key.clone());
                        if let Some(k) = shown {
                            self.alias.insert(key, k);
                            continue;
                        }
                        let next = (st.status == StageStatus::Waiting)
                            .then(|| item.owner_id.clone())
                            .flatten()
                            .map(|id| Next::Item(format!("@agent:{id}")));
                        if let Some(id) = &item.owner_id {
                            self.agent_now
                                .entry(id.clone())
                                .or_insert_with(|| key.clone());
                        }
                        self.put(Place::Owner(gkey), item, next);
                    }
                    StageStatus::Rejected | StageStatus::Blocked => {
                        item.lane = Lane::Waiting;
                        item.reason = if st.status == StageStatus::Rejected {
                            Reason::Rejected {
                                stage: st.key.clone(),
                                decision: st.decision.clone(),
                            }
                        } else {
                            Reason::Blocked(st.key.clone())
                        };
                        let gkey = owner_place(&mut item);
                        self.put(Place::Owner(gkey), item, None);
                    }
                    _ => {
                        if st.key == "merge" && planned && st.agent.is_empty() {
                            continue;
                        }
                        if queued {
                            continue;
                        }
                        queued = true;
                        // The latest earlier stage that is not finished.
                        let prev = r.stages[..i].iter().rev().find(|p| {
                            !matches!(p.status, StageStatus::Done | StageStatus::Skipped)
                        });
                        let next = match prev {
                            Some(p) => {
                                item.reason = Reason::WaitsStage(p.key.clone());
                                item.since = p.since;
                                Some(Next::Item(format!("run:{}:{}", r.id, p.key)))
                            }
                            None => {
                                item.reason = Reason::Turn { pos: 0, of: 0 };
                                None
                            }
                        };
                        if item.since == 0 {
                            item.since = r.created;
                        }
                        self.put(Place::Owner(format!("role:{role}")), item, next);
                    }
                }
            }
        }
    }

    fn requests_and_agents(&mut self) {
        let s = self.s;
        let done = |id: &str| s.agent(id).is_some_and(|a| a.state == AgentState::Done);
        let mut last_req: HashMap<String, String> = HashMap::new();
        for m in &s.messages {
            if m.answered_at.is_some()
                || m.to != YOU
                || !matches!(m.kind, MsgKind::Perm | MsgKind::Msg | MsgKind::Task)
                || done(&m.from)
            {
                continue;
            }
            let key = match &m.request {
                Some(r) => format!("req:{r}"),
                None => format!("req:{}+{}", m.from, m.ts),
            };
            last_req.insert(m.from.clone(), key.clone());
            let agent = s.agent(&m.from);
            self.put(
                Place::You,
                QueueItem {
                    key,
                    title: format!("{}: {}", s.name_of(&m.from), m.label),
                    detail: cap(&m.text, MAX_DETAIL),
                    owner_id: agent.map(|a| a.id.clone()),
                    owner_label: s.name_of(&m.from).to_string(),
                    team: agent.map(|a| a.team_key()).unwrap_or_default(),
                    lane: Lane::Waiting,
                    reason: if m.kind == MsgKind::Perm {
                        Reason::Approval
                    } else {
                        Reason::WaitsYou
                    },
                    since: m.ts,
                    source: "request".into(),
                    origin: Origin::Request,
                    location: m.request.clone().unwrap_or_default(),
                    context: String::new(),
                    blockers: Vec::new(),
                    chain: Vec::new(),
                    root: None,
                    holds: 0,
                },
                None,
            );
        }
        for a in &s.agents {
            if !matches!(a.state, AgentState::Waiting | AgentState::Stalled)
                || a.waits_on_own_work()
            {
                continue;
            }
            // What it waits on: its request to you, else its last unanswered
            // message to another agent.
            let (reason, next) = if let Some(k) = last_req.get(&a.id) {
                (Reason::WaitsYou, Next::Item(k.clone()))
            } else if let Some(m) = s
                .messages
                .iter()
                .rev()
                .find(|m| m.from == a.id && m.answered_at.is_none() && m.to != YOU)
            {
                (
                    Reason::WaitsAgent(s.name_of(&m.to).to_string()),
                    Next::Item(format!("@agent:{}", m.to)),
                )
            } else {
                continue;
            };
            if let Some(key) = self.agent_wait.get(&a.id).cloned() {
                if let Some(it) = self.item_mut(&key) {
                    it.reason = reason;
                }
                self.next.insert(key, next);
                continue;
            }
            let key = format!("agent:{}", a.id);
            self.agent_wait.insert(a.id.clone(), key.clone());
            let title = match task_text(&a.task) {
                "" => a.now.clone(),
                t => t.to_string(),
            };
            self.put(
                Place::Owner(format!("agent:{}", a.id)),
                QueueItem {
                    key,
                    title,
                    detail: cap(&a.task, MAX_DETAIL),
                    owner_id: Some(a.id.clone()),
                    owner_label: a.name.clone(),
                    team: a.team_key(),
                    lane: Lane::Waiting,
                    reason,
                    since: a.last_activity,
                    source: "agent".into(),
                    origin: Origin::Agent,
                    location: a.cwd.clone(),
                    context: a.run.clone(),
                    blockers: Vec::new(),
                    chain: Vec::new(),
                    root: None,
                    holds: 0,
                },
                Some(next),
            );
        }
    }

    fn todos(&mut self) {
        let s = self.s;
        let mut files: Vec<_> = s.todos.iter().collect();
        files.sort_by(|a, b| a.project.cmp(&b.project).then(a.path.cmp(&b.path)));
        for f in files {
            let label = if f.host.is_empty() {
                f.project.clone()
            } else {
                format!("{} @{}", f.project, f.host)
            };
            for it in &f.items {
                let context = if it.parent.is_empty() {
                    it.section.clone()
                } else if it.section.is_empty() {
                    it.parent.clone()
                } else {
                    format!("{} › {}", it.section, it.parent)
                };
                let mut item = QueueItem {
                    key: format!("todo:{}:{}", f.path, it.line),
                    title: it.title.clone(),
                    detail: it.detail.clone(),
                    owner_id: None,
                    owner_label: String::new(),
                    team: String::new(),
                    lane: Lane::Queued,
                    reason: Reason::Unassigned,
                    since: f.modified,
                    source: format!("todo.md:{}", it.line),
                    origin: Origin::Todo(it.line),
                    location: format!("{}:{}", f.path, it.line),
                    context,
                    blockers: Vec::new(),
                    chain: Vec::new(),
                    root: None,
                    holds: 0,
                };
                if it.waits_on_you || (!it.waits_on.is_empty() && names_you(&it.waits_on)) {
                    item.lane = Lane::Waiting;
                    item.reason = Reason::WaitsYou;
                    self.put(Place::You, item, None);
                    continue;
                }
                let mut next = None;
                if !it.waits_on.is_empty() {
                    item.lane = Lane::Waiting;
                    item.reason = Reason::WaitsOn(it.waits_on.clone());
                    let id = it.waits_on.trim().trim_start_matches('#');
                    next = Some(
                        if it.waits_on.trim().starts_with('#') && s.tasks.iter().any(|t| t.id == id)
                        {
                            Next::Item(format!("@task:{id}"))
                        } else {
                            Next::End(it.waits_on.clone())
                        },
                    );
                }
                self.put(Place::Pool(label.clone()), item, next);
            }
            if let Some(p) = self.pool.iter_mut().find(|p| p.label == label) {
                p.host = f.host.clone();
                p.omitted += f.omitted;
                p.file = f.path.clone();
            } else if f.omitted > 0 {
                self.pool.push(PoolGroup {
                    label,
                    host: f.host.clone(),
                    items: Vec::new(),
                    omitted: f.omitted,
                    file: f.path.clone(),
                });
            }
        }
    }

    fn item_mut(&mut self, key: &str) -> Option<&mut QueueItem> {
        self.owners
            .iter_mut()
            .flat_map(|g| g.items.iter_mut())
            .chain(self.you.iter_mut())
            .chain(self.pool.iter_mut().flat_map(|p| p.items.iter_mut()))
            .find(|i| i.key == key)
    }

    /// The item key an indirect target (`@task:<id>`, `@agent:<id>`) stands for.
    fn target(&self, k: &str) -> Option<String> {
        if let Some(id) = k.strip_prefix("@task:") {
            self.task_item.get(id).cloned()
        } else if let Some(id) = k.strip_prefix("@agent:") {
            self.agent_wait
                .get(id)
                .or_else(|| self.agent_now.get(id))
                .cloned()
        } else {
            Some(self.alias.get(k).cloned().unwrap_or_else(|| k.to_string()))
        }
    }

    fn chains(&mut self) {
        let all: HashMap<String, (String, Reason, Millis, Lane)> = self
            .owners
            .iter()
            .flat_map(|g| &g.items)
            .chain(&self.you)
            .chain(self.pool.iter().flat_map(|p| &p.items))
            .map(|i| {
                let label = if i.owner_label.is_empty() || i.origin == Origin::Request {
                    i.title.clone()
                } else {
                    format!("{}: {}", i.owner_label, i.title)
                };
                (i.key.clone(), (label, i.reason.clone(), i.since, i.lane))
            })
            .collect();
        let s = self.s;
        let mut chains: Vec<(String, Vec<ChainLink>)> = Vec::new();
        for start in all.keys() {
            let mut chain = Vec::new();
            let mut seen: HashSet<String> = HashSet::from([start.clone()]);
            let mut cur = start.clone();
            while let Some(n) = self.next.get(&cur) {
                if chain.len() >= MAX_CHAIN {
                    chain.push(cut_link());
                    break;
                }
                let k = match n {
                    Next::End(label) => {
                        chain.push(ChainLink {
                            label: label.clone(),
                            key: None,
                            reason: Reason::WaitsOn(label.clone()),
                            since: 0,
                        });
                        break;
                    }
                    Next::Item(k) => k,
                };
                let Some(key) = self.target(k).filter(|k| all.contains_key(k)) else {
                    // Something Argus does not see: an unknown task or agent.
                    let label = if let Some(id) = k.strip_prefix("@task:") {
                        format!("#{id}")
                    } else if let Some(id) = k.strip_prefix("@agent:") {
                        s.name_of(id).to_string()
                    } else if let Some(rest) = k.strip_prefix("run:") {
                        rest.rsplit_once(':')
                            .map(|(run, st)| format!("{run} · {st}"))
                            .unwrap_or_else(|| rest.to_string())
                    } else {
                        k.clone()
                    };
                    chain.push(ChainLink {
                        label: label.clone(),
                        key: None,
                        reason: Reason::WaitsOn(label),
                        since: 0,
                    });
                    break;
                };
                if !seen.insert(key.clone()) {
                    chain.push(cut_link());
                    break;
                }
                let (label, reason, since, _) = &all[&key];
                chain.push(ChainLink {
                    label: label.clone(),
                    key: Some(key.clone()),
                    reason: reason.clone(),
                    since: *since,
                });
                cur = key;
            }
            if !chain.is_empty() {
                chains.push((start.clone(), chain));
            }
        }
        let mut holds: HashMap<String, usize> = HashMap::new();
        for (start, chain) in chains {
            if let Some(k) = chain.last().and_then(|l| l.key.clone()) {
                if k != start {
                    *holds.entry(k).or_default() += 1;
                }
            }
            if let Some(it) = self.item_mut(&start) {
                it.root = Some(chain.len() - 1);
                it.chain = chain;
            }
        }
        for it in &mut self.you {
            it.holds = holds.get(&it.key).copied().unwrap_or(0);
        }
    }

    fn order(&mut self) {
        let s = self.s;
        for g in &mut self.owners {
            // Queue positions among the items whose turn it is.
            let mut turn: Vec<&mut QueueItem> = g
                .items
                .iter_mut()
                .filter(|i| matches!(i.reason, Reason::Turn { .. }))
                .collect();
            turn.sort_by(|a, b| turn_order(a, b, s));
            let of = turn.len();
            for (k, it) in turn.into_iter().enumerate() {
                it.reason = Reason::Turn { pos: k + 1, of };
            }
            g.items.sort_by(|a, b| {
                a.lane.cmp(&b.lane).then_with(|| match a.lane {
                    Lane::Now => a.since.cmp(&b.since),
                    Lane::Queued => queued_order(a, b, s),
                    Lane::Waiting => a.since.cmp(&b.since),
                })
            });
            g.longest_since = g
                .items
                .iter()
                .filter(|i| i.lane != Lane::Now && i.since > 0)
                .map(|i| i.since)
                .min()
                .unwrap_or(0);
        }
        // Bottleneck: the most loaded group of its team holds two or more items
        // while another agent of that team is idle.
        let mut most: HashMap<String, usize> = HashMap::new();
        for g in &self.owners {
            let m = most.entry(g.team.clone()).or_default();
            *m = (*m).max(g.pending());
        }
        for g in &mut self.owners {
            let n = g.pending();
            let owner = g.key.strip_prefix("agent:");
            let idle_mate = s.agents.iter().any(|a| {
                a.state == AgentState::Idle
                    && !g.team.is_empty()
                    && a.team_key() == g.team
                    && Some(a.id.as_str()) != owner
            });
            g.bottleneck = n >= 2 && most.get(&g.team) == Some(&n) && idle_mate;
        }
        self.owners.sort_by(|a, b| {
            b.bottleneck
                .cmp(&a.bottleneck)
                .then(b.pending().cmp(&a.pending()))
                .then_with(|| since_or_max(a.longest_since).cmp(&since_or_max(b.longest_since)))
                .then(a.label.cmp(&b.label))
        });
        self.you.sort_by(|a, b| {
            b.holds
                .cmp(&a.holds)
                .then((a.origin != Origin::Request).cmp(&(b.origin != Origin::Request)))
                .then(a.since.cmp(&b.since))
        });
        // Todo projects by name (kept in file order inside), task groups last.
        self.pool.sort_by(|a, b| {
            a.label
                .starts_with("tasks: ")
                .cmp(&b.label.starts_with("tasks: "))
                .then(a.label.cmp(&b.label))
        });
    }

    fn cap(&mut self) {
        let mut left = MAX_QUEUE_ITEMS;
        let mut take = |items: &mut Vec<QueueItem>| {
            items.truncate(left);
            left -= items.len();
        };
        for g in &mut self.owners {
            take(&mut g.items);
        }
        self.owners.retain(|g| !g.items.is_empty());
        take(&mut self.you);
        for p in &mut self.pool {
            let before = p.items.len();
            take(&mut p.items);
            p.omitted += (before - p.items.len()) as u32;
        }
    }
}

fn cut_link() -> ChainLink {
    ChainLink {
        label: "…".into(),
        key: None,
        reason: Reason::Unassigned,
        since: 0,
    }
}

fn since_or_max(t: Millis) -> Millis {
    if t > 0 {
        t
    } else {
        Millis::MAX
    }
}

/// Order of items whose turn it is: tasks by id, run stages by run creation
/// then stage order.
fn turn_order(a: &QueueItem, b: &QueueItem, s: &Snapshot) -> Ordering {
    match (&a.origin, &b.origin) {
        (Origin::Task(x), Origin::Task(y)) => id_order(x, y).then(a.since.cmp(&b.since)),
        (Origin::Run(_), Origin::Run(_)) => run_order(a, b, s),
        (Origin::Task(_), _) => Ordering::Less,
        (_, Origin::Task(_)) => Ordering::Greater,
        _ => a.since.cmp(&b.since),
    }
}

fn run_order(a: &QueueItem, b: &QueueItem, s: &Snapshot) -> Ordering {
    let pos = |i: &QueueItem| {
        let (run, stage) = i.context.split_once(" · ").unwrap_or((&i.context, ""));
        let created = s
            .runs
            .iter()
            .find(|r| r.id == run)
            .map(|r| r.created)
            .unwrap_or(0);
        let k = crate::model::STAGES
            .iter()
            .position(|x| *x == stage)
            .unwrap_or(99);
        (created, run.to_string(), k)
    };
    pos(a).cmp(&pos(b))
}

fn queued_order(a: &QueueItem, b: &QueueItem, s: &Snapshot) -> Ordering {
    let p = |i: &QueueItem| match i.reason {
        Reason::Turn { pos, .. } => pos,
        _ => usize::MAX,
    };
    p(a).cmp(&p(b)).then_with(|| match (&a.origin, &b.origin) {
        (Origin::Run(_), Origin::Run(_)) => run_order(a, b, s),
        _ => a.since.cmp(&b.since),
    })
}

/// The Backlog of a snapshot (already filtered to the chosen team or host).
pub fn build(s: &Snapshot) -> Queue {
    let mut b = Builder::new(s);
    b.tasks();
    b.runs();
    b.requests_and_agents();
    b.todos();
    b.chains();
    b.order();
    b.cap();
    Queue {
        owners: b.owners,
        you: b.you,
        pool: b.pool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Agent, Message, Run, RunStage, TodoFile, TodoItem, STAGES};

    const NOW: Millis = 1_800_000_000_000;

    fn agent(id: &str, team: &str, state: AgentState) -> Agent {
        Agent {
            id: id.into(),
            name: id.into(),
            team: Some(team.into()),
            state,
            ..Agent::default()
        }
    }

    fn task(id: &str, owner: Option<&str>, status: TaskStatus, deps: &[&str]) -> Task {
        Task {
            id: id.into(),
            title: format!("task {id}"),
            owner: owner.map(Into::into),
            status,
            blocked_by: deps.iter().map(|d| d.to_string()).collect(),
            source: format!("~/.claude/tasks/grp/{id}.json"),
            since: NOW - 60_000,
            ..Task::default()
        }
    }

    fn group<'a>(q: &'a Queue, label: &str) -> &'a OwnerGroup {
        q.owners
            .iter()
            .find(|g| g.label == label)
            .unwrap_or_else(|| panic!("no group {label}: {q:#?}"))
    }

    fn item<'a>(q: &'a Queue, key: &str) -> &'a QueueItem {
        q.find(key).unwrap_or_else(|| panic!("no item {key}"))
    }

    #[test]
    fn agent_teams_tasks_become_now_queued_waiting() {
        let s = Snapshot {
            taken_at: NOW,
            agents: vec![agent("x", "t", AgentState::Working)],
            tasks: vec![
                task("10", Some("x"), TaskStatus::Pending, &[]),
                task("2", Some("x"), TaskStatus::Pending, &[]),
                task("5", Some("x"), TaskStatus::Blocked, &["3"]),
                task("7", Some("x"), TaskStatus::InProgress, &[]),
                task("8", Some("x"), TaskStatus::Done, &[]),
            ],
            ..Default::default()
        };
        let q = build(&s);
        let g = group(&q, "x");
        let rows: Vec<(&str, Lane, &Reason)> = g
            .items
            .iter()
            .map(|i| (i.source.as_str(), i.lane, &i.reason))
            .collect();
        assert_eq!(
            rows,
            [
                ("task #7", Lane::Now, &Reason::Working),
                ("task #2", Lane::Queued, &Reason::Turn { pos: 1, of: 2 }),
                ("task #10", Lane::Queued, &Reason::Turn { pos: 2, of: 2 }),
                (
                    "task #5",
                    Lane::Waiting,
                    &Reason::WaitsTasks(vec!["3".into()])
                ),
            ]
        );
        let w = &g.items[3];
        assert_eq!(w.blockers, vec![("3".into(), String::new(), String::new())]);
        assert_eq!(w.owner_id.as_deref(), Some("x"));
        assert!(q.find("task:~/.claude/tasks/grp/8.json#8").is_none());
        // A blocker that is done no longer holds the task.
        let mut s2 = s.clone();
        s2.tasks.push(task("3", None, TaskStatus::Done, &[]));
        let q2 = build(&s2);
        let w = item(&q2, "task:~/.claude/tasks/grp/5.json#5");
        assert!(matches!(w.reason, Reason::Turn { .. }));
        assert_eq!(w.blockers[0].2, "task-done");
    }

    #[test]
    fn unowned_tasks_go_to_the_pool() {
        let s = Snapshot {
            tasks: vec![
                task("1", None, TaskStatus::Pending, &[]),
                task("2", None, TaskStatus::Pending, &["1"]),
            ],
            ..Default::default()
        };
        let q = build(&s);
        assert!(q.owners.is_empty());
        assert_eq!(q.pool.len(), 1);
        assert_eq!(q.pool[0].label, "tasks: grp");
        assert_eq!(q.pool[0].items[0].reason, Reason::Unassigned);
        assert_eq!(
            q.pool[0].items[1].reason,
            Reason::WaitsTasks(vec!["1".into()])
        );
    }

    fn run(stages: &[(&str, StageStatus, &str, &str)]) -> Run {
        Run {
            id: "R-20261116-1000-shop-x".into(),
            goal: "shop: add the export".into(),
            team: Some("shop".into()),
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

    #[test]
    fn run_test_stage_waits_for_dev() {
        use StageStatus::*;
        let s = Snapshot {
            taken_at: NOW,
            agents: vec![agent("dev-a", "shop", AgentState::Working)],
            runs: vec![run(&[
                ("dev", Running, "orchestra-developer", "dev-a"),
                ("test", Pending, "orchestra-test-writer", ""),
                ("qa", Rejected, "orchestra-qa-reviewer", "rev"),
            ])],
            ..Default::default()
        };
        let q = build(&s);
        let t = item(&q, "run:R-20261116-1000-shop-x:test");
        assert_eq!(t.lane, Lane::Queued);
        assert_eq!(t.owner_label, "orchestra-test-writer");
        assert_eq!(t.reason, Reason::WaitsStage("dev".into()));
        assert_eq!(t.title, "add the export");
        assert_eq!(t.context, "R-20261116-1000-shop-x · test");
        let d = item(&q, "run:R-20261116-1000-shop-x:dev");
        assert_eq!(d.lane, Lane::Now);
        assert_eq!(d.owner_id.as_deref(), Some("dev-a"));
        let r = item(&q, "run:R-20261116-1000-shop-x:qa");
        assert_eq!(r.lane, Lane::Waiting);
        assert_eq!(
            r.reason,
            Reason::Rejected {
                stage: "qa".into(),
                decision: String::new()
            }
        );
        assert!(q.find("run:R-20261116-1000-shop-x:security").is_none());
        // A finished run adds nothing.
        let s = Snapshot {
            runs: vec![run(&[("dev", Done, "", "")])],
            ..Default::default()
        };
        assert!(build(&s).is_empty());
    }

    fn perm(from: &str, ts: Millis, answered: bool) -> Message {
        Message {
            ts,
            from: from.into(),
            to: YOU.into(),
            kind: MsgKind::Perm,
            label: "Bash: deploy".into(),
            text: "May I deploy?".into(),
            answered_at: answered.then_some(ts + 1),
            request: Some(format!("r{ts}")),
        }
    }

    #[test]
    fn open_approval_is_waiting_on_you() {
        let s = Snapshot {
            taken_at: NOW,
            agents: vec![agent("db", "t", AgentState::Waiting)],
            messages: vec![
                perm("db", NOW - 900_000, true),
                perm("db", NOW - 300_000, false),
            ],
            ..Default::default()
        };
        let q = build(&s);
        assert_eq!(q.you.len(), 1);
        let a = &q.you[0];
        assert_eq!(a.reason, Reason::Approval);
        assert_eq!(a.since, NOW - 300_000);
        assert_eq!(a.title, "db: Bash: deploy");
        assert_eq!(a.detail, "May I deploy?");
        let g = group(&q, "db");
        assert_eq!(g.items[0].reason, Reason::WaitsYou);
        assert_eq!(g.items[0].lane, Lane::Waiting);
    }

    #[test]
    fn agent_waiting_on_agent() {
        let mut bg = agent("bg", "t", AgentState::Waiting);
        bg.now = "[bg 3m] npm test".into();
        let s = Snapshot {
            agents: vec![
                agent("a", "t", AgentState::Waiting),
                agent("b", "t", AgentState::Working),
                bg,
            ],
            messages: vec![Message {
                ts: 5,
                from: "a".into(),
                to: "b".into(),
                kind: MsgKind::Msg,
                label: "is the schema ready?".into(),
                ..Message::default()
            }],
            ..Default::default()
        };
        let q = build(&s);
        let a = item(&q, "agent:a");
        assert_eq!(a.reason, Reason::WaitsAgent("b".into()));
        assert!(q.find("agent:bg").is_none());
    }

    fn todo_file(project: &str, items: Vec<TodoItem>, omitted: u32) -> TodoFile {
        TodoFile {
            path: format!("/w/{project}/tasks/todo.md"),
            root: format!("/w/{project}"),
            project: project.into(),
            host: String::new(),
            modified: NOW - 7_200_000,
            items,
            omitted,
        }
    }

    fn todo(line: u32, title: &str, you: bool) -> TodoItem {
        TodoItem {
            line,
            title: title.into(),
            section: "Backlog".into(),
            waits_on_you: you,
            ..TodoItem::default()
        }
    }

    #[test]
    fn todos_feed_pool_and_you() {
        let s = Snapshot {
            todos: vec![
                todo_file("zeta", vec![todo(3, "z one", false)], 0),
                todo_file(
                    "alpha",
                    vec![
                        todo(4, "a one", false),
                        todo(9, "pick", true),
                        todo(12, "a two", false),
                    ],
                    5,
                ),
            ],
            ..Default::default()
        };
        let q = build(&s);
        assert_eq!(q.pool.len(), 2);
        assert_eq!(q.pool[0].label, "alpha");
        assert_eq!(q.pool[0].omitted, 5);
        assert_eq!(q.pool[0].file, "/w/alpha/tasks/todo.md");
        let titles: Vec<&str> = q.pool[0].items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, ["a one", "a two"]);
        assert_eq!(q.you.len(), 1);
        let y = &q.you[0];
        assert_eq!(y.reason, Reason::WaitsYou);
        assert_eq!(y.location, "/w/alpha/tasks/todo.md:9");
        assert_eq!(y.source, "todo.md:9");
        assert_eq!(y.since, NOW - 7_200_000);
    }

    #[test]
    fn todo_wait_reason_and_link() {
        let mut waits = todo(2, "ship", false);
        waits.waits_on = "#4".into();
        let mut legal = todo(3, "publish", false);
        legal.waits_on = "legal review".into();
        let mut mine = todo(5, "name it", false);
        mine.waits_on = "kullanıcı kararı".into();
        let s = Snapshot {
            agents: vec![agent("x", "t", AgentState::Working)],
            tasks: vec![task("4", Some("x"), TaskStatus::InProgress, &[])],
            todos: vec![todo_file("p", vec![waits, legal, mine], 0)],
            ..Default::default()
        };
        let q = build(&s);
        let w = item(&q, "todo:/w/p/tasks/todo.md:2");
        assert_eq!(w.reason, Reason::WaitsOn("#4".into()));
        assert_eq!(w.lane, Lane::Waiting);
        assert_eq!(w.chain.len(), 1);
        assert_eq!(
            w.chain[0].key.as_deref(),
            Some("task:~/.claude/tasks/grp/4.json#4")
        );
        let l = item(&q, "todo:/w/p/tasks/todo.md:3");
        assert_eq!(l.chain[0].key, None);
        assert_eq!(l.chain[0].label, "legal review");
        assert_eq!(q.you.len(), 1);
        assert_eq!(q.you[0].title, "name it");
    }

    #[test]
    fn chain_follows_tasks_stages_and_agents_to_root() {
        use StageStatus::*;
        // test waits for dev, dev's agent waits for b,
        // b waits for the user's approval.
        let mut s = Snapshot {
            taken_at: NOW,
            agents: vec![
                agent("dev-a", "shop", AgentState::Waiting),
                agent("b", "shop", AgentState::Waiting),
            ],
            runs: vec![run(&[
                ("dev", Waiting, "orchestra-developer", "dev-a"),
                ("test", Pending, "orchestra-test-writer", ""),
                ("qa", Pending, "orchestra-qa-reviewer", ""),
            ])],
            messages: vec![
                Message {
                    ts: NOW - 50_000,
                    from: "dev-a".into(),
                    to: "b".into(),
                    kind: MsgKind::Msg,
                    label: "need the keys".into(),
                    ..Message::default()
                },
                perm("b", NOW - 40_000, false),
            ],
            ..Default::default()
        };
        // And a task chain: 9 waits for 8, 8 waits for 7 (in progress).
        s.agents.push(agent("x", "t", AgentState::Working));
        s.tasks = vec![
            task("7", Some("x"), TaskStatus::InProgress, &[]),
            task("8", Some("x"), TaskStatus::Pending, &["7"]),
            task("9", Some("x"), TaskStatus::Pending, &["8"]),
        ];
        let q = build(&s);
        // Only the next stage is queued; later ones wait on it in turn.
        assert!(q.find("run:R-20261116-1000-shop-x:qa").is_none());
        let qa = item(&q, "run:R-20261116-1000-shop-x:test");
        let keys: Vec<Option<&str>> = qa.chain.iter().map(|l| l.key.as_deref()).collect();
        assert_eq!(
            keys,
            [
                Some("run:R-20261116-1000-shop-x:dev"),
                Some("agent:dev-a"),
                Some("agent:b"),
                Some(format!("req:r{}", NOW - 40_000).as_str()),
            ]
        );
        assert_eq!(qa.root, Some(3));
        assert!(qa.root_link().unwrap().reason.on_you());
        let nine = item(&q, "task:~/.claude/tasks/grp/9.json#9");
        let labels: Vec<&str> = nine.chain.iter().map(|l| l.label.as_str()).collect();
        assert_eq!(labels, ["x: task 8", "x: task 7"]);
        assert_eq!(nine.root_link().unwrap().reason, Reason::Working);
    }

    #[test]
    fn chain_is_cycle_safe_and_capped() {
        let s = Snapshot {
            agents: vec![agent("x", "t", AgentState::Working)],
            tasks: vec![
                task("1", Some("x"), TaskStatus::Pending, &["2"]),
                task("2", Some("x"), TaskStatus::Pending, &["1"]),
            ],
            ..Default::default()
        };
        let q = build(&s);
        let one = item(&q, "task:~/.claude/tasks/grp/1.json#1");
        assert_eq!(one.chain.len(), 2);
        assert_eq!(one.chain[1].label, "…");
        // A long line of tasks is cut after MAX_CHAIN links.
        let mut tasks = Vec::new();
        for k in 0..20 {
            let dep = (k + 1).to_string();
            tasks.push(task(
                &k.to_string(),
                Some("x"),
                TaskStatus::Pending,
                &[&dep],
            ));
        }
        let s = Snapshot {
            agents: s.agents.clone(),
            tasks,
            ..Default::default()
        };
        let q = build(&s);
        let first = item(&q, "task:~/.claude/tasks/grp/0.json#0");
        assert_eq!(first.chain.len(), MAX_CHAIN + 1);
        assert_eq!(first.chain[MAX_CHAIN].label, "…");
        // The unknown task at the end is named by its id.
        let last = item(&q, "task:~/.claude/tasks/grp/19.json#19");
        assert_eq!(last.chain[0].label, "#20");
    }

    #[test]
    fn approval_holds_count() {
        let s = Snapshot {
            taken_at: NOW,
            agents: vec![
                agent("a", "t", AgentState::Waiting),
                agent("b", "t", AgentState::Waiting),
                agent("c", "t", AgentState::Waiting),
            ],
            messages: vec![
                perm("a", NOW - 10_000, false),
                perm("b", NOW - 90_000, false),
                Message {
                    ts: NOW - 5_000,
                    from: "c".into(),
                    to: "a".into(),
                    kind: MsgKind::Msg,
                    label: "ready?".into(),
                    ..Message::default()
                },
            ],
            ..Default::default()
        };
        let q = build(&s);
        // a's approval holds a's own row and c (which waits for a).
        assert_eq!(q.you[0].key, format!("req:r{}", NOW - 10_000));
        assert_eq!(q.you[0].holds, 2);
        assert_eq!(q.you[1].holds, 1);
    }

    #[test]
    fn bottleneck_and_caps() {
        let mut s = Snapshot {
            agents: vec![
                agent("busy", "t", AgentState::Working),
                agent("free", "t", AgentState::Idle),
            ],
            ..Default::default()
        };
        s.tasks = (0..3)
            .map(|k| task(&k.to_string(), Some("busy"), TaskStatus::Pending, &[]))
            .collect();
        let q = build(&s);
        assert!(group(&q, "busy").bottleneck);
        s.todos = vec![todo_file(
            "big",
            (0..1500).map(|k| todo(k, "x", false)).collect(),
            0,
        )];
        let q = build(&s);
        assert_eq!(q.items().len(), MAX_QUEUE_ITEMS);
        assert_eq!(q.pool[0].omitted, 1500 + 3 - MAX_QUEUE_ITEMS as u32);
    }

    #[test]
    fn every_item_is_descriptive() {
        let s = crate::source::Source::poll(&mut crate::source::demo::DemoSource::new("en"));
        let q = build(&s);
        assert!(q.items().len() >= 10, "{}", q.items().len());
        for i in q.items() {
            assert!(!i.title.is_empty(), "{i:?}");
            assert!(!i.source.is_empty(), "{i:?}");
            assert!(!i.location.is_empty(), "{i:?}");
            assert!(i.since > 0, "{i:?}");
        }
        assert!(!q.you.is_empty());
        assert!(!q.pool.is_empty());
        assert!(q
            .owners
            .iter()
            .any(|g| g.items.iter().any(|i| i.lane == Lane::Queued)));
    }
}
