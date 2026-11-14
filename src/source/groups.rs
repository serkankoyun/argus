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
//! `displayName`).

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
        Rules {
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

    pub fn team_of(&self, name: &str) -> Option<String> {
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
            .or_else(|| self.default.clone())
    }

    pub fn apply(&self, snap: &mut Snapshot) {
        snap.team_labels = self.labels.clone();
        for a in snap.agents.iter_mut() {
            if a.team.is_none() {
                a.team = self.team_of(&a.name);
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
        let best = (1..=rest.len().saturating_sub(1).min(3))
            .rev()
            .map(|k| rest[..k].join("-"))
            .find(|c| shared(c));
        if let Some(c) = best {
            teams.insert(c);
        }
    }
    if teams.is_empty() {
        return None;
    }
    Some(serde_json::json!({
        "strip": roles.iter().map(|r| format!("{r}-")).collect::<Vec<_>>(),
        "teams": teams.into_iter().collect::<Vec<_>>(),
        "default": default,
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
        let r = Rules::from_value(&json!({"teams": ["a"], "labels": {"a": "Team A"}}));
        let mut snap = Snapshot::default();
        r.apply(&mut snap);
        assert_eq!(snap.team_label("a"), Some("Team A"));
        let r = Rules::from_value(&json!({"teams": ["a"]}));
        assert_eq!(r.team_of("zzz"), None);
    }
}
