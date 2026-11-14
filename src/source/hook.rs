//! `argus hook`: a Claude Code `PermissionRequest` hook that lets Argus
//! approve or reject tool calls. The hook writes a request file, waits for a
//! decision file written by Argus (locally or over SSH) and prints Claude
//! Code's decision JSON. If nobody answers in time it prints nothing, so the
//! normal permission prompt is shown.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::{now_millis, Millis};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub ts: Millis,
    pub session_id: String,
    pub cwd: String,
    pub tool: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Decision {
    pub allow: bool,
    #[serde(default)]
    pub message: String,
}

pub fn requests_dir(root: &Path) -> PathBuf {
    root.join("argus").join("requests")
}

pub fn decisions_dir(root: &Path) -> PathBuf {
    root.join("argus").join("decisions")
}

/// Pending requests (newest last).
pub fn pending(root: &Path) -> Vec<Request> {
    let mut out: Vec<Request> = std::fs::read_dir(requests_dir(root))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|t| serde_json::from_str(&t).ok())
        .collect();
    out.sort_by_key(|r: &Request| r.ts);
    out
}

/// Writes a decision for request `id`.
pub fn decide(root: &Path, id: &str, allow: bool) -> std::io::Result<()> {
    let dir = decisions_dir(root);
    std::fs::create_dir_all(&dir)?;
    let d = Decision {
        allow,
        message: if allow {
            String::new()
        } else {
            "Rejected in Argus".into()
        },
    };
    std::fs::write(dir.join(format!("{id}.json")), serde_json::to_vec(&d)?)
}

/// The JSON Claude Code expects from a PermissionRequest hook.
pub fn hook_output(d: &Decision) -> Value {
    let decision = if d.allow {
        json!({ "behavior": "allow" })
    } else {
        json!({ "behavior": "deny", "message": d.message })
    };
    json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": decision } })
}

/// Runs the hook: stdin is Claude Code's PermissionRequest payload.
pub fn run(root: &Path, wait: Duration) -> anyhow::Result<()> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let v: Value = serde_json::from_str(&input).unwrap_or(Value::Null);
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let tool = s("tool_name");
    let target = super::claude::tool_target(&tool, v.get("tool_input").unwrap_or(&Value::Null));
    let id = format!("{}-{}", now_millis(), std::process::id());
    let req = Request {
        id: id.clone(),
        ts: now_millis(),
        session_id: s("session_id"),
        cwd: s("cwd"),
        tool,
        target,
    };
    let rdir = requests_dir(root);
    std::fs::create_dir_all(&rdir)?;
    let rpath = rdir.join(format!("{id}.json"));
    std::fs::write(&rpath, serde_json::to_vec(&req)?)?;
    let dpath = decisions_dir(root).join(format!("{id}.json"));
    let deadline = Instant::now() + wait;
    let mut decision = None;
    while Instant::now() < deadline {
        if let Ok(text) = std::fs::read_to_string(&dpath) {
            decision = serde_json::from_str::<Decision>(&text).ok();
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    let _ = std::fs::remove_file(&rpath);
    let _ = std::fs::remove_file(&dpath);
    if let Some(d) = decision {
        println!("{}", hook_output(&d));
    }
    Ok(())
}

/// Adds the approval hook and the lifecycle event hooks (`argus hook --event`)
/// to `settings.json`, keeping a `.bak` copy. Running it again changes nothing.
pub fn install(root: &Path, wait_secs: u64) -> anyhow::Result<PathBuf> {
    let path = root.join("settings.json");
    let mut settings: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| json!({}));
    if path.exists() {
        std::fs::copy(&path, path.with_extension("json.bak"))?;
    }
    let exe = std::env::current_exe()?.display().to_string();
    let hooks = settings
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("settings.json is not an object"))?
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("`hooks` is not an object"))?;
    let mut add =
        |event: &str, command: String, marker: &str, timeout: u64| -> anyhow::Result<()> {
            let arr = hooks
                .entry(event)
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or_else(|| anyhow::anyhow!("`{event}` is not a list"))?;
            if !arr.iter().any(|e| e.to_string().contains(marker)) {
                let mut entry = json!({
                    "hooks": [{ "type": "command", "command": command, "timeout": timeout }]
                });
                if matches!(event, "PreToolUse" | "PostToolUse" | "PermissionRequest") {
                    entry["matcher"] = json!("*");
                }
                arr.push(entry);
            }
            Ok(())
        };
    add(
        "PermissionRequest",
        format!("\"{exe}\" hook"),
        " hook\"",
        wait_secs + 10,
    )?;
    for event in super::events::HOOK_EVENTS {
        add(event, format!("\"{exe}\" hook --event"), "hook --event", 5)?;
    }
    std::fs::create_dir_all(root)?;
    std::fs::write(&path, serde_json::to_string_pretty(&settings)?)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_matches_claude_code_schema() {
        let allow = hook_output(&Decision {
            allow: true,
            message: String::new(),
        });
        assert_eq!(
            allow["hookSpecificOutput"]["hookEventName"],
            "PermissionRequest"
        );
        assert_eq!(allow["hookSpecificOutput"]["decision"]["behavior"], "allow");
        let deny = hook_output(&Decision {
            allow: false,
            message: "no".into(),
        });
        assert_eq!(deny["hookSpecificOutput"]["decision"]["behavior"], "deny");
        assert_eq!(deny["hookSpecificOutput"]["decision"]["message"], "no");
    }

    #[test]
    fn decisions_round_trip_and_install_is_idempotent() {
        let root = std::env::temp_dir().join(format!("argus-hook-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        decide(&root, "r1", true).unwrap();
        let text = std::fs::read_to_string(decisions_dir(&root).join("r1.json")).unwrap();
        assert!(serde_json::from_str::<Decision>(&text).unwrap().allow);
        install(&root, 120).unwrap();
        install(&root, 120).unwrap();
        let s: Value =
            serde_json::from_str(&std::fs::read_to_string(root.join("settings.json")).unwrap())
                .unwrap();
        // The approval hook and the event hook, once each.
        assert_eq!(s["hooks"]["PermissionRequest"].as_array().unwrap().len(), 2);
        assert_eq!(s["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert!(s["hooks"]["Stop"][0].get("matcher").is_none());
        let _ = std::fs::remove_dir_all(&root);
    }
}
