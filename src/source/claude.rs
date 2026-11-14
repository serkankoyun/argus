//! Live source: reads Claude Code's local data under `~/.claude`.
//!
//! - `projects/<project>/<session>.jsonl`: one transcript per session
//! - `projects/<project>/<session>/subagents/agent-<id>.jsonl`: subagents
//! - `teams/<team>/inboxes/<member>.json`: Agent Teams messages
//! - `tasks/<group>/<id>.json`: Agent Teams task lists
//!
//! Transcripts are read incrementally (only new bytes on each poll) and parsed
//! leniently: unknown fields are ignored, so format changes degrade gracefully.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::pricing;
use super::system::SystemProbe;
use super::{actions, hook, Control, Notice, Source};
use crate::model::*;

/// Sessions untouched for longer than this are ignored.
const HORIZON: Duration = Duration::from_secs(24 * 3600);
const MAX_MESSAGES: usize = 300;
const MAX_TOOLS: usize = 400;
const TRANSCRIPT_LINES: usize = 40;

/// Resolves the Claude configuration directory.
pub fn default_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join(".claude")
}

pub struct ClaudeSource {
    root: PathBuf,
    files: HashMap<PathBuf, Tail>,
    probe: SystemProbe,
    metrics: Metrics,
    last_poll: Option<(Millis, u64, f64)>,
    host_name: String,
}

impl ClaudeSource {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            files: HashMap::new(),
            probe: SystemProbe::new(),
            metrics: Metrics::default(),
            last_poll: None,
            host_name: String::new(),
        }
    }

    fn discover(&self) -> Vec<(PathBuf, Option<String>)> {
        let mut out = Vec::new();
        let Ok(projects) = std::fs::read_dir(self.root.join("projects")) else {
            return out;
        };
        for project in projects.flatten() {
            let pdir = project.path();
            let Ok(entries) = std::fs::read_dir(&pdir) else {
                continue;
            };
            for e in entries.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "jsonl") && recent(&p) {
                    out.push((p, None));
                } else if p.is_dir() {
                    let session = p.file_name().map(|s| s.to_string_lossy().into_owned());
                    if let Ok(subs) = std::fs::read_dir(p.join("subagents")) {
                        for s in subs.flatten() {
                            let sp = s.path();
                            if sp.extension().is_some_and(|x| x == "jsonl") && recent(&sp) {
                                out.push((sp, session.clone()));
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

fn recent(p: &Path) -> bool {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < HORIZON)
}

impl Source for ClaudeSource {
    fn poll(&mut self) -> Snapshot {
        let now = now_millis();
        for (path, parent) in self.discover() {
            let tail = self
                .files
                .entry(path.clone())
                .or_insert_with(|| Tail::new(&path, parent));
            tail.read_new();
        }
        let host = self.probe.sample();
        let mut snap = build_snapshot(
            self.files.values().map(|t| &t.session),
            &host.name,
            &host.claude_cwds,
            now,
        );
        merge_teams(&self.root, &mut snap);
        apply_requests(&mut snap, &hook::pending(&self.root));
        self.host_name = host.name.clone();
        snap.tasks = read_tasks(&self.root);
        snap.hosts = vec![Host {
            name: host.name.clone(),
            region: "local".into(),
            connected: true,
            cpu_pct: host.cpu_pct,
            mem_pct: host.mem_pct,
            rtt_ms: None,
        }];
        update_metrics(&mut self.metrics, &mut self.last_poll, &snap, now);
        snap.metrics = self.metrics.clone();
        snap.source = self.root.display().to_string();
        snap.taken_at = now;
        snap
    }

    fn owns(&self, agent: &str) -> bool {
        self.files.values().any(|t| t.session.id == agent)
    }

    fn host(&self) -> Option<String> {
        Some(self.host_name.clone())
    }

    fn control(&mut self, c: &Control) -> Notice {
        match c {
            Control::Decide { request, allow, .. } => hook::decide(&self.root, request, *allow)
                .map(|_| {
                    if *allow {
                        "approved".to_string()
                    } else {
                        "rejected".to_string()
                    }
                })
                .map_err(|e| e.to_string()),
            Control::Run { dir, prompt, .. } => actions::run_local(dir, &["-p", prompt]),
            Control::Resume { agent, text } => {
                let Some(t) = self.files.values().find(|t| &t.session.id == agent) else {
                    return Err("unknown agent".into());
                };
                if t.session.parent.is_some() {
                    return Err("subagents cannot be resumed; message their parent".into());
                }
                let cwd = t.session.cwd.clone();
                actions::run_local(&cwd, &["-p", "--resume", agent, text])
            }
            Control::AddHost(_) => Err("not supported by this data source".into()),
        }
    }
}

/// Replaces heuristic approval guesses with real `argus hook` requests.
pub fn apply_requests(snap: &mut Snapshot, pending: &[hook::Request]) {
    if pending.is_empty() {
        return;
    }
    snap.messages
        .retain(|m| !(m.kind == MsgKind::Perm && pending.iter().any(|r| r.session_id == m.from)));
    for r in pending {
        if let Some(a) = snap.agents.iter_mut().find(|a| a.id == r.session_id) {
            a.state = AgentState::Waiting;
            a.tool = r.tool.clone();
            a.now = r.target.clone();
        }
        snap.messages.push(Message {
            ts: r.ts,
            from: r.session_id.clone(),
            to: YOU.into(),
            kind: MsgKind::Perm,
            label: short(&format!("{} {}", r.tool, r.target), 40),
            text: format!("{} {}", r.tool, r.target),
            answered_at: None,
            request: Some(r.id.clone()),
        });
    }
    snap.messages.sort_by_key(|m| m.ts);
}

/// Incrementally read transcript file.
struct Tail {
    path: PathBuf,
    offset: u64,
    partial: Vec<u8>,
    session: Session,
}

impl Tail {
    fn new(path: &Path, parent_session: Option<String>) -> Self {
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let project = path
            .ancestors()
            .find(|a| a.parent().is_some_and(|p| p.ends_with("projects")))
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let session = match parent_session {
            Some(parent) => {
                let agent_id = stem.trim_start_matches("agent-").to_string();
                Session::new(format!("{parent}/{agent_id}"), Some(parent), project)
            }
            None => Session::new(stem, None, project),
        };
        Self {
            path: path.to_path_buf(),
            offset: 0,
            partial: Vec::new(),
            session,
        }
    }

    fn read_new(&mut self) {
        let Ok(mut f) = File::open(&self.path) else {
            return;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // Truncated or rewritten: start over.
            let (id, parent, project) = (
                self.session.id.clone(),
                self.session.parent.clone(),
                self.session.project.clone(),
            );
            self.session = Session::new(id, parent, project);
            self.offset = 0;
            self.partial.clear();
        }
        if len == self.offset || f.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if f.read_to_end(&mut buf).is_err() {
            return;
        }
        self.offset += buf.len() as u64;
        self.partial.extend_from_slice(&buf);
        let mut start = 0;
        while let Some(pos) = self.partial[start..].iter().position(|b| *b == b'\n') {
            let line = &self.partial[start..start + pos];
            if let Ok(v) = serde_json::from_slice::<Value>(line) {
                self.session.ingest(&v);
            }
            start += pos + 1;
        }
        self.partial.drain(..start);
    }
}

#[derive(Debug, Clone)]
struct Pending {
    name: String,
    target: String,
    ts: Millis,
}

#[derive(Debug, Clone)]
struct Spawn {
    tool_use_id: String,
    ts: Millis,
    description: String,
    agent_type: String,
    prompt: String,
    result: Option<(Millis, String)>,
}

#[derive(Debug, Clone)]
struct Sent {
    ts: Millis,
    to: String,
    text: String,
    summary: String,
}

/// Everything learned from one transcript.
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub parent: Option<String>,
    pub project: String,
    cwd: String,
    branch: String,
    model: String,
    agent_name: Option<String>,
    team: Option<String>,
    first_prompt: String,
    usage: Usage,
    cost: f64,
    context: u64,
    started: Millis,
    last: Millis,
    last_kind: LastKind,
    pending: HashMap<String, Pending>,
    last_tool: (String, String),
    tool_calls: u32,
    errors: u32,
    tools: Vec<ToolCall>,
    files: Vec<(String, i64, i64)>,
    transcript: Vec<(Millis, String, String)>,
    spawns: Vec<Spawn>,
    sent: Vec<Sent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LastKind {
    None,
    User,
    ToolResult,
    AssistantTool,
    AssistantEnd,
}

impl Session {
    fn new(id: String, parent: Option<String>, project: String) -> Self {
        Self {
            id,
            parent,
            project,
            cwd: String::new(),
            branch: String::new(),
            model: String::new(),
            agent_name: None,
            team: None,
            first_prompt: String::new(),
            usage: Usage::default(),
            cost: 0.0,
            context: 0,
            started: 0,
            last: 0,
            last_kind: LastKind::None,
            pending: HashMap::new(),
            last_tool: (String::new(), String::new()),
            tool_calls: 0,
            errors: 0,
            tools: Vec::new(),
            files: Vec::new(),
            transcript: Vec::new(),
            spawns: Vec::new(),
            sent: Vec::new(),
        }
    }

    fn note(&mut self, ts: Millis, role: &str, text: &str) {
        let text: String = text.trim().chars().take(400).collect();
        if text.is_empty() {
            return;
        }
        self.transcript.push((ts, role.to_string(), text));
        if self.transcript.len() > TRANSCRIPT_LINES {
            self.transcript.remove(0);
        }
    }

    pub fn ingest(&mut self, v: &Value) {
        let ts = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.timestamp_millis());
        let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
        if kind != "user" && kind != "assistant" {
            return;
        }
        let ts = ts.unwrap_or(self.last);
        if self.started == 0 {
            self.started = ts;
        }
        self.last = self.last.max(ts);
        for (field, slot) in [("cwd", &mut self.cwd), ("gitBranch", &mut self.branch)] {
            if let Some(s) = v.get(field).and_then(Value::as_str) {
                if !s.is_empty() {
                    *slot = s.to_string();
                }
            }
        }
        if let Some(n) = v.get("agentName").and_then(Value::as_str) {
            self.agent_name = Some(n.to_string());
        }
        if let Some(n) = v.get("teamName").and_then(Value::as_str) {
            self.team = Some(n.to_string());
        }
        let Some(msg) = v.get("message") else { return };
        match kind {
            "assistant" => self.ingest_assistant(msg, ts),
            _ => self.ingest_user(msg, ts),
        }
    }

    fn ingest_assistant(&mut self, msg: &Value, ts: Millis) {
        if let Some(m) = msg.get("model").and_then(Value::as_str) {
            if !m.starts_with('<') {
                self.model = m.to_string();
            }
        }
        if let Some(u) = msg.get("usage") {
            let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
            let turn = Usage {
                input: n("input_tokens"),
                output: n("output_tokens"),
                cache_read: n("cache_read_input_tokens"),
                cache_write: n("cache_creation_input_tokens"),
            };
            self.cost += pricing::cost(&self.model, &turn);
            self.usage.add(&turn);
            let ctx = turn.input + turn.cache_read + turn.cache_write;
            if ctx > 0 {
                self.context = ctx + turn.output;
            }
        }
        let mut used_tool = false;
        for item in content_items(msg) {
            match item.get("type").and_then(Value::as_str) {
                Some("text") => {
                    let t = item.get("text").and_then(Value::as_str).unwrap_or("");
                    self.note(ts, "assistant", t);
                }
                Some("tool_use") => {
                    used_tool = true;
                    self.tool_use(item, ts);
                }
                _ => {}
            }
        }
        self.last_kind = if used_tool {
            LastKind::AssistantTool
        } else {
            LastKind::AssistantEnd
        };
    }

    fn tool_use(&mut self, item: &Value, ts: Millis) {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let name = item
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("?")
            .to_string();
        let input = item.get("input").cloned().unwrap_or(Value::Null);
        let target = tool_target(&name, &input);
        self.tool_calls += 1;
        self.last_tool = (name.clone(), target.clone());
        self.note(ts, "tool", &format!("{name} {target}"));
        track_files(&mut self.files, &name, &input);
        self.tools.push(ToolCall {
            ts,
            agent: self.id.clone(),
            tool: name.clone(),
            target: target.clone(),
            duration_ms: None,
            ok: None,
        });
        if self.tools.len() > MAX_TOOLS {
            self.tools.remove(0);
        }
        let s = |k: &str| {
            input
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        match name.as_str() {
            "Task" | "Agent" => self.spawns.push(Spawn {
                tool_use_id: id.clone(),
                ts,
                description: s("description"),
                agent_type: s("subagent_type"),
                prompt: s("prompt"),
                result: None,
            }),
            "SendMessage" => {
                let to = first_of(&input, &["recipient", "to", "target", "teammate"]);
                let text = first_of(&input, &["message", "content", "text"]);
                self.sent.push(Sent {
                    ts,
                    to,
                    summary: s("summary"),
                    text,
                });
            }
            _ => {}
        }
        self.pending.insert(id, Pending { name, target, ts });
    }

    fn ingest_user(&mut self, msg: &Value, ts: Millis) {
        if let Some(text) = msg.get("content").and_then(Value::as_str) {
            if self.first_prompt.is_empty() {
                self.first_prompt = text.chars().take(2000).collect();
            }
            self.note(ts, "user", text);
            self.last_kind = LastKind::User;
            return;
        }
        let mut saw_result = false;
        for item in content_items(msg) {
            match item.get("type").and_then(Value::as_str) {
                Some("tool_result") => {
                    saw_result = true;
                    let id = item
                        .get("tool_use_id")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let is_err = item
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    if is_err {
                        self.errors += 1;
                    }
                    let text = result_text(item);
                    if let Some(p) = self.pending.remove(id) {
                        let dur = ts - p.ts;
                        if let Some(t) = self
                            .tools
                            .iter_mut()
                            .rev()
                            .find(|t| t.ts == p.ts && t.tool == p.name)
                        {
                            t.duration_ms = Some(dur);
                            t.ok = Some(!is_err);
                        }
                        if is_err {
                            self.note(
                                ts,
                                "error",
                                &format!("{} failed: {}", p.name, first_line(&text)),
                            );
                        }
                    }
                    if let Some(sp) = self.spawns.iter_mut().find(|s| s.tool_use_id == id) {
                        sp.result = Some((ts, text));
                    }
                }
                Some("text") => {
                    let t = item.get("text").and_then(Value::as_str).unwrap_or("");
                    if self.first_prompt.is_empty() {
                        self.first_prompt = t.chars().take(2000).collect();
                    }
                    self.note(ts, "user", t);
                }
                _ => {}
            }
        }
        self.last_kind = if saw_result {
            LastKind::ToolResult
        } else {
            LastKind::User
        };
    }
}

fn content_items(msg: &Value) -> impl Iterator<Item = &Value> {
    msg.get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn first_of(v: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn result_text(item: &Value) -> String {
    match item.get("content") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|p| p.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(120).collect()
}

fn short(s: &str, n: usize) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() > n {
        let mut t: String = one.chars().take(n.saturating_sub(1)).collect();
        t.push('…');
        t
    } else {
        one
    }
}

/// A short, human-readable target for a tool call.
pub fn tool_target(name: &str, input: &Value) -> String {
    let get = |k: &str| input.get(k).and_then(Value::as_str);
    let path = get("file_path")
        .or_else(|| get("notebook_path"))
        .or_else(|| get("path"));
    let raw = match name {
        "Bash" => get("command").map(str::to_string),
        "Grep" => get("pattern").map(|p| match path {
            Some(dir) => format!("\"{p}\" in {dir}"),
            None => format!("\"{p}\""),
        }),
        "Glob" => get("pattern").map(str::to_string),
        "WebFetch" => get("url").map(str::to_string),
        "WebSearch" => get("query").map(str::to_string),
        "Task" | "Agent" => get("description").map(str::to_string),
        "SendMessage" => Some(format!(
            "→ {}",
            first_of(input, &["recipient", "to", "target"])
        )),
        _ => path.map(str::to_string),
    };
    let raw = raw
        .or_else(|| path.map(str::to_string))
        .or_else(|| get("description").map(str::to_string))
        .unwrap_or_default();
    short(&raw, 72)
}

fn track_files(files: &mut Vec<(String, i64, i64)>, name: &str, input: &Value) {
    let lines = |k: &str| {
        input
            .get(k)
            .and_then(Value::as_str)
            .map(|s| s.lines().count() as i64)
            .unwrap_or(0)
    };
    let path = match input.get("file_path").and_then(Value::as_str) {
        Some(p) => p.to_string(),
        None => return,
    };
    let (add, del) = match name {
        "Edit" => (lines("new_string"), lines("old_string")),
        "Write" => (lines("content"), 0),
        "MultiEdit" => input
            .get("edits")
            .and_then(Value::as_array)
            .map(|edits| {
                edits.iter().fold((0, 0), |(a, d), e| {
                    let l = |k: &str| {
                        e.get(k)
                            .and_then(Value::as_str)
                            .map(|s| s.lines().count() as i64)
                            .unwrap_or(0)
                    };
                    (a + l("new_string"), d + l("old_string"))
                })
            })
            .unwrap_or((0, 0)),
        _ => return,
    };
    if let Some(f) = files.iter_mut().find(|f| f.0 == path) {
        f.1 += add;
        f.2 += del;
    } else {
        files.push((path, add, del));
    }
}

/// Turns parsed sessions into a snapshot. Pure function, unit-tested below.
pub fn build_snapshot<'a>(
    sessions: impl Iterator<Item = &'a Session>,
    host: &str,
    live_cwds: &[PathBuf],
    now: Millis,
) -> Snapshot {
    let sessions: Vec<&Session> = sessions.filter(|s| s.started > 0).collect();
    let mut snap = Snapshot::default();

    for s in &sessions {
        let alive = live_cwds.iter().any(|c| Path::new(&s.cwd) == c.as_path());
        let state = infer_state(s, alive, now, &sessions);
        let (tool, target) = match s.pending.values().max_by_key(|p| p.ts) {
            Some(p) => (p.name.clone(), p.target.clone()),
            None => s.last_tool.clone(),
        };
        snap.agents.push(Agent {
            id: s.id.clone(),
            name: agent_name(s, &sessions),
            host: host.to_string(),
            project: project_name(s),
            cwd: s.cwd.clone(),
            model: s.model.clone(),
            parent: s.parent.clone(),
            team: s.team.clone(),
            state,
            tool,
            now: target,
            usage: s.usage,
            context_tokens: s.context,
            context_window: pricing::context_window(&s.model, s.context),
            cost_usd: s.cost,
            started: s.started,
            last_activity: s.last,
            tool_calls: s.tool_calls,
            errors: s.errors,
            git_branch: s.branch.clone(),
            files: s.files.clone(),
            transcript: s.transcript.clone(),
        });
        snap.tools.extend(s.tools.iter().cloned());
    }

    // Messages: task hand-offs, results, SendMessage calls, approvals.
    for s in &sessions {
        for sp in &s.spawns {
            let child = sessions
                .iter()
                .find(|c| {
                    c.parent.as_deref() == Some(s.id.as_str())
                        && prompt_matches(&c.first_prompt, &sp.prompt)
                })
                .map(|c| c.id.clone())
                .unwrap_or_else(|| format!("{}/{}", s.id, sp.tool_use_id));
            snap.messages.push(Message {
                ts: sp.ts,
                from: s.id.clone(),
                to: child.clone(),
                kind: MsgKind::Task,
                label: short(&sp.description, 40),
                text: short(&sp.prompt, 600),
                answered_at: sp.result.as_ref().map(|r| r.0),
                request: None,
            });
            if let Some((rts, text)) = &sp.result {
                snap.messages.push(Message {
                    ts: *rts,
                    from: child,
                    to: s.id.clone(),
                    kind: MsgKind::Result,
                    label: short(&first_line(text), 40),
                    text: short(text, 600),
                    answered_at: None,
                    request: None,
                });
            }
        }
        for m in &s.sent {
            let to = sessions
                .iter()
                .find(|c| agent_name(c, &sessions) == m.to)
                .map(|c| c.id.clone())
                .unwrap_or_else(|| m.to.clone());
            let label = if m.summary.is_empty() {
                short(&m.text, 40)
            } else {
                short(&m.summary, 40)
            };
            snap.messages.push(Message {
                ts: m.ts,
                from: s.id.clone(),
                to,
                kind: MsgKind::Msg,
                label,
                text: short(&m.text, 600),
                answered_at: None,
                request: None,
            });
        }
        let alive = live_cwds.iter().any(|c| Path::new(&s.cwd) == c.as_path());
        if let Some(p) = s.pending.values().max_by_key(|p| p.ts) {
            if alive && now - p.ts > 20_000 && now - s.last > 20_000 && !is_long_running(&p.name) {
                snap.messages.push(Message {
                    ts: p.ts,
                    from: s.id.clone(),
                    to: YOU.into(),
                    kind: MsgKind::Perm,
                    label: short(&format!("{} {}", p.name, p.target), 40),
                    text: format!("{} {}", p.name, p.target),
                    answered_at: None,
                    request: None,
                });
            }
        }
    }
    snap.messages.sort_by_key(|m| m.ts);
    if snap.messages.len() > MAX_MESSAGES {
        snap.messages.drain(..snap.messages.len() - MAX_MESSAGES);
    }
    snap.tools.sort_by_key(|t| t.ts);
    if snap.tools.len() > MAX_TOOLS {
        snap.tools.drain(..snap.tools.len() - MAX_TOOLS);
    }
    // Stable ordering: most recently active roots first.
    snap.agents
        .sort_by_key(|a| std::cmp::Reverse(a.last_activity));
    snap
}

fn is_long_running(tool: &str) -> bool {
    matches!(tool, "Task" | "Agent" | "WebFetch" | "WebSearch")
}

fn prompt_matches(first_prompt: &str, prompt: &str) -> bool {
    let a: String = first_prompt
        .split_whitespace()
        .take(30)
        .collect::<Vec<_>>()
        .join(" ");
    let b: String = prompt
        .split_whitespace()
        .take(30)
        .collect::<Vec<_>>()
        .join(" ");
    !a.is_empty() && !b.is_empty() && (a.starts_with(&b) || b.starts_with(&a))
}

fn project_name(s: &Session) -> String {
    Path::new(&s.cwd)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| s.project.clone())
}

fn agent_name(s: &Session, all: &[&Session]) -> String {
    if let Some(n) = &s.agent_name {
        return n.clone();
    }
    if let Some(parent) = &s.parent {
        if let Some(p) = all.iter().find(|p| &p.id == parent) {
            if let Some(sp) = p
                .spawns
                .iter()
                .find(|sp| prompt_matches(&s.first_prompt, &sp.prompt))
            {
                let base = if sp.agent_type.is_empty() || sp.agent_type == "general-purpose" {
                    &sp.description
                } else {
                    &sp.agent_type
                };
                return short(&base.to_lowercase().replace(' ', "-"), 18);
            }
        }
        let id = s.id.rsplit('/').next().unwrap_or("");
        return format!("agent-{}", &id[..id.len().min(6)]);
    }
    let short_id = &s.id[..s.id.len().min(4)];
    format!("{}#{short_id}", project_name(s))
}

fn infer_state(s: &Session, alive: bool, now: Millis, all: &[&Session]) -> AgentState {
    let age = now - s.last;
    // A subagent is done once its parent received the result.
    if let Some(parent) = &s.parent {
        if let Some(p) = all.iter().find(|p| &p.id == parent) {
            if p.spawns
                .iter()
                .any(|sp| sp.result.is_some() && prompt_matches(&s.first_prompt, &sp.prompt))
            {
                return AgentState::Done;
            }
        }
    }
    let live = alive || age < 5 * 60_000;
    if !live {
        return AgentState::Done;
    }
    if let Some(p) = s.pending.values().max_by_key(|p| p.ts) {
        let waited = now - p.ts;
        if waited > 10 * 60_000 && !is_long_running(&p.name) {
            return AgentState::Stalled;
        }
        if waited > 20_000 && age > 20_000 && !is_long_running(&p.name) {
            return AgentState::Waiting;
        }
        return AgentState::Working;
    }
    match s.last_kind {
        LastKind::User | LastKind::ToolResult if age < 120_000 => AgentState::Working,
        LastKind::AssistantEnd | LastKind::None => AgentState::Idle,
        _ if age > 10 * 60_000 => AgentState::Stalled,
        _ => AgentState::Idle,
    }
}

/// Adds Agent Teams inbox messages.
fn merge_teams(root: &Path, snap: &mut Snapshot) {
    let Ok(teams) = std::fs::read_dir(root.join("teams")) else {
        return;
    };
    for team in teams.flatten() {
        let Ok(inboxes) = std::fs::read_dir(team.path().join("inboxes")) else {
            continue;
        };
        for inbox in inboxes.flatten() {
            let p = inbox.path();
            if !recent(&p) {
                continue;
            }
            let to_name = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            for it in items {
                let from_name = first_of(&it, &["from", "sender"]);
                let body = first_of(&it, &["text", "message", "content"]);
                let summary = first_of(&it, &["summary"]);
                let ts = it
                    .get("timestamp")
                    .and_then(Value::as_str)
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                    .map(|d| d.timestamp_millis())
                    .unwrap_or(snap.taken_at);
                let id_of = |name: &str| {
                    snap.agents
                        .iter()
                        .find(|a| a.name == name)
                        .map(|a| a.id.clone())
                        .unwrap_or_else(|| name.to_string())
                };
                let msg = Message {
                    ts,
                    from: id_of(&from_name),
                    to: id_of(&to_name),
                    kind: MsgKind::Msg,
                    label: short(if summary.is_empty() { &body } else { &summary }, 40),
                    text: short(&body, 600),
                    answered_at: None,
                    request: None,
                };
                let dup = snap
                    .messages
                    .iter()
                    .any(|m| m.from == msg.from && m.to == msg.to && (m.ts - msg.ts).abs() < 5_000);
                if !dup {
                    snap.messages.push(msg);
                }
            }
        }
    }
    snap.messages.sort_by_key(|m| m.ts);
}

/// Reads Agent Teams task files.
fn read_tasks(root: &Path) -> Vec<Task> {
    let mut out = Vec::new();
    let Ok(groups) = std::fs::read_dir(root.join("tasks")) else {
        return out;
    };
    for g in groups.flatten() {
        let Ok(files) = std::fs::read_dir(g.path()) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().is_none_or(|x| x != "json") || !recent(&p) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Ok(v) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let id = match v.get("id") {
                Some(Value::String(s)) => s.clone(),
                Some(Value::Number(n)) => n.to_string(),
                _ => p
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            };
            let blocked_by: Vec<String> = v
                .get("blockedBy")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|x| {
                            x.as_str()
                                .map(str::to_string)
                                .unwrap_or_else(|| x.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default();
            let status = match v.get("status").and_then(Value::as_str).unwrap_or("") {
                "completed" | "done" => TaskStatus::Done,
                "in_progress" | "in-progress" => TaskStatus::InProgress,
                _ if !blocked_by.is_empty() => TaskStatus::Blocked,
                _ => TaskStatus::Pending,
            };
            out.push(Task {
                id,
                title: first_of(&v, &["subject", "title", "description"]),
                owner: v.get("owner").and_then(Value::as_str).map(str::to_string),
                status,
                blocked_by,
            });
        }
    }
    out
}

pub fn update_metrics(
    m: &mut Metrics,
    last: &mut Option<(Millis, u64, f64)>,
    s: &Snapshot,
    now: Millis,
) {
    let tokens: u64 = s.agents.iter().map(|a| a.tokens()).sum();
    let cost = s.total_cost();
    let (tps, cph) = match *last {
        Some((t0, tok0, cost0)) if now > t0 => {
            let dt = (now - t0) as f64 / 1000.0;
            (
                tokens.saturating_sub(tok0) as f64 / dt,
                (cost - cost0).max(0.0) / dt * 3600.0,
            )
        }
        _ => (0.0, 0.0),
    };
    *last = Some((now, tokens, cost));
    let minute = now - 60_000;
    let active: Vec<&Agent> = s
        .agents
        .iter()
        .filter(|a| a.state != AgentState::Done)
        .collect();
    let ctx = if active.is_empty() {
        0.0
    } else {
        active.iter().map(|a| a.context_pct() as f64).sum::<f64>() / active.len() as f64
    };
    m.tokens_per_sec.push(tps);
    m.cost_per_hour.push(cph);
    m.tools_per_min
        .push(s.tools.iter().filter(|t| t.ts > minute).count() as f64);
    m.msgs_per_min
        .push(s.messages.iter().filter(|x| x.ts > minute).count() as f64);
    m.errors.push(
        s.tools
            .iter()
            .filter(|t| t.ts > minute && t.ok == Some(false))
            .count() as f64,
    );
    m.ctx_avg.push(ctx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ts(sec: i64) -> String {
        chrono::DateTime::from_timestamp(1_800_000_000 + sec, 0)
            .unwrap()
            .to_rfc3339()
    }
    const T0: Millis = 1_800_000_000_000;

    fn lead() -> Session {
        let mut s = Session::new("sess-1".into(), None, "-home-me-shop".into());
        s.ingest(
            &json!({"type":"user","cwd":"/home/me/shop","gitBranch":"main","timestamp":ts(0),
            "message":{"role":"user","content":"Add rate limiting to /login"}}),
        );
        s.ingest(&json!({"type":"assistant","timestamp":ts(2),"message":{"role":"assistant","model":"claude-sonnet-5-5",
            "usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":1000,"cache_creation_input_tokens":200},
            "content":[{"type":"text","text":"I'll delegate the schema check."},
                       {"type":"tool_use","id":"t1","name":"Task","input":{"description":"Check users schema","subagent_type":"db-dev","prompt":"Is users.email unique? Check migrations."}}]}}));
        s
    }

    fn child() -> Session {
        let mut c = Session::new(
            "sess-1/abc123".into(),
            Some("sess-1".into()),
            "-home-me-shop".into(),
        );
        c.ingest(
            &json!({"type":"user","cwd":"/home/me/shop","timestamp":ts(3),
            "message":{"role":"user","content":"Is users.email unique? Check migrations."}}),
        );
        c.ingest(&json!({"type":"assistant","timestamp":ts(4),"message":{"role":"assistant","model":"claude-haiku-5-5",
            "usage":{"input_tokens":10,"output_tokens":5},
            "content":[{"type":"tool_use","id":"r1","name":"Read","input":{"file_path":"db/migrations/0042.sql"}}]}}));
        c
    }

    #[test]
    fn parses_usage_tools_and_transcript() {
        let s = lead();
        assert_eq!(s.usage.billed(), 350);
        assert_eq!(s.context, 1350);
        assert_eq!(s.tool_calls, 1);
        assert_eq!(s.model, "claude-sonnet-5-5");
        assert_eq!(s.spawns.len(), 1);
        assert!(s.pending.contains_key("t1"));
        assert_eq!(s.transcript.len(), 3);
    }

    #[test]
    fn links_subagents_and_builds_messages() {
        let (l, c) = (lead(), child());
        let snap = build_snapshot([&l, &c].into_iter(), "box", &[], T0 + 10_000);
        let sub = snap.agent("sess-1/abc123").expect("child agent");
        assert_eq!(sub.name, "db-dev");
        assert_eq!(sub.parent.as_deref(), Some("sess-1"));
        assert_eq!(sub.tool, "Read");
        assert_eq!(sub.state, AgentState::Working);
        let task = snap
            .messages
            .iter()
            .find(|m| m.kind == MsgKind::Task)
            .expect("task msg");
        assert_eq!(task.to, "sess-1/abc123");
        assert_eq!(task.label, "Check users schema");
        assert_eq!(snap.agent("sess-1").unwrap().name, "shop#sess");
    }

    #[test]
    fn results_complete_subagents() {
        let (mut l, c) = (lead(), child());
        l.ingest(&json!({"type":"user","timestamp":ts(30),"message":{"role":"user","content":[
            {"type":"tool_result","tool_use_id":"t1","content":"Yes, migration 0042 adds a unique index."}]}}));
        let snap = build_snapshot([&l, &c].into_iter(), "box", &[], T0 + 31_000);
        assert_eq!(snap.agent("sess-1/abc123").unwrap().state, AgentState::Done);
        let r = snap
            .messages
            .iter()
            .find(|m| m.kind == MsgKind::Result)
            .expect("result msg");
        assert_eq!(r.from, "sess-1/abc123");
        assert!(r.label.starts_with("Yes, migration"));
    }

    #[test]
    fn long_pending_tool_means_waiting_for_approval() {
        let mut c = child();
        c.cwd = "/home/me/shop".into();
        let live = [PathBuf::from("/home/me/shop")];
        let snap = build_snapshot([&c].into_iter(), "box", &live, T0 + 60_000);
        assert_eq!(snap.agents[0].state, AgentState::Waiting);
        assert!(snap
            .messages
            .iter()
            .any(|m| m.kind == MsgKind::Perm && m.to == YOU));
    }

    #[test]
    fn tool_targets_are_short_and_useful() {
        assert_eq!(
            tool_target("Bash", &json!({"command":"npm   test"})),
            "npm test"
        );
        assert_eq!(
            tool_target("Grep", &json!({"pattern":"unique","path":"db"})),
            "\"unique\" in db"
        );
        assert_eq!(
            tool_target("Edit", &json!({"file_path":"src/a.rs"})),
            "src/a.rs"
        );
    }

    #[test]
    fn counts_edited_lines() {
        let mut files = Vec::new();
        track_files(
            &mut files,
            "Edit",
            &json!({"file_path":"a.rs","old_string":"x","new_string":"y\nz"}),
        );
        track_files(
            &mut files,
            "Write",
            &json!({"file_path":"b.rs","content":"1\n2\n3"}),
        );
        assert_eq!(files, vec![("a.rs".into(), 2, 1), ("b.rs".into(), 3, 0)]);
    }
}
