//! Launching and resuming Claude Code runs, locally or over SSH.

use std::process::{Command, Stdio};

use super::Notice;

/// Quotes `s` for a POSIX shell.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn claude_command() -> Command {
    if cfg!(windows) {
        // npm installs a `claude.cmd` shim on Windows.
        let mut c = Command::new("cmd");
        c.args(["/C", "claude"]);
        c
    } else {
        Command::new("claude")
    }
}

/// Starts `claude -p` detached in `dir`; the collector picks up its transcript.
pub fn run_local(dir: &str, args: &[&str]) -> Notice {
    let mut c = claude_command();
    c.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !dir.is_empty() {
        c.current_dir(dir);
    }
    match c.spawn() {
        Ok(mut child) => {
            // Reap the process when it exits so it does not linger as a zombie.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(format!(
                "started claude in {}",
                if dir.is_empty() { "." } else { dir }
            ))
        }
        Err(e) => Err(format!("could not start claude: {e}")),
    }
}

/// Starts `claude -p` detached on a remote machine.
pub fn run_remote(target: &str, dir: &str, args: &[&str]) -> Notice {
    let quoted: Vec<String> = args.iter().map(|a| sh_quote(a)).collect();
    let cd = if dir.is_empty() {
        String::new()
    } else {
        format!("cd {} && ", sh_quote(dir))
    };
    let script = format!("{cd}nohup claude {} >/dev/null 2>&1 &", quoted.join(" "));
    let status = Command::new("ssh")
        .args(["-T", "-o", "BatchMode=yes", target, &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(s) if s.success() => Ok(format!("started claude on {target}")),
        Ok(s) => Err(format!("ssh exited with {s}")),
        Err(e) => Err(format!("ssh: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_for_shell() {
        assert_eq!(sh_quote("it's"), "'it'\\''s'");
        assert_eq!(sh_quote("a b"), "'a b'");
    }
}
