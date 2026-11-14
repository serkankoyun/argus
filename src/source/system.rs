//! Host metrics and running Claude Code processes.

use std::path::PathBuf;

use sysinfo::{ProcessesToUpdate, System};

pub struct SystemProbe {
    sys: System,
}

#[derive(Debug, Clone, Default)]
pub struct HostStats {
    pub name: String,
    pub cpu_pct: u8,
    pub mem_pct: u8,
    /// Working directories of running `claude` processes.
    pub claude_cwds: Vec<PathBuf>,
}

impl Default for SystemProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemProbe {
    pub fn new() -> Self {
        Self { sys: System::new() }
    }

    pub fn sample(&mut self) -> HostStats {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);

        let total = self.sys.total_memory().max(1);
        let mem_pct = (self.sys.used_memory() * 100 / total).min(100) as u8;
        let cpu_pct = self.sys.global_cpu_usage().round().clamp(0.0, 100.0) as u8;

        let mut claude_cwds = Vec::new();
        for p in self.sys.processes().values() {
            if is_claude(p) {
                if let Some(cwd) = p.cwd() {
                    claude_cwds.push(cwd.to_path_buf());
                }
            }
        }
        HostStats {
            name: System::host_name().unwrap_or_else(|| "local".into()),
            cpu_pct,
            mem_pct,
            claude_cwds,
        }
    }
}

fn is_claude(p: &sysinfo::Process) -> bool {
    let name = p.name().to_string_lossy().to_lowercase();
    if name == "claude" || name == "claude.exe" {
        return true;
    }
    // Installed through npm, Claude Code runs as `node .../claude-code/cli.js`.
    p.cmd().iter().any(|a| {
        let a = a.to_string_lossy();
        a.contains("@anthropic-ai/claude-code") || a.ends_with("/claude") || a.ends_with("\\claude")
    })
}
