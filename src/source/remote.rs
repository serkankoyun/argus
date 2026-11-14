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

use super::Source;
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

/// Finds a usable remote argus, installing it if possible.
fn remote_binary(target: &str, sh: &Arc<Mutex<Shared>>) -> Result<String, String> {
    let probe = format!("command -v argus || (test -x {REMOTE_BIN} && echo {REMOTE_BIN}) || echo MISSING; uname -sm");
    let out = ssh(target)
        .arg(probe)
        .output()
        .map_err(|e| format!("ssh: {e}"))?;
    if !out.status.success() {
        return Err(first_line(&String::from_utf8_lossy(&out.stderr))
            .unwrap_or_else(|| "ssh failed".into()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let found = lines.next().unwrap_or("MISSING").trim().to_string();
    let platform = lines.next().unwrap_or("").trim().to_string();
    if found != "MISSING" && !found.is_empty() {
        return Ok(found);
    }
    if !platform_matches(&platform) {
        return Err(format!(
            "argus not installed on remote ({platform}); install it there"
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
}
