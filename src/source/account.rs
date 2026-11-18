//! The Claude account a machine is signed in with, read from Claude Code's
//! `.claude.json` (`<claude dir>/.claude.json` when `CLAUDE_CONFIG_DIR` or
//! `--claude-dir` points there, else `~/.claude.json`).
//!
//! Privacy: only the organization name, a plan label and a masked email leave
//! this module. The email is masked here, at the source, so the raw address
//! never reaches a snapshot, `argus snapshot` output, logs or the SSH stream.
//! Tokens and every other field of the file are never deserialized.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::Deserialize;

use crate::model::Account;

/// The file is rewritten often and can be several MB: look at most this often.
const REFRESH: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    /// Kept as a loose value so an unexpected field type degrades, not fails.
    #[serde(default)]
    oauth_account: Option<serde_json::Value>,
}

/// Account from the text of a `.claude.json`. `None` when the text is not a
/// JSON object; an empty account when nobody is signed in (API key use).
pub fn parse(text: &str) -> Option<Account> {
    let file: ConfigFile = serde_json::from_str(text).ok()?;
    let Some(serde_json::Value::Object(o)) = file.oauth_account else {
        return Some(Account::default());
    };
    let field = |k: &str| o.get(k).and_then(|v| v.as_str()).unwrap_or("").trim();
    Some(Account {
        org: field("organizationName").to_string(),
        plan: plan_label(field("organizationType")),
        email: mask_email(field("emailAddress")),
    })
}

/// `someone@example.com` becomes `s***@example.com`: the first character of the
/// local part, `***`, then the domain. Without `@` only the first character
/// stays. Masking a masked address returns it unchanged.
pub fn mask_email(raw: &str) -> String {
    let raw = raw.trim();
    if raw.is_empty() {
        return String::new();
    }
    let (local, domain) = match raw.rsplit_once('@') {
        Some((l, d)) => (l, Some(d)),
        None => (raw, None),
    };
    let first: String = local.chars().take(1).collect();
    match domain {
        Some(d) => format!("{first}***@{d}"),
        None => format!("{first}***"),
    }
}

/// Readable plan from Claude Code's `organizationType`.
pub fn plan_label(kind: &str) -> String {
    match kind.trim() {
        "claude_team" => "Team".into(),
        "claude_max" => "Max".into(),
        "claude_pro" => "Pro".into(),
        "claude_enterprise" => "Enterprise".into(),
        other => other
            .split(['_', '-', ' '])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                match c.next() {
                    Some(f) => f
                        .to_uppercase()
                        .chain(c.flat_map(char::to_lowercase))
                        .collect(),
                    None => String::new(),
                }
            })
            .collect::<Vec<String>>()
            .join(" "),
    }
}

/// Where Claude Code keeps `.claude.json`, most specific first.
pub fn candidates(claude_dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![claude_dir.join(".claude.json")];
    if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        let p = PathBuf::from(home).join(".claude.json");
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Caches the account; the file is checked at most every [`REFRESH`] and
/// parsed again only when its modification time or size changed.
pub struct AccountReader {
    paths: Vec<PathBuf>,
    refresh: Duration,
    checked: Option<Instant>,
    stamp: Option<(PathBuf, Option<SystemTime>, u64)>,
    cached: Option<Account>,
}

impl AccountReader {
    pub fn new(claude_dir: &Path) -> Self {
        Self::with_paths(candidates(claude_dir), REFRESH)
    }

    pub fn with_paths(paths: Vec<PathBuf>, refresh: Duration) -> Self {
        Self {
            paths,
            refresh,
            checked: None,
            stamp: None,
            cached: None,
        }
    }

    pub fn read(&mut self) -> Option<Account> {
        if self.checked.is_some_and(|t| t.elapsed() < self.refresh) {
            return self.cached.clone();
        }
        self.checked = Some(Instant::now());
        let found = self.paths.iter().find_map(|p| {
            let m = std::fs::metadata(p).ok().filter(|m| m.is_file())?;
            Some((p.clone(), m.modified().ok(), m.len()))
        });
        match found {
            None => {
                self.stamp = None;
                self.cached = None;
            }
            Some(stamp) if self.stamp.as_ref() == Some(&stamp) => {}
            Some(stamp) => {
                let fresh = std::fs::read_to_string(&stamp.0)
                    .ok()
                    .and_then(|t| parse(&t));
                // A half-written file keeps the last good value of the same
                // file; a file that never parsed stays `None`.
                let same_file = self.stamp.as_ref().map(|s| &s.0) == Some(&stamp.0);
                if fresh.is_some() || !same_file {
                    self.cached = fresh;
                }
                self.stamp = Some(stamp);
            }
        }
        self.cached.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIGNED_IN: &str = r#"{
        "numStartups": 12,
        "oauthAccount": {
            "accountUuid": "00000000-0000-0000-0000-000000000000",
            "emailAddress": "someone@example.com",
            "organizationName": "Acme",
            "organizationType": "claude_team",
            "seatTier": "standard",
            "billingType": "stripe_subscription"
        },
        "projects": {"/tmp/x": {"allowedTools": []}}
    }"#;

    #[test]
    fn parses_signed_in_account_and_masks_the_email() {
        let a = parse(SIGNED_IN).unwrap();
        assert_eq!(a.org, "Acme");
        assert_eq!(a.plan, "Team");
        assert_eq!(a.email, "s***@example.com");
        assert!(a.signed_in());
    }

    #[test]
    fn no_oauth_account_means_not_signed_in() {
        for text in [
            r#"{"numStartups": 3}"#,
            r#"{"oauthAccount": null}"#,
            r#"{"oauthAccount": "odd"}"#,
        ] {
            let a = parse(text).unwrap();
            assert!(!a.signed_in(), "{text}");
        }
    }

    #[test]
    fn malformed_json_is_none_without_panicking() {
        for text in [
            "",
            "{",
            "not json",
            "[1, 2]",
            "{\"oauthAccount\": {",
            "null",
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }

    #[test]
    fn unexpected_field_types_degrade() {
        let text = r#"{"oauthAccount": {"organizationName": 7, "emailAddress": "x@example.com"}}"#;
        let a = parse(text).unwrap();
        assert_eq!(a.org, "");
        assert_eq!(a.email, "x***@example.com");
    }

    #[test]
    fn masks_emails() {
        assert_eq!(mask_email("someone@example.com"), "s***@example.com");
        assert_eq!(mask_email("a@example.com"), "a***@example.com");
        assert_eq!(mask_email("@example.com"), "***@example.com");
        assert_eq!(mask_email("plain"), "p***");
        assert_eq!(mask_email(""), "");
        assert_eq!(mask_email("   "), "");
        assert_eq!(mask_email("a@b@example.com"), "a***@example.com");
        assert_eq!(mask_email("\u{f6}mer@example.com"), "\u{f6}***@example.com");
        // Idempotent: a remote's masked address stays as it is.
        assert_eq!(mask_email("s***@example.com"), "s***@example.com");
    }

    #[test]
    fn maps_plans() {
        assert_eq!(plan_label("claude_team"), "Team");
        assert_eq!(plan_label("claude_max"), "Max");
        assert_eq!(plan_label("claude_pro"), "Pro");
        assert_eq!(plan_label("claude_enterprise"), "Enterprise");
        assert_eq!(plan_label("claude_free_trial"), "Claude Free Trial");
        assert_eq!(plan_label("API"), "Api");
        assert_eq!(plan_label(""), "");
    }

    fn temp(name: &str) -> PathBuf {
        let d =
            std::env::temp_dir().join(format!("argus-account-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn reader_prefers_the_claude_dir_and_follows_changes() {
        let dir = temp("reader");
        let inner = dir.join("cfg").join(".claude.json");
        let home = dir.join(".claude.json");
        std::fs::create_dir_all(dir.join("cfg")).unwrap();
        let mut r = AccountReader::with_paths(vec![inner.clone(), home.clone()], Duration::ZERO);
        assert_eq!(r.read(), None);
        std::fs::write(&home, r#"{"oauthAccount": {"organizationName": "Home"}}"#).unwrap();
        assert_eq!(r.read().unwrap().org, "Home");
        std::fs::write(&inner, SIGNED_IN).unwrap();
        assert_eq!(r.read().unwrap().org, "Acme");
        // A signed-out file (different size) is picked up.
        std::fs::write(&inner, r#"{"numStartups": 1}"#).unwrap();
        assert!(!r.read().unwrap().signed_in());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reader_caches_between_refreshes() {
        let dir = temp("cache");
        let f = dir.join(".claude.json");
        std::fs::write(&f, SIGNED_IN).unwrap();
        let mut r = AccountReader::with_paths(vec![f.clone()], Duration::from_secs(3600));
        assert_eq!(r.read().unwrap().org, "Acme");
        std::fs::remove_file(&f).unwrap();
        assert_eq!(r.read().unwrap().org, "Acme");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn half_written_file_keeps_the_last_account() {
        let dir = temp("partial");
        let f = dir.join(".claude.json");
        std::fs::write(&f, SIGNED_IN).unwrap();
        let mut r = AccountReader::with_paths(vec![f.clone()], Duration::ZERO);
        assert_eq!(r.read().unwrap().org, "Acme");
        std::fs::write(&f, "{\"oauthAccount\": {\"organiz").unwrap();
        assert_eq!(r.read().unwrap().org, "Acme");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn raw_email_never_reaches_snapshot_json() {
        let dir = temp("snapshot");
        let f = dir.join(".claude.json");
        std::fs::write(&f, SIGNED_IN).unwrap();
        let mut r = AccountReader::with_paths(vec![f], Duration::ZERO);
        let snap = crate::model::Snapshot {
            hosts: vec![crate::model::Host {
                name: "h".into(),
                region: "local".into(),
                connected: true,
                cpu_pct: 0,
                mem_pct: 0,
                rtt_ms: None,
                account: r.read(),
            }],
            ..Default::default()
        };
        let json = serde_json::to_string(&snap).unwrap();
        assert!(json.contains("s***@example.com"), "{json}");
        assert!(!json.contains("someone"), "{json}");
        assert!(!json.contains("accountUuid") && !json.contains("billingType"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hosts_without_account_still_deserialize() {
        let old =
            r#"{"name":"h","region":"ssh","connected":true,"cpu_pct":1,"mem_pct":2,"rtt_ms":null}"#;
        let h: crate::model::Host = serde_json::from_str(old).unwrap();
        assert_eq!(h.account, None);
    }
}
