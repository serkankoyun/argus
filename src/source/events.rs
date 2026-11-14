//! Lifecycle events: an open, append-only JSONL format that tells Argus exactly
//! what an agent is doing, instead of guessing it from transcripts.
//!
//! Claude Code only writes a transcript line when a step finishes, so during a
//! long `Bash` command or while it waits for the user the transcript is silent.
//! `argus hook --install` registers `argus hook --event` for Claude Code's
//! lifecycle hooks, which appends one line per event to
//! `<claude dir>/argus/events/events-YYYY-MM-DD.jsonl`. Any other tool can write
//! the same format (see `docs/EVENTS.md`) to a directory listed in
//! `ARGUS_EVENTS`.
//!
//! One line:
//! `{"v":1,"ts":"2026-11-14T10:43:25.767Z","event":"PreToolUse","sid":"<session>",
//!   "agentId":"<subagent>","tool":"Bash","status":"..."}`
//! Parsing is lenient: `ts` may be RFC 3339 or epoch milliseconds, `sid` may be
//! `session_id`, `agentId` may be `agent_id`, `tool` may be `tool_name`.

use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::{json, Value};

use crate::model::{now_millis, AgentState, Millis, MsgKind, Snapshot};

/// Event files untouched for longer than this are ignored.
const HORIZON: Duration = Duration::from_secs(24 * 3600);
/// Event files older than this are deleted by `argus hook --event`.
const KEEP_DAYS: u64 = 3;
/// A tool reported as started but never finished is trusted for this long.
const MAX_TOOL_MS: Millis = 6 * 3600 * 1000;

pub fn dir(root: &Path) -> PathBuf {
    root.join("argus").join("events")
}

/// Directories read for events: Argus's own, the ones in `ARGUS_EVENTS`
/// (a path list), and known tools that write the same format.
pub fn dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![dir(root)];
    if let Some(list) = std::env::var_os("ARGUS_EVENTS") {
        out.extend(std::env::split_paths(&list).filter(|p| !p.as_os_str().is_empty()));
    }
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        // agent-orchestra logs Claude Code hook events in this format.
        out.push(PathBuf::from(home).join(".agent-orchestra").join("events"));
    }
    out.dedup();
    out
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub ts: Millis,
    pub event: String,
    pub sid: String,
    pub agent_id: String,
    pub tool: String,
    pub status: String,
    /// Permission mode, when the writer reports it.
    pub mode: String,
}

/// Permission modes in which Claude Code never shows a permission prompt.
pub fn mode_never_asks(mode: &str) -> bool {
    matches!(mode, "bypassPermissions" | "auto" | "dontAsk" | "yolo")
}

fn text(v: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

/// Parses one line; `None` when it is not an event.
pub fn parse(line: &str) -> Option<Event> {
    let v: Value = serde_json::from_str(line).ok()?;
    let ts = match v.get("ts") {
        Some(Value::Number(n)) => n.as_i64()?,
        Some(Value::String(s)) => chrono::DateTime::parse_from_rfc3339(s)
            .ok()?
            .timestamp_millis(),
        _ => return None,
    };
    let event = text(&v, &["event", "hook_event_name"]);
    let sid = text(&v, &["sid", "session_id"]);
    if event.is_empty() || sid.is_empty() {
        return None;
    }
    Some(Event {
        ts,
        event,
        sid,
        agent_id: text(&v, &["agentId", "agent_id"]),
        tool: text(&v, &["tool", "tool_name"]),
        status: text(&v, &["status", "notification_type"]),
        mode: text(&v, &["mode", "permission_mode", "permissionMode"]),
    })
}

/// What the events say about one session.
#[derive(Debug, Clone, Default)]
pub struct Live {
    pub last: Millis,
    /// Tool started (PreToolUse) and not finished yet.
    pub running: Option<(String, Millis)>,
    /// Waiting for a permission answer.
    pub asking: bool,
    /// Finished its turn or waits for a prompt.
    pub idle: bool,
    pub ended: bool,
    pub finished_subagents: HashSet<String>,
    pub mode: String,
}

impl Live {
    fn apply(&mut self, e: &Event) {
        if !e.mode.is_empty() {
            self.mode = e.mode.clone();
        }
        if e.ts < self.last {
            return;
        }
        self.last = e.ts;
        match e.event.as_str() {
            "PreToolUse" => {
                self.running = Some((e.tool.clone(), e.ts));
                self.asking = false;
                self.idle = false;
            }
            "PostToolUse" | "PostToolUseFailure" | "UserPromptSubmit" | "SessionStart" => {
                self.running = None;
                self.asking = false;
                self.idle = false;
                self.ended = false;
            }
            "PermissionRequest" => self.asking = true,
            "Notification" if e.status.contains("permission") => self.asking = true,
            "Notification" if e.status.contains("idle") => {
                self.idle = true;
                self.running = None;
            }
            "Stop" | "SubagentStop" => {
                self.idle = true;
                self.running = None;
                self.asking = false;
            }
            "SessionEnd" => {
                self.ended = true;
                self.running = None;
                self.asking = false;
            }
            _ => {}
        }
    }
}

/// Incremental reader over all event directories.
#[derive(Default)]
pub struct EventLog {
    dirs: Vec<PathBuf>,
    offsets: HashMap<PathBuf, (u64, Vec<u8>)>,
    pub sessions: HashMap<String, Live>,
}

impl EventLog {
    pub fn new(dirs: Vec<PathBuf>) -> Self {
        Self {
            dirs,
            ..Self::default()
        }
    }

    /// Session events go to the session; events carrying `agentId` come from a
    /// subagent of that session and go to `<sid>/<agentId>`, the subagent's id.
    pub fn record(&mut self, e: &Event) {
        if e.agent_id.is_empty() {
            self.sessions.entry(e.sid.clone()).or_default().apply(e);
            return;
        }
        if e.event == "SubagentStop" {
            let parent = self.sessions.entry(e.sid.clone()).or_default();
            parent.finished_subagents.insert(e.agent_id.clone());
        }
        let key = format!("{}/{}", e.sid, e.agent_id);
        self.sessions.entry(key).or_default().apply(e);
    }

    pub fn poll(&mut self) {
        let mut files: Vec<PathBuf> = self
            .dirs
            .iter()
            .filter_map(|d| std::fs::read_dir(d).ok())
            .flat_map(|r| r.flatten().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "jsonl") && recent(p))
            .collect();
        files.sort();
        for path in files {
            let (offset, partial) = self.offsets.entry(path.clone()).or_default();
            let Ok(mut f) = File::open(&path) else {
                continue;
            };
            let len = f.metadata().map(|m| m.len()).unwrap_or(0);
            if len < *offset {
                *offset = 0;
                partial.clear();
            }
            if len == *offset || f.seek(SeekFrom::Start(*offset)).is_err() {
                continue;
            }
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_err() {
                continue;
            }
            *offset += buf.len() as u64;
            partial.extend_from_slice(&buf);
            let Some(end) = partial.iter().rposition(|b| *b == b'\n') else {
                continue;
            };
            let complete: Vec<u8> = partial.drain(..=end).collect();
            for line in String::from_utf8_lossy(&complete).lines() {
                if let Some(e) = parse(line) {
                    self.record(&e);
                }
            }
        }
    }

    /// Corrects agent states in `snap` with what the events say.
    pub fn apply(&self, snap: &mut Snapshot, now: Millis) {
        let mut approved = HashSet::new();
        for a in &mut snap.agents {
            let (root, sub) = match a.id.split_once('/') {
                Some((r, s)) => (r, Some(s)),
                None => (a.id.as_str(), None),
            };
            if let Some(sub) = sub {
                if self
                    .sessions
                    .get(root)
                    .is_some_and(|l| l.finished_subagents.contains(sub))
                {
                    a.state = AgentState::Done;
                    continue;
                }
            }
            // A session (or the parent of a subagent) that never asks for
            // permission is not waiting for an approval.
            let quiet = [a.id.as_str(), root]
                .iter()
                .filter_map(|k| self.sessions.get(*k))
                .any(|l| mode_never_asks(&l.mode) && !l.asking);
            if quiet && a.state == AgentState::Waiting {
                a.state = AgentState::Working;
                approved.insert(a.id.clone());
            }
            let Some(live) = self.sessions.get(a.id.as_str()) else {
                continue;
            };
            if live.last + 1000 < a.last_activity {
                continue; // The transcript is newer than the last event.
            }
            if live.ended {
                a.state = AgentState::Done;
            } else if live.asking {
                a.state = AgentState::Waiting;
            } else if let Some((tool, since)) = &live.running {
                if now - since < MAX_TOOL_MS {
                    a.state = AgentState::Working;
                    if !tool.is_empty() {
                        a.tool = tool.clone();
                    }
                    a.last_activity = a.last_activity.max(live.last);
                    approved.insert(a.id.clone());
                }
            } else if live.idle && (a.state != AgentState::Done || now - live.last < 30 * 60_000) {
                a.state = AgentState::Idle;
                approved.insert(a.id.clone());
            }
        }
        // A running or idle agent is not waiting for an approval.
        snap.messages.retain(|m| {
            !(m.kind == MsgKind::Perm && m.request.is_none() && approved.contains(&m.from))
        });
    }
}

fn recent(p: &Path) -> bool {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age < HORIZON)
}

/// `argus hook --event`: turns a Claude Code hook payload into one event line.
pub fn line_from_hook(payload: &Value, ts: Millis) -> Option<String> {
    let s = |k: &str| payload.get(k).and_then(Value::as_str).unwrap_or("");
    let event = s("hook_event_name");
    let sid = s("session_id");
    if event.is_empty() || sid.is_empty() {
        return None;
    }
    let ts = chrono::DateTime::from_timestamp_millis(ts)?
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut v = json!({ "v": 1, "ts": ts, "event": event, "sid": sid });
    for (from, to) in [
        ("agent_id", "agentId"),
        ("tool_name", "tool"),
        ("notification_type", "status"),
        ("permission_mode", "mode"),
        ("cwd", "cwd"),
    ] {
        if !s(from).is_empty() {
            v[to] = json!(s(from));
        }
    }
    Some(v.to_string())
}

/// Appends a hook payload to today's event file. Never fails loudly: a hook
/// must not disturb Claude Code.
pub fn record(root: &Path, input: &str) {
    let Ok(payload) = serde_json::from_str::<Value>(input) else {
        return;
    };
    let now = now_millis();
    let Some(line) = line_from_hook(&payload, now) else {
        return;
    };
    let dir = dir(root);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let day = chrono::Utc::now().format("%Y-%m-%d");
    let path = dir.join(format!("events-{day}.jsonl"));
    let fresh = !path.exists();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
    if fresh {
        prune(&dir);
    }
}

fn prune(dir: &Path) {
    let keep = Duration::from_secs(KEEP_DAYS * 24 * 3600);
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age > keep);
        if old && e.path().extension().is_some_and(|x| x == "jsonl") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Claude Code hook events recorded by `argus hook --event`.
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Notification",
    "Stop",
    "SubagentStop",
    "SessionEnd",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Agent, Message};

    #[test]
    fn parses_own_and_compatible_lines() {
        let e = parse(r#"{"v":1,"ts":"2026-11-14T10:43:25.767Z","event":"PostToolUse","sid":"s1","tool":"Bash","durMs":20}"#).unwrap();
        assert_eq!(
            (e.event.as_str(), e.sid.as_str(), e.tool.as_str()),
            ("PostToolUse", "s1", "Bash")
        );
        assert!(e.ts > 1_700_000_000_000);
        let e = parse(r#"{"ts":5,"hook_event_name":"Stop","session_id":"s2"}"#).unwrap();
        assert_eq!((e.ts, e.event.as_str()), (5, "Stop"));
        assert!(parse(r#"{"ts":5,"event":"Stop"}"#).is_none());
        assert!(parse("not json").is_none());
    }

    #[test]
    fn hook_payload_round_trips() {
        let p = json!({"hook_event_name":"Notification","session_id":"s","notification_type":"idle_prompt","cwd":"/w"});
        let e = parse(&line_from_hook(&p, 1_794_652_662_022).unwrap()).unwrap();
        assert_eq!(e.status, "idle_prompt");
        assert_eq!(e.ts, 1_794_652_662_022);
    }

    fn snap_with(state: AgentState, last: Millis) -> Snapshot {
        let mut s = Snapshot::default();
        for id in ["s", "s/sub"] {
            s.agents.push(Agent {
                id: id.into(),
                state,
                last_activity: last,
                ..Agent::default()
            });
        }
        s.messages.push(Message {
            from: "s".into(),
            to: crate::model::YOU.into(),
            kind: MsgKind::Perm,
            ..Message::default()
        });
        s
    }

    #[test]
    fn events_correct_guessed_states() {
        let mut log = EventLog::default();
        let ev = |ts, event: &str, extra: &str| {
            parse(&format!(
                r#"{{"ts":{ts},"event":"{event}","sid":"s"{extra}}}"#
            ))
            .unwrap()
        };
        log.record(&ev(1000, "PreToolUse", r#","tool":"Bash""#));
        // A subagent running a long command is working, not waiting.
        log.record(&ev(1000, "PreToolUse", r#","tool":"Bash","agentId":"sub""#));
        // A long Bash command looked like a pending approval; it is running.
        let mut snap = snap_with(AgentState::Waiting, 900);
        log.apply(&mut snap, 400_000);
        assert_eq!(snap.agents[0].state, AgentState::Working);
        assert_eq!(snap.agents[1].state, AgentState::Working);
        assert!(snap.messages.is_empty());
        log.record(&ev(2000, "Notification", r#","status":"idle_prompt""#));
        log.record(&ev(2100, "SubagentStop", r#","agentId":"sub""#));
        let mut snap = snap_with(AgentState::Working, 1900);
        log.apply(&mut snap, 3000);
        assert_eq!(snap.agents[0].state, AgentState::Idle);
        assert_eq!(snap.agents[1].state, AgentState::Done);
        // Sessions in auto mode never wait for approvals.
        let mut log = EventLog::default();
        log.record(&parse(r#"{"ts":5,"event":"PostToolUse","sid":"s","mode":"auto"}"#).unwrap());
        let mut snap = snap_with(AgentState::Waiting, 900);
        log.apply(&mut snap, 1000);
        assert_eq!(snap.agents[1].state, AgentState::Working);
        assert!(snap.messages.is_empty());
    }
}
