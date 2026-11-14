//! Remote hosts over SSH. Argus runs `argus collect` on the remote machine
//! through the user's own `ssh` and reads one JSON snapshot per line from
//! its stdout. If the remote has no `argus`, the local binary is uploaded to
//! `~/.cache/argus/argus` when the platforms match.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{actions, Control, Notice, Source};
use crate::model::*;

/// Wire format version of the collect protocol.
pub const PROTOCOL: u32 = 1;

#[derive(Serialize, Deserialize)]
pub struct Frame {
    pub v: u32,
    pub snapshot: Snapshot,
}

/// Remote path used when argus is not on the remote PATH.
const REMOTE_BIN: &str = "~/.cache/argus/argus";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Installing,
    Live,
    Down(String),
}

struct Shared {
    latest: Option<Snapshot>,
    state: LinkState,
    rtt_ms: Option<u32>,
    last_frame: Option<Instant>,
}

pub struct RemoteSource {
    target: String,
    shared: Arc<Mutex<Shared>>,
}

impl RemoteSource {
    pub fn new(target: String) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            latest: None,
            state: LinkState::Connecting,
            rtt_ms: None,
            last_frame: None,
        }));
        let (t, sh) = (target.clone(), shared.clone());
        thread::Builder::new()
            .name(format!("argus-ssh-{target}"))
            .spawn(move || supervise(&t, &sh))
            .expect("spawn ssh thread");
        Self { target, shared }
    }
}

fn set_state(sh: &Arc<Mutex<Shared>>, s: LinkState) {
    if let Ok(mut g) = sh.lock() {
        g.state = s;
    }
}

fn ssh(target: &str) -> Command {
    let mut c = Command::new("ssh");
    c.args([
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=8",
        "-o",
        "ServerAliveInterval=10",
        target,
    ]);
    // Never let ssh read the terminal: it shares the console with the TUI and would
    // swallow the user's key presses. Callers that feed data use `.stdin(piped())`.
    c.stdin(Stdio::null());
    c
}

/// Keeps a collector session alive, reconnecting with back-off.
fn supervise(target: &str, sh: &Arc<Mutex<Shared>>) {
    let mut backoff = Duration::from_secs(2);
    loop {
        set_state(sh, LinkState::Connecting);
        let started = Instant::now();
        let rtt = measure_rtt(target);
        if let Ok(mut g) = sh.lock() {
            g.rtt_ms = rtt;
        }
        match run_collector(target, sh) {
            Ok(()) => set_state(sh, LinkState::Down("collector exited".into())),
            Err(e) => set_state(sh, LinkState::Down(e)),
        }
        if started.elapsed() > Duration::from_secs(60) {
            backoff = Duration::from_secs(2);
        }
        thread::sleep(backoff);
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

fn measure_rtt(target: &str) -> Option<u32> {
    let t0 = Instant::now();
    let ok = ssh(target)
        .arg("true")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?
        .success();
    ok.then(|| t0.elapsed().as_millis().min(u32::MAX as u128) as u32)
}

/// Places where users typically install argus on a remote machine. Non-interactive
/// SSH shells often lack `~/.local/bin` or Homebrew on `PATH`, so look there too.
const CANDIDATES: &[&str] = &[
    "~/.cache/argus/argus",
    "~/.local/bin/argus",
    "~/.cargo/bin/argus",
    "/usr/local/bin/argus",
    "/opt/homebrew/bin/argus",
    "/home/linuxbrew/.linuxbrew/bin/argus",
];

/// Shell script that prints `BIN=<path>` for the first remote argus of exactly
/// this version (if any) and `OS=<uname -sm>`. An older remote collector is
/// never reused: it may not understand newer Claude Code data, so it gets replaced.
pub fn probe_script() -> String {
    let list = CANDIDATES.join(" ");
    let version = env!("CARGO_PKG_VERSION");
    format!(
        "for p in \"$(command -v argus 2>/dev/null)\" {list}; do \
         if [ -n \"$p\" ] && [ -x \"$p\" ] && [ \"$(\"$p\" --version 2>/dev/null)\" = 'argus {version}' ]; then echo \"BIN=$p\"; break; fi; \
         done; echo \"OS=$(uname -sm)\""
    )
}

/// Parses the probe output into (binary, platform).
pub fn parse_probe(out: &str) -> (Option<String>, String) {
    let mut bin = None;
    let mut os = String::new();
    for line in out.lines() {
        if let Some(b) = line.trim().strip_prefix("BIN=") {
            if !b.is_empty() {
                bin = Some(b.to_string());
            }
        } else if let Some(o) = line.trim().strip_prefix("OS=") {
            os = o.to_string();
        }
    }
    (bin, os)
}

/// Finds a usable remote argus, installing it if possible.
fn remote_binary(target: &str, sh: &Arc<Mutex<Shared>>) -> Result<String, String> {
    let out = ssh(target)
        .arg(sh_c(&probe_script()))
        .output()
        .map_err(|e| format!("ssh: {e}"))?;
    if !out.status.success() && out.stdout.is_empty() {
        return Err(first_line(&String::from_utf8_lossy(&out.stderr))
            .unwrap_or_else(|| "ssh failed".into()));
    }
    let (found, platform) = parse_probe(&String::from_utf8_lossy(&out.stdout));
    if let Some(bin) = found {
        return Ok(bin);
    }
    let force_download = std::env::var_os("ARGUS_FORCE_REMOTE_DOWNLOAD").is_some();
    if force_download || !platform_matches(&platform) {
        // Different OS/CPU (e.g. Windows -> Linux): the remote fetches its own build.
        set_state(sh, LinkState::Installing);
        let out = ssh(target)
            .arg(sh_c(&download_script()))
            .output()
            .map_err(|e| format!("ssh: {e}"))?;
        if out.status.success() {
            return Ok(REMOTE_BIN.to_string());
        }
        let os = if platform.is_empty() {
            "unknown system".to_string()
        } else {
            platform
        };
        let why = first_line(&String::from_utf8_lossy(&out.stdout))
            .or_else(|| first_line(&String::from_utf8_lossy(&out.stderr)))
            .unwrap_or_default();
        return Err(format!(
            "could not install argus on remote ({os}) {why}: see INSTALL.md §4"
        ));
    }
    set_state(sh, LinkState::Installing);
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&exe).map_err(|e| e.to_string())?;
    let mut child = ssh(target)
        .arg(format!("mkdir -p ~/.cache/argus && cat > {REMOTE_BIN}.tmp && chmod +x {REMOTE_BIN}.tmp && mv {REMOTE_BIN}.tmp {REMOTE_BIN}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .ok_or("no stdin")?
        .write_all(&bytes)
        .map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        Ok(REMOTE_BIN.to_string())
    } else {
        Err("upload failed".into())
    }
}

/// Runs `script` with a POSIX shell whatever the remote login shell is.
fn sh_c(script: &str) -> String {
    format!("sh -c {}", super::actions::sh_quote(script))
}

/// Shell script run on the remote: downloads the matching release build into
/// `~/.cache/argus/argus` (this version first, else the newest release,
/// pre-releases included).
pub fn download_script() -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(
        r#"set -e
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64|Linux-amd64) t=x86_64-unknown-linux-gnu ;;
  Linux-aarch64|Linux-arm64) t=aarch64-unknown-linux-gnu ;;
  Darwin-x86_64) t=x86_64-apple-darwin ;;
  Darwin-arm64) t=aarch64-apple-darwin ;;
  *) echo "unsupported platform $(uname -sm)"; exit 3 ;;
esac
get() {{ if command -v curl >/dev/null 2>&1; then curl -fsSL "$1"; elif command -v wget >/dev/null 2>&1; then wget -qO- "$1"; else echo "needs curl or wget"; exit 4; fi; }}
base=https://github.com/serkankoyun/argus/releases
tmp=$(mktemp -d)
v=v{version}
if ! get "$base/download/$v/argus-$v-$t.tar.gz" > "$tmp/a.tgz" 2>/dev/null; then
  v=$(get 'https://api.github.com/repos/serkankoyun/argus/releases?per_page=1' | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
  [ -n "$v" ] || {{ echo "no release found"; exit 5; }}
  get "$base/download/$v/argus-$v-$t.tar.gz" > "$tmp/a.tgz"
fi
tar xzf "$tmp/a.tgz" -C "$tmp"
mkdir -p "$HOME/.cache/argus"
mv "$tmp"/argus-*/argus "$HOME/.cache/argus/argus.tmp"
chmod +x "$HOME/.cache/argus/argus.tmp"
mv "$HOME/.cache/argus/argus.tmp" "$HOME/.cache/argus/argus"
rm -rf "$tmp"
"$HOME/.cache/argus/argus" --version"#
    )
}

/// `uname -sm` output vs. the platform this binary was built for.
pub fn platform_matches(uname: &str) -> bool {
    let mut parts = uname.split_whitespace();
    let os = parts.next().unwrap_or("").to_lowercase();
    let arch = parts.next().unwrap_or("").to_lowercase();
    let os_ok = match std::env::consts::OS {
        "linux" => os == "linux",
        "macos" => os == "darwin",
        _ => false,
    };
    let arch_ok = match std::env::consts::ARCH {
        "x86_64" => arch == "x86_64" || arch == "amd64",
        "aarch64" => arch == "aarch64" || arch == "arm64",
        other => arch == other,
    };
    os_ok && arch_ok
}

fn first_line(s: &str) -> Option<String> {
    s.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_string)
}

fn run_collector(target: &str, sh: &Arc<Mutex<Shared>>) -> Result<(), String> {
    let bin = remote_binary(target, sh)?;
    let mut child: Child = ssh(target)
        .arg(format!("{bin} collect"))
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("ssh: {e}"))?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|e| e.to_string())?;
        match serde_json::from_str::<Frame>(&line) {
            Ok(f) if f.v == PROTOCOL => {
                if let Ok(mut g) = sh.lock() {
                    g.latest = Some(f.snapshot);
                    g.state = LinkState::Live;
                    g.last_frame = Some(Instant::now());
                }
            }
            Ok(f) => {
                let _ = child.kill();
                return Err(format!(
                    "protocol v{} (local v{PROTOCOL}); update argus on the remote",
                    f.v
                ));
            }
            Err(_) => continue,
        }
    }
    let _ = child.wait();
    Ok(())
}

impl Source for RemoteSource {
    fn poll(&mut self) -> Snapshot {
        let g = match self.shared.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let stale = g
            .last_frame
            .is_none_or(|t| t.elapsed() > Duration::from_secs(10));
        let mut snap = g.latest.clone().unwrap_or_default();
        let connected = g.state == LinkState::Live && !stale;
        let region = match &g.state {
            LinkState::Live => "ssh".to_string(),
            LinkState::Connecting => "connecting…".to_string(),
            LinkState::Installing => "installing…".to_string(),
            LinkState::Down(why) => why.clone(),
        };
        if snap.hosts.is_empty() {
            snap.hosts.push(Host {
                name: self.target.clone(),
                region: String::new(),
                connected: false,
                cpu_pct: 0,
                mem_pct: 0,
                rtt_ms: None,
            });
        }
        for h in &mut snap.hosts {
            h.connected = connected;
            h.rtt_ms = g.rtt_ms;
            h.region = region.clone();
        }
        snap
    }

    fn owns(&self, agent: &str) -> bool {
        self.shared
            .lock()
            .ok()
            .and_then(|g| g.latest.as_ref().map(|s| s.agent(agent).is_some()))
            .unwrap_or(false)
    }

    fn host(&self) -> Option<String> {
        let name = self.shared.lock().ok().and_then(|g| {
            g.latest
                .as_ref()
                .and_then(|s| s.hosts.first().map(|h| h.name.clone()))
        });
        Some(name.unwrap_or_else(|| self.target.clone()))
    }

    fn control(&mut self, c: &Control) -> Notice {
        match c {
            Control::Decide { request, allow, .. } => {
                let body = serde_json::json!({ "allow": allow, "message": if *allow { "" } else { "Rejected in Argus" } }).to_string();
                let script = format!(
                    "mkdir -p ~/.claude/argus/decisions && cat > ~/.claude/argus/decisions/{}.json",
                    actions::sh_quote(request).trim_matches('\'')
                );
                let mut child = ssh(&self.target)
                    .arg(script)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                child
                    .stdin
                    .take()
                    .ok_or("no stdin")?
                    .write_all(body.as_bytes())
                    .map_err(|e| e.to_string())?;
                let ok = child.wait().map_err(|e| e.to_string())?.success();
                if ok {
                    Ok(format!(
                        "{} on {}",
                        if *allow { "approved" } else { "rejected" },
                        self.target
                    ))
                } else {
                    Err("could not deliver the decision over ssh".into())
                }
            }
            Control::Run { dir, prompt, .. } => {
                actions::run_remote(&self.target, dir, &["-p", prompt])
            }
            Control::Resume { agent, text } => {
                let cwd = self.shared.lock().ok().and_then(|g| {
                    g.latest
                        .as_ref()
                        .and_then(|s| s.agent(agent).map(|a| a.cwd.clone()))
                });
                match cwd {
                    Some(cwd) => {
                        actions::run_remote(&self.target, &cwd, &["-p", "--resume", agent, text])
                    }
                    None => Err("unknown agent".into()),
                }
            }
            Control::AddHost(_) => Err("not supported by this data source".into()),
        }
    }
}

/// `argus collect`: streams local snapshots as JSON lines until stdout closes.
pub fn collect(mut source: Box<dyn Source>, every: Duration) -> anyhow::Result<()> {
    let stdout = std::io::stdout();
    loop {
        let frame = Frame {
            v: PROTOCOL,
            snapshot: source.poll(),
        };
        let mut out = stdout.lock();
        serde_json::to_writer(&mut out, &frame)?;
        out.write_all(b"\n")?;
        out.flush()?;
        drop(out);
        thread::sleep(every);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_round_trip() {
        let mut s = Snapshot {
            source: "x".into(),
            ..Default::default()
        };
        s.metrics.tokens_per_sec.push(3.0);
        let line = serde_json::to_string(&Frame {
            v: PROTOCOL,
            snapshot: s,
        })
        .unwrap();
        let back: Frame = serde_json::from_str(&line).unwrap();
        assert_eq!(back.snapshot.source, "x");
        assert_eq!(back.snapshot.metrics.tokens_per_sec.last(), 3.0);
    }

    #[test]
    fn matches_platforms() {
        let here = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => Some("Linux x86_64"),
            ("macos", "aarch64") => Some("Darwin arm64"),
            _ => None,
        };
        if let Some(u) = here {
            assert!(platform_matches(u));
        }
        assert!(!platform_matches("Plan9 mips"));
    }

    #[test]
    fn parses_probe_output() {
        let (bin, os) = parse_probe("BIN=/home/me/.local/bin/argus\nOS=Linux x86_64\n");
        assert_eq!(bin.as_deref(), Some("/home/me/.local/bin/argus"));
        assert_eq!(os, "Linux x86_64");
        let (bin, os) = parse_probe("OS=Linux aarch64\n");
        assert!(bin.is_none());
        assert_eq!(os, "Linux aarch64");
    }

    #[test]
    fn download_script_covers_platforms_and_version() {
        let s = download_script();
        for t in [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-apple-darwin",
            "aarch64-apple-darwin",
        ] {
            assert!(s.contains(t), "missing {t}");
        }
        assert!(s.contains(&format!("v={}", ["v", env!("CARGO_PKG_VERSION")].concat())));
        assert!(sh_c("echo 'x'").starts_with("sh -c '"));
    }

    #[test]
    fn probe_checks_common_install_paths() {
        let s = probe_script();
        assert!(s.contains("~/.local/bin/argus") && s.contains("~/.cache/argus/argus"));
        assert!(s.contains("command -v argus"));
        assert!(s.contains(&format!("'argus {}'", env!("CARGO_PKG_VERSION"))));
    }
}
