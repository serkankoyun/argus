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
//!   "default": "no-team"
//! }
//! ```
//! An agent's name loses the first matching `strip` prefix; the rest is matched
//! against the team codes (longest prefix wins). Codes come from `teams` and,
//! optionally, from `teams_file`: a JSON list of strings, or objects with a
//! `code` (or `name`), at the top level or under `teams`. Agents that match no
//! code get `default` when it is set. `aliases` maps retired codes to current
//! ones, so agents from before a rename join the renamed team. Agents that
//! already have a team keep it.

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
        if let Some(file) = v.get("teams_file").and_then(Value::as_str) {
            let extra = std::fs::read_to_string(file)
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok());
            if let Some(extra) = extra {
                let mut codes = strings(Some(&extra));
                codes.extend(strings(extra.get("teams")));
                teams.extend(codes);
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
        for a in snap.agents.iter_mut() {
            if a.team.is_none() {
                a.team = self.team_of(&a.name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let r = Rules::from_value(&json!({"teams": ["a"]}));
        assert_eq!(r.team_of("zzz"), None);
    }
}
