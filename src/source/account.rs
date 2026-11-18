//! The Claude account a machine is signed in with, read from Claude Code's
//! `.claude.json`: `<claude dir>/.claude.json`, and `~/.claude.json` as well
//! when the claude dir is the default `~/.claude`.
//!
//! Privacy: only the organization name, a plan label and a masked email leave
//! this module. Every email-shaped text is masked here, at the source (also
//! inside the organization name, e.g. "someone@example.com's Organization"),
//! so a raw address never reaches a snapshot, `argus snapshot` output, logs or
//! the SSH stream. Only `organizationName`, `organizationType` and
//! `emailAddress` are deserialized; tokens, names, ids and every other field
//! of the file are skipped without being stored.

use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::de::{self, Deserializer, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;

use crate::model::Account;

/// The file is rewritten often and can be several MB: look at most this often.
const REFRESH: Duration = Duration::from_secs(60);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    #[serde(default, deserialize_with = "oauth_or_none")]
    oauth_account: Option<OauthAccount>,
}

/// The three fields Argus uses; any other key is skipped by serde.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct OauthAccount {
    #[serde(deserialize_with = "string_or_none")]
    organization_name: Option<String>,
    #[serde(deserialize_with = "string_or_none")]
    organization_type: Option<String>,
    #[serde(deserialize_with = "string_or_none")]
    email_address: Option<String>,
}

/// Skips the rest of a sequence or map without storing it.
fn skip_seq<'de, A: SeqAccess<'de>>(mut seq: A) -> Result<(), A::Error> {
    while seq.next_element::<IgnoredAny>()?.is_some() {}
    Ok(())
}

fn skip_map<'de, A: MapAccess<'de>>(mut map: A) -> Result<(), A::Error> {
    while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
    Ok(())
}

/// A string, or `None` for any other JSON type (lenient, never an error).
fn string_or_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<String>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("any JSON value")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Some(v.to_string()))
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
            skip_seq(seq).map(|_| None)
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            skip_map(map).map(|_| None)
        }
    }
    d.deserialize_any(V)
}

/// The `oauthAccount` object, or `None` for any other JSON type.
fn oauth_or_none<'de, D: Deserializer<'de>>(d: D) -> Result<Option<OauthAccount>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Option<OauthAccount>;
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("any JSON value")
        }
        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            OauthAccount::deserialize(de::value::MapAccessDeserializer::new(map)).map(Some)
        }
        fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_f64<E: de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
            skip_seq(seq).map(|_| None)
        }
    }
    d.deserialize_any(V)
}

/// Account from the text of a `.claude.json`. `None` when the text is not a
/// JSON object; an empty account when nobody is signed in (API key use).
pub fn parse(text: &str) -> Option<Account> {
    let file: ConfigFile = serde_json::from_str(text).ok()?;
    let Some(o) = file.oauth_account else {
        return Some(Account::default());
    };
    let field = |v: &Option<String>| v.as_deref().unwrap_or("").trim().to_string();
    let mut account = Account {
        org: field(&o.organization_name),
        plan: plan_label(&field(&o.organization_type)),
        email: field(&o.email_address),
    };
    sanitize(&mut account);
    Some(account)
}

/// Masks every email-shaped text in an account: the whole `email` field and
/// any `local@domain` inside the organization or plan. Idempotent, so it runs
/// both where the file is read and where a remote frame arrives.
pub fn sanitize(a: &mut Account) {
    a.org = mask_emails_in(&a.org);
    a.plan = mask_emails_in(&a.plan);
    a.email = mask_email(&a.email);
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

/// Masks each `local@domain` token inside free text with [`mask_email`]:
/// "someone@example.com's Organization" becomes "s***@example.com's
/// Organization". Text without such a token is returned unchanged.
pub fn mask_emails_in(text: &str) -> String {
    // An apostrophe is valid in a local part (o'brien@...); after the domain
    // it ends the token, so "x@example.com's" keeps its "'s".
    let local_ok = |c: char| c.is_alphanumeric() || "._%+-*'".contains(c);
    let domain_ok = |c: char| c.is_alphanumeric() || ".-".contains(c);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut done, mut j) = (0, 0);
    while j < chars.len() {
        if chars[j] == '@' {
            let mut start = j;
            while start > done && local_ok(chars[start - 1]) {
                start -= 1;
            }
            let mut end = j + 1;
            while end < chars.len() && domain_ok(chars[end]) {
                end += 1;
            }
            // A sentence may end right after the address.
            while end > j + 1 && matches!(chars[end - 1], '.' | '-') {
                end -= 1;
            }
            if start < j && end > j + 1 {
                out.extend(&chars[done..start]);
                let token: String = chars[start..end].iter().collect();
                out.push_str(&mask_email(&token));
                done = end;
                j = end;
                continue;
            }
        }
        j += 1;
    }
    out.extend(&chars[done..]);
    out
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
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from);
    candidates_with_home(claude_dir, home.as_deref())
}

/// `<claude dir>/.claude.json`; `<home>/.claude.json` too, but only when the
/// claude dir is the default `<home>/.claude`. With `CLAUDE_CONFIG_DIR` or
/// `--claude-dir` Claude Code keeps the file in that dir, and the home file
/// may belong to another login.
pub fn candidates_with_home(claude_dir: &Path, home: Option<&Path>) -> Vec<PathBuf> {
    let mut out = vec![claude_dir.join(".claude.json")];
    if let Some(home) = home {
        if claude_dir == home.join(".claude") {
            out.push(home.join(".claude.json"));
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

    /// Individual accounts name their organization after the email.
    const PERSONAL_ORG: &str = r#"{
        "oauthAccount": {
            "emailAddress": "someone@example.com",
            "organizationName": "someone@example.com's Organization",
            "organizationType": "claude_max",
            "displayName": "Some One",
            "organizationUuid": "11111111-1111-1111-1111-111111111111"
        }
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
    fn masks_an_email_inside_the_organization_name() {
        let a = parse(PERSONAL_ORG).unwrap();
        assert_eq!(a.org, "s***@example.com's Organization");
        assert_eq!(a.plan, "Max");
        assert_eq!(a.email, "s***@example.com");
    }

    #[test]
    fn no_oauth_account_means_not_signed_in() {
        for text in [
            r#"{"numStartups": 3}"#,
            r#"{"oauthAccount": null}"#,
            r#"{"oauthAccount": "odd"}"#,
            r#"{"oauthAccount": [1, {"a": 2}]}"#,
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
        let text = r#"{"oauthAccount": {"organizationName": 7, "organizationType": {"x": [1]},
            "emailAddress": "x@example.com", "fullName": ["a", "b"]}}"#;
        let a = parse(text).unwrap();
        assert_eq!(a.org, "");
        assert_eq!(a.plan, "");
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
    fn masks_emails_inside_text() {
        let m = mask_emails_in;
        assert_eq!(
            m("someone@example.com's Organization"),
            "s***@example.com's Organization"
        );
        assert_eq!(
            m("Team of a.b+c@example.com and x@example.org."),
            "Team of a***@example.com and x***@example.org."
        );
        assert_eq!(m("(someone@example.com)"), "(s***@example.com)");
        assert_eq!(m("o'brien@example.com"), "o***@example.com");
        assert_eq!(
            m("o'brien@example.com's Organization"),
            "o***@example.com's Organization"
        );
        assert_eq!(m("Acme"), "Acme");
        assert_eq!(m("Acme @ HQ"), "Acme @ HQ");
        assert_eq!(m("@example.com"), "@example.com");
        assert_eq!(m(""), "");
        // Idempotent.
        let once = m("someone@example.com's Organization");
        assert_eq!(m(&once), once);
    }

    #[test]
    fn sanitize_masks_every_field() {
        let mut a = Account {
            org: "someone@example.com's Organization".into(),
            plan: "Seat for other@example.com".into(),
            email: "someone@example.com".into(),
        };
        sanitize(&mut a);
        let all = format!("{} {} {}", a.org, a.plan, a.email);
        assert!(
            !all.contains("someone@") && !all.contains("other@"),
            "{all}"
        );
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

    #[test]
    fn home_file_only_with_the_default_claude_dir() {
        let home = Path::new("/home/u");
        assert_eq!(
            candidates_with_home(&home.join(".claude"), Some(home)),
            vec![home.join(".claude/.claude.json"), home.join(".claude.json")]
        );
        assert_eq!(
            candidates_with_home(Path::new("/srv/cfg"), Some(home)),
            vec![Path::new("/srv/cfg").join(".claude.json")]
        );
        assert_eq!(
            candidates_with_home(Path::new("/srv/cfg"), None),
            vec![Path::new("/srv/cfg").join(".claude.json")]
        );
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
    fn raw_email_never_reaches_snapshot_or_frame_json() {
        for (name, text) in [("plain", SIGNED_IN), ("personal", PERSONAL_ORG)] {
            let dir = temp(&format!("snapshot-{name}"));
            let f = dir.join(".claude.json");
            std::fs::write(&f, text).unwrap();
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
            let frame = serde_json::to_string(&crate::source::remote::Frame {
                v: crate::source::remote::PROTOCOL,
                snapshot: snap,
            })
            .unwrap();
            for out in [&json, &frame] {
                assert!(out.contains("s***@example.com"), "{out}");
                assert!(!out.contains("someone"), "{out}");
                assert!(!out.contains("accountUuid") && !out.contains("billingType"));
                assert!(!out.contains("Some One") && !out.contains("11111111"));
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn hosts_without_account_still_deserialize() {
        let old =
            r#"{"name":"h","region":"ssh","connected":true,"cpu_pct":1,"mem_pct":2,"rtt_ms":null}"#;
        let h: crate::model::Host = serde_json::from_str(old).unwrap();
        assert_eq!(h.account, None);
    }
}
