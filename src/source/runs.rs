//! Orchestrator runs: units of work that pass through stages (analysis, plan,
//! development, test, QA, security, CI, merge).
//!
//! A run is a folder `runs/<run-id>/` next to (or above) a session's working
//! folder, holding an optional `run.json` (`id`, `goal`, `createdAt`, `stages`
//! with each stage's `id` and `agent` type) and stage files such as
//! `analysis.md`, `dev.md`, `dev-wip.md`, `qa-2.md`. A stage file may start
//! with a `---` front matter (`stage`, `status`, `owner`); its first lines may
//! hold a decision (`KABUL`, `RET`, `ONAY`, `VETO`, `ACCEPT`, `REJECT`).
//! Everything is optional and read leniently: files are often left `pending`
//! while the work goes on, so live agents whose description names the run
//! (`<name> · R-20261114-1720-slug · <task>`) fill in what files do not say.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::Value;

use crate::model::{
    run_id_in, Agent, AgentState, Millis, Run, RunStage, Snapshot, StageStatus, STAGES,
};

/// Runs untouched for longer than this are not read.
const MAX_AGE_MS: Millis = 3 * 24 * 3600 * 1000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct StageFile {
    pub stage: String,
    pub status: String,
    /// The last decision in the file (later rounds are appended).
    pub decision: String,
    /// Rejecting decisions in the file (earlier rounds).
    pub rejections: u32,
    pub modified: Millis,
}

#[derive(Debug, Clone, Default)]
pub struct RunDir {
    pub id: String,
    pub goal: String,
    pub created: Millis,
    /// Stage keys listed in `run.json`, with the agent type of each.
    pub planned: Vec<(String, String)>,
    pub files: Vec<StageFile>,
    pub modified: Millis,
}

/// Reads run folders, re-reading only files that changed.
#[derive(Default)]
pub struct RunReader {
    cache: HashMap<PathBuf, (SystemTime, Option<StageFile>)>,
    runs_json: HashMap<PathBuf, (SystemTime, RunDir)>,
}

fn millis(t: SystemTime) -> Millis {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as Millis)
        .unwrap_or(0)
}

/// `runs/` folders in the given working folders or up to three levels above.
pub fn find_dirs<'a>(cwds: impl Iterator<Item = &'a str>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for cwd in cwds.filter(|c| !c.is_empty()) {
        for dir in Path::new(cwd).ancestors().take(4) {
            let runs = dir.join("runs");
            if runs.is_dir() && !out.contains(&runs) {
                out.push(runs);
            }
        }
    }
    out
}

/// Canonical stage key of a stage file name or `stage:` value.
pub fn stage_key(name: &str) -> Option<&'static str> {
    let n = name.to_ascii_lowercase();
    let head = n.split(['-', '_', '.', ' ']).next().unwrap_or("");
    let key = match head {
        "analysis" | "analiz" | "analyst" => "analysis",
        "plan" | "design" | "architect" => "plan",
        "dev" | "develop" | "development" | "gelistirme" => "dev",
        "test" | "tests" => "test",
        "qa" | "review" => "qa",
        "security" | "guvenlik" | "sec" => "security",
        "ci" => "ci",
        "merge" | "birlesme" | "cand" | "release" => "merge",
        _ => return None,
    };
    Some(key)
}

/// Stage an agent type works in, by the words it contains.
pub fn stage_of_type(kind: &str) -> Option<&'static str> {
    let k = kind.to_ascii_lowercase();
    let has = |w: &[&str]| w.iter().any(|x| k.contains(x));
    Some(if has(&["analyst", "analist"]) {
        "analysis"
    } else if has(&["architect", "mimar", "planner"]) {
        "plan"
    } else if has(&["test-writer", "tester", "testci"]) {
        "test"
    } else if has(&["security", "guvenlik"]) {
        "security"
    } else if has(&["qa", "review"]) {
        "qa"
    } else if has(&["ci-", "-ci", "ci_watch"]) || k == "ci" {
        "ci"
    } else if has(&["developer", "gelistirici", "coder"]) {
        "dev"
    } else {
        return None;
    })
}

const ACCEPT: [&str; 4] = ["KABUL", "ONAY", "ACCEPT", "APPROVE"];
const REJECT: [&str; 3] = ["RET", "VETO", "REJECT"];

/// A review decision on a line: `Karar: KABUL`, `**Karar: KOŞULLU ONAY.**`,
/// `Decision: REJECT`, or the word alone (`RET`). Headings that merely start
/// with a word (`## RET — kapanacaklar`) and templates naming both outcomes
/// (`## Karar: KABUL / RET`) are not decisions.
fn decision_in(line: &str) -> Option<String> {
    let t = line
        .trim()
        .trim_start_matches(['#', '*', '>', '-', ' '])
        .trim_end_matches(['*', ' ', '.']);
    let labelled = [
        "Karar:",
        "KARAR:",
        "Sonuç:",
        "Decision:",
        "Verdict:",
        "decision:",
    ]
    .iter()
    .find_map(|p| t.strip_prefix(p));
    let rest = match labelled {
        Some(r) => r.trim_start_matches(['*', ' ']),
        None => t,
    };
    let words: Vec<String> = rest
        .split(|c: char| !(c.is_alphabetic()))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect();
    let is_accept = |w: &str| ACCEPT.contains(&w);
    let is_reject = |w: &str| REJECT.contains(&w);
    let (conditional, first) = match words.first().map(String::as_str) {
        Some("KOŞULLU") | Some("CONDITIONAL") => (true, words.get(1)?.as_str()),
        Some(w) => (false, w),
        None => return None,
    };
    if !(is_accept(first) || is_reject(first)) {
        return None;
    }
    if labelled.is_none() && words.len() > 1 + usize::from(conditional) {
        return None;
    }
    // A template that names both outcomes.
    let named = |f: &dyn Fn(&str) -> bool| words.iter().take(4).any(|w| f(w));
    if named(&is_accept) && named(&is_reject) {
        return None;
    }
    Some(if conditional {
        format!("{} {first}", words[0])
    } else {
        first.to_string()
    })
}

pub fn is_rejection(decision: &str) -> bool {
    REJECT.contains(&decision)
}

/// Parses a stage file: front matter, then a decision in the first lines.
pub fn parse_stage_file(name: &str, text: &str, modified: Millis) -> Option<StageFile> {
    let mut f = StageFile {
        modified,
        ..StageFile::default()
    };
    let mut body = text;
    if let Some(rest) = text.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            for line in rest[..end].lines() {
                if let Some((k, v)) = line.split_once(':') {
                    let v = v.trim().to_string();
                    match k.trim() {
                        "stage" => f.stage = v,
                        "status" => f.status = v.to_ascii_lowercase(),
                        "decision" | "karar" | "verdict" => {
                            f.decision = decision_in(&v).unwrap_or_default()
                        }
                        _ => {}
                    }
                }
            }
            body = &rest[end + 4..];
        }
    }
    let key = stage_key(&f.stage).or_else(|| stage_key(name))?;
    f.stage = key.to_string();
    // A file still marked pending is a template (it names "KABUL / RET").
    if f.status == "pending" {
        f.decision.clear();
        return Some(f);
    }
    let found: Vec<String> = body.lines().filter_map(decision_in).collect();
    f.rejections = found.iter().filter(|d| is_rejection(d)).count() as u32;
    if let Some(last) = found.last() {
        f.decision = last.clone();
    }
    if f.rejections == 0 && is_rejection(&f.decision) {
        f.rejections = 1;
    }
    Some(f)
}

fn parse_run_json(text: &str, dir_name: &str) -> RunDir {
    let v: Value = serde_json::from_str(text).unwrap_or(Value::Null);
    let s = |keys: &[&str]| {
        keys.iter()
            .find_map(|k| v.get(*k).and_then(Value::as_str))
            .unwrap_or("")
            .to_string()
    };
    let created = chrono::DateTime::parse_from_rfc3339(&s(&["createdAt", "created"]))
        .map(|d| d.timestamp_millis())
        .unwrap_or(0);
    let planned = v
        .get("stages")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| {
                    let (id, agent) = match x {
                        Value::String(id) => (id.as_str(), ""),
                        _ => (
                            x.get("id").and_then(Value::as_str)?,
                            x.get("agent").and_then(Value::as_str).unwrap_or(""),
                        ),
                    };
                    Some((stage_key(id)?.to_string(), agent.to_string()))
                })
                .collect()
        })
        .unwrap_or_default();
    let id = s(&["id", "run"]);
    RunDir {
        id: if id.is_empty() { dir_name.into() } else { id },
        goal: s(&["goal", "title", "summary"]),
        created,
        planned,
        ..RunDir::default()
    }
}

impl RunReader {
    pub fn read(&mut self, dirs: &[PathBuf], now: Millis) -> Vec<RunDir> {
        let mut out = Vec::new();
        for runs in dirs {
            let Ok(entries) = std::fs::read_dir(runs) else {
                continue;
            };
            for e in entries.flatten() {
                let path = e.path();
                let name = e.file_name().to_string_lossy().to_string();
                if !path.is_dir() || run_id_in(&name).is_none() {
                    continue;
                }
                if let Some(r) = self.read_run(&path, &name, now) {
                    out.push(r);
                }
            }
        }
        out
    }

    fn read_run(&mut self, dir: &Path, name: &str, now: Millis) -> Option<RunDir> {
        let entries: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().collect();
        let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let newest = entries
            .iter()
            .filter_map(|e| mtime(&e.path()))
            .chain(mtime(dir))
            .max()?;
        if now - millis(newest) > MAX_AGE_MS {
            return None;
        }
        let json = dir.join("run.json");
        let mut run = match mtime(&json) {
            Some(t) => match self.runs_json.get(&json) {
                Some((at, r)) if *at == t => r.clone(),
                _ => {
                    let text = std::fs::read_to_string(&json).unwrap_or_default();
                    let r = parse_run_json(&text, name);
                    self.runs_json.insert(json.clone(), (t, r.clone()));
                    r
                }
            },
            None => RunDir {
                id: name.to_string(),
                ..RunDir::default()
            },
        };
        run.modified = millis(newest);
        for e in &entries {
            let p = e.path();
            let file = e.file_name().to_string_lossy().to_string();
            if !file.ends_with(".md") {
                continue;
            }
            let Some(t) = mtime(&p) else { continue };
            let parsed = match self.cache.get(&p) {
                Some((at, f)) if *at == t => f.clone(),
                _ => {
                    // Later rounds append their decision at the end.
                    let text = std::fs::read_to_string(&p).unwrap_or_default();
                    let f = parse_stage_file(&file, &text, millis(t));
                    self.cache.insert(p.clone(), (t, f.clone()));
                    f
                }
            };
            if let Some(f) = parsed {
                run.files.push(f);
            }
        }
        Some(run)
    }
}

/// Stage an agent works in for its run.
fn agent_stage(a: &Agent, planned: &[(String, String)]) -> Option<&'static str> {
    if !a.agent_type.is_empty() {
        if let Some((k, _)) = planned.iter().find(|(_, t)| *t == a.agent_type) {
            return STAGES.iter().find(|s| *s == k).copied();
        }
    }
    stage_of_type(&a.agent_type).or_else(|| stage_of_type(a.name.split(['-', ' ']).next()?))
}

/// Combines run folders with the agents working on them.
pub fn build_runs(dirs: Vec<RunDir>, snap: &Snapshot, host: &str, now: Millis) -> Vec<Run> {
    let mut dirs = dirs;
    // Runs only known from agents (another machine's files, no folder yet).
    for a in snap.agents.iter().filter(|a| !a.run.is_empty()) {
        if !dirs.iter().any(|d| d.id == a.run) {
            dirs.push(RunDir {
                id: a.run.clone(),
                created: a.started,
                modified: a.started,
                ..RunDir::default()
            });
        }
    }
    let mut runs: Vec<Run> = dirs.into_iter().map(|d| build_run(d, snap, host)).collect();
    runs.retain(|r| r.active() || now - r.updated < MAX_AGE_MS);
    runs.sort_by_key(|r| (std::cmp::Reverse(r.active()), std::cmp::Reverse(r.updated)));
    runs
}

fn build_run(d: RunDir, snap: &Snapshot, host: &str) -> Run {
    let agents: Vec<&Agent> = snap.agents.iter().filter(|a| a.run == d.id).collect();
    // A lead started other agents of the same run; it is not a stage worker.
    let is_lead = |a: &Agent| {
        a.parent.is_some()
            && agents
                .iter()
                .any(|c| c.parent.as_deref() == Some(a.id.as_str()))
    };
    let lead = agents.iter().find(|a| is_lead(a)).copied();
    let mut updated = d.modified.max(d.created);
    let mut stages = Vec::new();
    for key in STAGES {
        let files: Vec<&StageFile> = d.files.iter().filter(|f| f.stage == key).collect();
        let workers: Vec<&Agent> = agents
            .iter()
            .filter(|a| !is_lead(a) && agent_stage(a, &d.planned) == Some(key))
            .copied()
            .collect();
        let mut st = RunStage {
            key: key.to_string(),
            rounds: workers.len() as u32,
            rejections: files.iter().map(|f| f.rejections).sum(),
            agent: d
                .planned
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, t)| t.clone())
                .unwrap_or_default(),
            ..RunStage::default()
        };
        let latest = files
            .iter()
            .filter(|f| !f.decision.is_empty() || f.status != "pending")
            .max_by_key(|f| f.modified);
        if let Some(f) = latest {
            st.decision = f.decision.clone();
            st.since = f.modified;
            st.status = if is_rejection(&f.decision) {
                StageStatus::Rejected
            } else if !f.decision.is_empty() || f.status == "done" {
                StageStatus::Done
            } else if f.status == "skipped" {
                StageStatus::Skipped
            } else if f.status == "blocked" {
                StageStatus::Blocked
            } else if f.status.starts_with("in") || f.status == "running" {
                StageStatus::Running
            } else {
                StageStatus::Pending
            };
        }
        let live: Vec<&&Agent> = workers
            .iter()
            .filter(|a| a.state != AgentState::Done)
            .collect();
        if !live.is_empty() {
            let all_wait = live
                .iter()
                .all(|a| matches!(a.state, AgentState::Waiting | AgentState::Stalled));
            st.status = if all_wait {
                StageStatus::Waiting
            } else {
                StageStatus::Running
            };
            st.owners = live.iter().map(|a| a.name.clone()).collect();
            st.since = live.iter().map(|a| a.started).min().unwrap_or(0);
        } else if let Some(last) = workers.iter().max_by_key(|a| a.last_activity) {
            st.owners = vec![last.name.clone()];
            // The agent finished; files are often never updated.
            if matches!(st.status, StageStatus::Pending | StageStatus::Running) {
                st.status = StageStatus::Done;
                st.since = last.last_activity;
            }
        }
        if st.status == StageStatus::Pending
            && files.is_empty()
            && !d.planned.is_empty()
            && !d.planned.iter().any(|(k, _)| k == key)
            && key != "merge"
        {
            st.status = StageStatus::Skipped;
        }
        updated = updated.max(workers.iter().map(|a| a.last_activity).max().unwrap_or(0));
        stages.push(st);
    }
    // Stages nobody worked on while later ones went ahead were skipped
    // (a coordinator often does a small analysis itself without a file).
    for k in 0..stages.len() {
        let later = stages[k + 1..].iter().any(|s| {
            s.status.active() || matches!(s.status, StageStatus::Done | StageStatus::Rejected)
        });
        if later && stages[k].status == StageStatus::Pending && stages[k].owners.is_empty() {
            stages[k].status = StageStatus::Skipped;
        }
    }
    // A stage resumed for another round (the lead sends the developer back
    // after the test) is held since the latest hand-off, not since its
    // agent first started.
    let handoff = stages
        .iter()
        .filter(|s| s.status == StageStatus::Done)
        .map(|s| s.since)
        .max()
        .unwrap_or(0);
    for st in stages.iter_mut().filter(|s| s.status.active()) {
        st.since = st.since.max(handoff);
    }
    if let Some(l) = lead {
        updated = updated.max(l.last_activity);
    }
    // Team: a known team code in the run id ("R-...-kabuk-tasarim-nav-1"),
    // else the team most of its agents belong to (the lead counts more).
    // Groups without a label (independent reviewers, the default group) do
    // not own runs when labelled teams exist.
    let labelled = |t: &str| snap.team_labels.iter().any(|(c, _)| c == t);
    let slug = d.id.get(16..).unwrap_or("");
    let named = snap
        .team_labels
        .iter()
        .filter(|(c, _)| slug.starts_with(c.as_str()))
        .max_by_key(|(c, _)| c.len())
        .map(|(c, _)| c.clone());
    let mut votes: HashMap<&str, usize> = HashMap::new();
    for a in &agents {
        if let Some(t) = a.team.as_deref() {
            if snap.team_labels.is_empty() || labelled(t) {
                *votes.entry(t).or_default() += if is_lead(a) { 3 } else { 1 };
            }
        }
    }
    let team = named.or_else(|| {
        votes
            .into_iter()
            .max_by_key(|(t, n)| (*n, std::cmp::Reverse(*t)))
            .map(|(t, _)| t.to_string())
    });
    let goal = if d.goal.is_empty() {
        agents
            .iter()
            .map(|a| crate::model::task_text(&a.task))
            .find(|t| !t.is_empty())
            .unwrap_or(slug)
            .to_string()
    } else {
        d.goal
    };
    Run {
        goal,
        id: d.id,
        team,
        host: host.to_string(),
        created: d.created,
        updated,
        stages,
        lead: lead.map(|l| l.name.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_files_as_written_by_agent_orchestra() {
        // Template left pending, as most files are.
        let f = parse_stage_file(
            "dev.md",
            "---\nrun: R-20261114-1720-kabuk-tasarim-nav-1-demo\nstage: dev\nstatus: pending\nowner: orchestra-developer\n---\n\n# Geliştirme\n> Devir sözleşmesi ...",
            5,
        )
        .unwrap();
        assert_eq!(
            (f.stage.as_str(), f.status.as_str(), f.decision.as_str()),
            ("dev", "pending", "")
        );
        // Decision alone on the first body line (R-20261112-surum-gecmisi/qa.md).
        let f = parse_stage_file(
            "qa.md",
            "---\nrun: R-20261112-surum-gecmisi\nstage: qa\nstatus: done\nauthor: qa-surum-gecmisi\n---\n\nRET\n\n# QA: v2.9 adayı",
            6,
        )
        .unwrap();
        assert_eq!((f.status.as_str(), f.decision.as_str()), ("done", "RET"));
        // Round two and work-in-progress files belong to their stage.
        assert_eq!(
            parse_stage_file("qa-2.md", "**Karar: KABUL**", 1)
                .unwrap()
                .decision,
            "KABUL"
        );
        assert_eq!(
            parse_stage_file("test-wip.md", "", 1).unwrap().stage,
            "test"
        );
        assert!(parse_stage_file("store.md", "x", 1).is_none());
        assert_eq!(decision_in("RETURN value"), None);
        // nav-1 qa.md, 18:55: a pending template naming both outcomes.
        let f = parse_stage_file(
            "qa.md",
            "---\nstage: qa\nstatus: pending\nowner: orchestra-qa-reviewer\n---\n\n# QA\n\n## Karar: KABUL / RET\n",
            1,
        )
        .unwrap();
        assert_eq!(f.decision, "");
        assert_eq!(decision_in("## Karar: KABUL / RET"), None);
        // qa-orkestra-rolleri.md: round 1 RET, a "RET" heading, round 2 KABUL appended.
        let f = parse_stage_file(
            "qa-orkestra-rolleri.md",
            "---\nstage: qa\nstatus: done\n---\n\n## Karar: RET\n\n### ORTA-2 — KOŞULLU ONAY koşullarının kapanışını denetleyen kapı yok\n## RET — kapanacaklar\n\n## Karar: KABUL\n\nPR 3411 ... Tur 1'deki RET maddeleri kanıtla kapandı\n",
            1,
        )
        .unwrap();
        assert_eq!((f.decision.as_str(), f.rejections), ("KABUL", 1));
        // security.md: conditional approval.
        assert_eq!(
            decision_in("**Karar: KOŞULLU ONAY.** Kod kendi içinde sağlam. OTP, oturum").as_deref(),
            Some("KOŞULLU ONAY")
        );
    }

    #[test]
    fn run_json_with_and_without_stages() {
        let r = parse_run_json(
            r#"{"id":"R-20261114-1720-kabuk-tasarim-nav-1-demo","goal":"kabuk-tasarim: NAV-1","createdAt":"2026-11-14T17:20:03.633Z",
                "stages":[{"id":"analysis","agent":"orchestra-analyst"},{"id":"dev","agent":"orchestra-developer"},{"id":"qa","agent":"orchestra-qa-reviewer"}]}"#,
            "x",
        );
        assert_eq!(r.planned.len(), 3);
        assert_eq!(r.planned[1], ("dev".into(), "orchestra-developer".into()));
        let r = parse_run_json(
            r#"{"run":"R-20261112-surum-gecmisi","status":"ready"}"#,
            "x",
        );
        assert_eq!(r.id, "R-20261112-surum-gecmisi");
        assert!(r.planned.is_empty());
    }

    fn agent(id: &str, name: &str, kind: &str, parent: Option<&str>, state: AgentState) -> Agent {
        Agent {
            id: id.into(),
            name: name.into(),
            agent_type: kind.into(),
            parent: parent.map(str::to_string),
            run: "R-20261114-1720-kabuk-tasarim-nav-1-demo".into(),
            team: Some("kabuk-tasarim".into()),
            state,
            started: 100,
            last_activity: 200,
            ..Agent::default()
        }
    }

    /// Live, 18:30: every stage file of the run was still `pending`, the
    /// developer had handed back and the tester was running under the lead.
    #[test]
    fn live_agents_fill_in_pending_files() {
        let d = RunDir {
            id: "R-20261114-1720-kabuk-tasarim-nav-1-demo".into(),
            planned: ["analysis", "plan", "dev", "test", "qa", "ci"]
                .iter()
                .map(|k| (k.to_string(), String::new()))
                .collect(),
            files: vec![StageFile {
                stage: "dev".into(),
                status: "pending".into(),
                ..StageFile::default()
            }],
            ..RunDir::default()
        };
        let snap = Snapshot {
            agents: vec![
                agent("s", "Koordinatör", "", None, AgentState::Idle),
                agent(
                    "s/lead",
                    "Ekip lideri · gezinme",
                    "orchestra-developer",
                    Some("s"),
                    AgentState::Waiting,
                ),
                agent(
                    "s/dev",
                    "Geliştirici · gezinme",
                    "orchestra-developer",
                    Some("s/lead"),
                    AgentState::Done,
                ),
                agent(
                    "s/test",
                    "Testçi · gezinme",
                    "orchestra-test-writer",
                    Some("s/lead"),
                    AgentState::Working,
                ),
            ],
            ..Snapshot::default()
        };
        let mut snap = snap;
        snap.team_labels = vec![("kabuk-tasarim".into(), "Uygulama Kabuğu".into())];
        snap.agents[1].team = Some("Bağımsız".into());
        let runs = build_runs(vec![d], &snap, "box", 1000);
        let r = &runs[0];
        let st = |k: &str| r.stages.iter().find(|s| s.key == k).unwrap();
        assert_eq!(st("dev").status, StageStatus::Done);
        assert_eq!(st("test").status, StageStatus::Running);
        assert_eq!(st("test").owners, vec!["Testçi · gezinme".to_string()]);
        assert_eq!(st("security").status, StageStatus::Skipped);
        // Nobody did analysis or plan as an agent while development went ahead.
        assert_eq!(st("analysis").status, StageStatus::Skipped);
        assert_eq!(st("qa").status, StageStatus::Pending);
        assert_eq!(st("merge").status, StageStatus::Pending);
        assert_eq!(r.lead.as_deref(), Some("Ekip lideri · gezinme"));
        assert_eq!(r.team.as_deref(), Some("kabuk-tasarim"));
        assert_eq!(r.current().unwrap().key, "test");
        // The developer finished at 200; the tester (started 100) is held since then.
        assert_eq!(st("test").since, 200);
        assert!(r.active());
    }

    #[test]
    fn planned_agent_type_is_kept_on_stages() {
        let mut d = parse_run_json(
            r#"{"id":"R-20261116-1000-shop-api-x","stages":[{"id":"dev","agent":"orchestra-developer"},{"id":"test","agent":"orchestra-test-writer"},"qa"]}"#,
            "x",
        );
        d.modified = 900;
        let runs = build_runs(vec![d], &Snapshot::default(), "box", 1000);
        let st = |k: &str| runs[0].stages.iter().find(|s| s.key == k).unwrap().clone();
        assert_eq!(st("dev").agent, "orchestra-developer");
        assert_eq!(st("test").agent, "orchestra-test-writer");
        assert_eq!(st("qa").agent, "");
        assert_eq!(st("analysis").agent, "");
    }

    #[test]
    fn run_ids_and_stage_names() {
        assert_eq!(
            run_id_in(
                "testci-kabuk-tasarim-gezinme · R-20261114-1720-kabuk-tasarim-nav-1-demo · Gezinme"
            ),
            Some("R-20261114-1720-kabuk-tasarim-nav-1-demo")
        );
        assert_eq!(
            run_id_in("Akış: R-20261114-1720-x, Jira CLM20-120."),
            Some("R-20261114-1720-x")
        );
        assert_eq!(run_id_in("R-2026 nope"), None);
        use crate::model::name_in_description as name;
        // Live, 18:43: a coordinator writing "<run> <agent-name>".
        assert_eq!(
            name("R-20261114-1842-doktor-veri-bankasi analist-hekim-verisi-kamu-kaynaklari"),
            Some("analist-hekim-verisi-kamu-kaynaklari")
        );
        assert_eq!(
            name("testci-kabuk-tasarim-gezinme · R-20261114-1720-kabuk-tasarim-nav-1-demo · Gezinme bağımsız testi"),
            Some("testci-kabuk-tasarim-gezinme")
        );
        assert_eq!(
            name("R-20261114-1515-kabuk-tasarim-kurulum-si · Kurulum sihirbazı"),
            None
        );
        assert_eq!(
            name("R-20261114-1538-kurulum-sihirbazi-liste analysis"),
            None
        );
        assert_eq!(name("Check users schema"), None);
        assert_eq!(stage_of_type("orchestra-qa-reviewer"), Some("qa"));
        assert_eq!(stage_of_type("orchestra-ci-watcher"), Some("ci"));
        assert_eq!(stage_of_type("orchestra-test-writer"), Some("test"));
        assert_eq!(
            stage_of_type("orchestra-security-reviewer"),
            Some("security")
        );
        assert_eq!(stage_of_type("general-purpose"), None);
    }
}
