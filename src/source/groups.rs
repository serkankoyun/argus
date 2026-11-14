//! Team rules: group agents into the user's own teams by their names, for
//! setups that do not use Agent Teams and do not report a `team` in events.
//!
//! `<claude dir>/argus/teams.json`:
//! ```json
//! {
//!   "strip": ["developer-", "qa-"],
//!   "teams": ["payments", "search-api"],
//!   "teams_file": "/path/to/teams.json",
//!   "aliases": { "old-code": "new-code" },
//!   "labels": { "payments": "Payments and Billing" },
//!   "label_key": "displayName",
//!   "short_names": true,
//!   "roles": { "developer": "Developer" },
//!   "coordinator": "Coordinator",
//!   "default": "no-team"
//! }
//! ```
//! An agent's name loses the first matching `strip` prefix; the rest is matched
//! against the team codes (longest prefix wins). Codes come from `teams` and,
//! optionally, from `teams_file`: a JSON list of strings, or objects with a
//! `code` (or `name`), at the top level or under `teams`. Agents that match no
//! code get `default` when it is set. `aliases` maps retired codes to current
//! ones, so agents from before a rename join the renamed team. Agents that
//! already have a team keep it. Team display names come from `labels`, or from
//! the `teams_file` objects' `label_key` field (default: `label`, `title`,
//! `displayName`). With `short_names`, a grouped agent is named
//! `<role> · <work>` (role labels from `roles`); `coordinator` names root
//! sessions that started subagents and gives them a group of their own.

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::model::Snapshot;

pub fn path(root: &Path) -> PathBuf {
    root.join("argus").join("teams.json")
}

#[derive(Debug, Default)]
pub struct Rules {
    strip: Vec<String>,
    teams: Vec<String>,
    aliases: Vec<(String, String)>,
    labels: Vec<(String, String)>,
    default: Option<String>,
    short_names: bool,
    roles: Vec<(String, String)>,
    coordinator: Option<String>,
    /// Agent name -> task, from `tasks` and `tasks_file`.
    tasks: Vec<(String, String)>,
    /// `teams_file` / `tasks_file` paths that do not exist.
    missing: Vec<String>,
    /// Role prefixes that work across teams (independent QA, security), and
    /// the group they are shown in.
    independent_roles: Vec<String>,
    independent: Option<String>,
}

fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|x| match x {
                    Value::String(s) => Some(s.clone()),
                    Value::Object(_) => x
                        .get("code")
                        .or_else(|| x.get("name"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    _ => None,
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

impl Rules {
    pub fn load(root: &Path) -> Option<Rules> {
        let v: Value = serde_json::from_str(&std::fs::read_to_string(path(root)).ok()?).ok()?;
        Some(Self::from_value(&v))
    }

    pub fn from_value(v: &Value) -> Rules {
        let mut teams = strings(v.get("teams"));
        let mut labels: Vec<(String, String)> = v
            .get("labels")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(file) = v.get("teams_file").and_then(Value::as_str) {
            let extra = std::fs::read_to_string(file)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok());
            if let Some(extra) = extra {
                let mut codes = strings(Some(&extra));
                codes.extend(strings(extra.get("teams")));
                teams.extend(codes);
                let keys: Vec<String> = match v.get("label_key").and_then(Value::as_str) {
                    Some(k) => vec![k.to_string()],
                    None => vec!["label".into(), "title".into(), "displayName".into()],
                };
                let items = extra
                    .as_array()
                    .or_else(|| extra.get("teams").and_then(Value::as_array));
                for it in items.into_iter().flatten() {
                    let code = it
                        .get("code")
                        .or_else(|| it.get("name"))
                        .and_then(Value::as_str);
                    let label = keys.iter().find_map(|k| it.get(k).and_then(Value::as_str));
                    if let (Some(c), Some(l)) = (code, label) {
                        if !labels.iter().any(|(x, _)| x == c) {
                            labels.push((c.to_string(), l.to_string()));
                        }
                    }
                }
            }
        }
        teams.sort();
        teams.dedup();
        let aliases = v
            .get("aliases")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .filter(|(k, v)| !k.is_empty() && !v.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let mut missing = Vec::new();
        let object = |x: Option<&Value>| -> Vec<(String, String)> {
            x.and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut tasks = object(v.get("tasks"));
        if let Some(file) = v.get("tasks_file").and_then(Value::as_str) {
            match std::fs::read_to_string(file)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            {
                Some(extra) => tasks.extend(object(Some(&extra))),
                None => missing.push(file.to_string()),
            }
        }
        if let Some(file) = v.get("teams_file").and_then(Value::as_str) {
            if !Path::new(file).exists() {
                missing.push(file.to_string());
            }
        }
        let roles = v
            .get("roles")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        Rules {
            independent_roles: strings(v.get("independent_roles")),
            independent: v
                .get("independent")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            tasks,
            missing,
            short_names: v
                .get("short_names")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            roles,
            coordinator: v
                .get("coordinator")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            labels,
            aliases,
            strip: strings(v.get("strip")),
            teams,
            default: v
                .get("default")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string),
        }
    }

    /// Every team code it knows, current and retired.
    pub fn codes(&self) -> Vec<String> {
        let mut c = self.teams.clone();
        c.extend(self.aliases.iter().map(|(k, _)| k.clone()));
        c
    }

    /// Merges a fresh suggestion into existing rules without losing anything:
    /// new role prefixes are added, and suggested team codes only when no known
    /// code already covers them (a known code being a segment prefix of them).
    pub fn merge_suggestion(existing: &Value, suggestion: &Value) -> Value {
        let mut out = existing.clone();
        let known = Rules::from_value(existing).codes();
        let covered = |code: &str| {
            known.iter().any(|k| {
                code == k
                    || code.starts_with(&format!("{k}-"))
                    || k.starts_with(&format!("{code}-"))
            })
        };
        let Some(obj) = out.as_object_mut() else {
            return suggestion.clone();
        };
        let mut strip = strings(obj.get("strip"));
        for s in strings(suggestion.get("strip")) {
            if !strip.contains(&s) {
                strip.push(s);
            }
        }
        obj.insert("strip".into(), Value::from(strip));
        let mut teams = strings(obj.get("teams"));
        for t in strings(suggestion.get("teams")) {
            if !covered(&t) && !teams.contains(&t) {
                teams.push(t);
            }
        }
        obj.insert("teams".into(), Value::from(teams));
        for key in ["default", "short_names", "roles"] {
            if !obj.contains_key(key) {
                if let Some(v) = suggestion.get(key) {
                    obj.insert(key.into(), v.clone());
                }
            }
        }
        out
    }

    /// `<role> · <work>` for a name like `<role>-<team>-<work>`.
    pub fn short_name(&self, name: &str, team: &str) -> Option<String> {
        let prefix = self.strip.iter().find(|p| name.starts_with(p.as_str()))?;
        let role = prefix.trim_end_matches('-');
        let rest = &name[prefix.len()..];
        let codes = std::iter::once(team).chain(
            self.aliases
                .iter()
                .filter(|(_, v)| v == team)
                .map(|(k, _)| k.as_str()),
        );
        let work = codes
            .filter_map(|c| rest.strip_prefix(c))
            .map(|w| w.trim_start_matches('-'))
            .find(|w| !w.is_empty())
            .unwrap_or(rest);
        let label = self
            .roles
            .iter()
            .find(|(k, _)| k == role)
            .map(|(_, v)| v.as_str())
            .unwrap_or(role);
        Some(format!("{label} · {work}"))
    }

    pub fn team_of(&self, name: &str) -> Option<String> {
        self.code_of(name).or_else(|| self.default.clone())
    }

    /// The team a name encodes, without falling back to the default.
    pub fn code_of(&self, name: &str) -> Option<String> {
        let rest = self
            .strip
            .iter()
            .find_map(|p| name.strip_prefix(p.as_str()))
            .unwrap_or(name);
        // Current codes and retired ones compete on length; a code only matches
        // a whole name segment ("ag" must not match "agent-...").
        let fits = |code: &str| {
            rest.strip_prefix(code)
                .is_some_and(|after| after.is_empty() || after.starts_with('-'))
        };
        let current = self.teams.iter().map(|t| (t.as_str(), t.as_str()));
        let retired = self.aliases.iter().map(|(k, v)| (k.as_str(), v.as_str()));
        current
            .chain(retired)
            .filter(|(code, _)| fits(code))
            .max_by_key(|(code, _)| code.len())
            .map(|(_, team)| team.to_string())
    }

    pub fn apply(&self, snap: &mut Snapshot) {
        snap.team_labels = self.labels.clone();
        snap.warnings.extend(
            self.missing
                .iter()
                .map(|m| ("warn-teams-file".to_string(), m.clone())),
        );
        let parents: std::collections::HashSet<String> = snap
            .agents
            .iter()
            .filter_map(|a| a.parent.clone())
            .collect();
        for a in snap.agents.iter_mut() {
            if let Some((_, t)) = self.tasks.iter().find(|(n, _)| *n == a.name) {
                a.task = t.clone();
            }
            if a.parent.is_none() && parents.contains(&a.id) {
                a.coordinator = true;
                if let Some(label) = &self.coordinator {
                    if a.team.is_none() {
                        let project = a.name.split('#').next().unwrap_or("").to_string();
                        a.name = format!("{label} · {project}");
                        a.role = label.clone();
                        a.team = Some(label.clone());
                        continue;
                    }
                }
            }
            if a.team.is_none() {
                let independent = self
                    .independent_roles
                    .iter()
                    .any(|p| a.name.starts_with(p.as_str()));
                a.team = match (&self.independent, independent) {
                    (Some(label), true) => Some(label.clone()),
                    _ => self.code_of(&a.name),
                };
            }
            if let Some(prefix) = self.strip.iter().find(|p| a.name.starts_with(p.as_str())) {
                let role = prefix.trim_end_matches('-');
                a.role = self
                    .roles
                    .iter()
                    .find(|(k, _)| k == role)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| role.to_string());
            }
            if self.short_names {
                if let Some(team) = a.team.clone() {
                    if let Some(short) = self.short_name(&a.name, &team) {
                        a.name = short;
                    }
                }
            }
        }
        // Agents whose name says nothing join the team of whoever started them
        // (a team lead's helpers); the rest get the default group.
        for _ in 0..4 {
            let teams: std::collections::HashMap<String, String> = snap
                .agents
                .iter()
                .filter(|a| !a.coordinator)
                .filter_map(|a| Some((a.id.clone(), a.team.clone()?)))
                .collect();
            for a in snap.agents.iter_mut().filter(|a| a.team.is_none()) {
                if let Some(t) = a.parent.as_ref().and_then(|p| teams.get(p)) {
                    a.team = Some(t.clone());
                }
            }
        }
        for a in snap.agents.iter_mut().filter(|a| a.team.is_none()) {
            a.team = self.default.clone();
        }
        for a in snap.agents.iter_mut() {
            if let Some(team) = &a.team {
                if let Some((_, l)) = self.labels.iter().find(|(c, _)| c == team) {
                    a.team_label = l.clone();
                }
            }
        }
    }
}

/// `argus init`: guesses team rules from agent names shaped like
/// `<role>-<team>-<work>`. A role is a first segment shared by several agents;
/// a team is the longest prefix of the rest (up to 3 segments, always leaving a
/// work part) shared by at least two different jobs. (Several roles on the same
/// job prove nothing: developer, tester and QA all share the job name.) Returns `None` when no such pattern shows up.
pub fn infer(names: &[String], default: &str) -> Option<Value> {
    use std::collections::{BTreeMap, BTreeSet};
    let split: Vec<Vec<&str>> = names
        .iter()
        .map(|n| n.split('-').collect::<Vec<_>>())
        .filter(|p| p.len() >= 3 && p.iter().all(|s| !s.is_empty()))
        .collect();
    let mut first: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &split {
        *first.entry(p[0]).or_default() += 1;
    }
    let roles: BTreeSet<&str> = first
        .iter()
        .filter(|(_, n)| **n >= 2)
        .map(|(r, _)| *r)
        .collect();
    // prefix -> (roles, jobs) seen with it
    let mut seen: BTreeMap<String, (BTreeSet<&str>, BTreeSet<String>)> = BTreeMap::new();
    for p in split.iter().filter(|p| roles.contains(p[0])) {
        let rest = &p[1..];
        for k in 1..=rest.len().saturating_sub(1).min(3) {
            let e = seen.entry(rest[..k].join("-")).or_default();
            e.0.insert(p[0]);
            e.1.insert(rest[k..].join("-"));
        }
    }
    let shared = |code: &str| seen.get(code).is_some_and(|(_, jobs)| jobs.len() >= 2);
    let mut teams: BTreeSet<String> = BTreeSet::new();
    for p in split.iter().filter(|p| roles.contains(p[0])) {
        let rest = &p[1..];
        // The prefix with the most distinct jobs; on a tie the longer one
        // ("karar" and "karar-kural" cover the same jobs: the code is
        // "karar-kural"; "kabuk-tasarim" beats "kabuk-tasarim-kurulum" once
        // more jobs show up).
        let best = (1..=rest.len().saturating_sub(1).min(3))
            .map(|k| rest[..k].join("-"))
            .filter(|c| shared(c))
            .max_by_key(|c| (seen.get(c).map_or(0, |(_, j)| j.len()), c.len()));
        if let Some(c) = best {
            teams.insert(c);
        }
    }
    if teams.is_empty() {
        return None;
    }
    let labels: serde_json::Map<String, Value> = roles
        .iter()
        .map(|r| {
            let mut c = r.chars();
            let title = c
                .next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default();
            (r.to_string(), Value::String(title))
        })
        .collect();
    Some(serde_json::json!({
        "strip": roles.iter().map(|r| format!("{r}-")).collect::<Vec<_>>(),
        "teams": teams.into_iter().collect::<Vec<_>>(),
        "default": default,
        "short_names": true,
        "roles": labels,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn infers_roles_and_team_codes() {
        let names: Vec<String> = [
            "gelistirici-karar-kural-liste-alimi",
            "qa-karar-kural-liste-alimi",
            "testci-karar-kural-kanal-esitligi",
            "gelistirici-provizyon-dis-kurum",
            "testci-provizyon-dis-kurum",
            "qa-provizyon-dis-kurum",
            "analist-kurum-agi-provider",
            "mimar-kurum-agi-network-adr",
            "analist-tibbi-listeler-demo",
            "mimar-tibbi-listeler-yukleme",
            "Claimer#1b4b",
        ]
        .map(String::from)
        .to_vec();
        let v = infer(&names, "no-team").unwrap();
        let teams = strings(v.get("teams"));
        assert!(teams.contains(&"karar-kural".to_string()), "{teams:?}");
        // One job only: not enough evidence for a team.
        assert!(
            !teams.iter().any(|t| t.starts_with("provizyon")),
            "{teams:?}"
        );
        assert!(teams.contains(&"kurum-agi".to_string()), "{teams:?}");
        assert!(strings(v.get("strip")).contains(&"qa-".to_string()));
        let r = Rules::from_value(&v);
        assert_eq!(
            r.team_of("qa-karar-kural-yeni-is").as_deref(),
            Some("karar-kural")
        );
        assert!(infer(&["Claimer#1b4b".to_string()], "x").is_none());
    }

    #[test]
    fn suggestions_never_drop_existing_rules() {
        let existing = json!({"strip": ["qa-"], "teams": ["kabuk-tasarim"], "aliases": {"ag": "kurum-agi"}, "labels": {"x": "X"}});
        let suggestion = json!({"strip": ["qa-", "mimar-"], "teams": ["kabuk-tasarim-kurulum", "ag-network", "tazminat"], "short_names": true});
        let m = Rules::merge_suggestion(&existing, &suggestion);
        assert_eq!(strings(m.get("teams")), vec!["kabuk-tasarim", "tazminat"]);
        assert_eq!(strings(m.get("strip")), vec!["qa-", "mimar-"]);
        assert_eq!(m["labels"]["x"], "X");
        assert_eq!(m["short_names"], true);
    }

    #[test]
    fn longest_code_after_role_prefix() {
        let r = Rules::from_value(&json!({
            "strip": ["gelistirici-", "qa-"],
            "teams": ["karar", "karar-kural", {"code": "provizyon"}],
            "default": "ekipsiz"
        }));
        assert_eq!(
            r.team_of("gelistirici-karar-kural-is1").as_deref(),
            Some("karar-kural")
        );
        assert_eq!(r.team_of("qa-provizyon-demo").as_deref(), Some("provizyon"));
        assert_eq!(r.team_of("analist-1").as_deref(), Some("ekipsiz"));
        let r = Rules::from_value(&json!({
            "strip": ["qa-"],
            "teams": ["kurum-agi"],
            "aliases": {"ag": "kurum-agi"}
        }));
        assert_eq!(
            r.team_of("qa-ag-muzakere-demo").as_deref(),
            Some("kurum-agi")
        );
        assert_eq!(r.team_of("qa-agent-x"), None);
        let r = Rules::from_value(&json!({
            "strip": ["gelistirici-"],
            "teams": ["karar-kural"],
            "roles": {"gelistirici": "Geliştirici"},
            "short_names": true
        }));
        assert_eq!(
            r.short_name("gelistirici-karar-kural-liste-alimi", "karar-kural")
                .as_deref(),
            Some("Geliştirici · liste-alimi")
        );
        let r = Rules::from_value(&json!({"teams": ["a"], "labels": {"a": "Team A"}}));
        let mut snap = Snapshot::default();
        r.apply(&mut snap);
        assert_eq!(snap.team_label("a"), Some("Team A"));
        let r = Rules::from_value(&json!({"teams": ["a"]}));
        assert_eq!(r.team_of("zzz"), None);
    }
}
