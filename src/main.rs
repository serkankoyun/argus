//! Argus — watch and steer your Claude Code agents live, from the terminal.

// Parts of the data model are consumed by views that land in later milestones.
// TODO(v1.0): remove once every view is implemented.
#![allow(dead_code)]

mod app;
mod i18n;
mod model;
mod source;
mod ui;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use clap::{Parser, Subcommand};
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton, MouseEventKind,
};
use ratatui::crossterm::execute;

use crate::app::App;
use crate::i18n::I18n;
use crate::source::{
    claude::ClaudeSource, demo::DemoSource, multi::MultiSource, remote::RemoteSource, Source,
};

/// Command-line options.
#[derive(Parser, Debug)]
#[command(name = "argus", version, about)]
struct Cli {
    /// UI language (e.g. `en`, `tr`). Defaults to the system locale.
    #[arg(long)]
    lang: Option<String>,

    /// Run with built-in demo data instead of real Claude Code sessions.
    #[arg(long)]
    demo: bool,

    /// Claude configuration directory (default: $CLAUDE_CONFIG_DIR or ~/.claude).
    #[arg(long, value_name = "DIR", global = true)]
    claude_dir: Option<PathBuf>,

    /// Also watch a remote machine over SSH (repeatable), e.g. `--host user@prod-1`.
    #[arg(long = "host", value_name = "SSH_TARGET")]
    hosts: Vec<String>,

    /// Do not watch the local machine (only the `--host` machines).
    #[arg(long)]
    no_local: bool,

    /// Show the last key Argus received in the footer (for troubleshooting).
    #[arg(long)]
    debug_keys: bool,

    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Stream this machine's snapshots as JSON lines (used over SSH by `--host`).
    Collect,
    /// Claude Code PermissionRequest hook: lets Argus approve tool calls.
    Hook {
        /// Add the hook to the Claude settings.json instead of running it.
        #[arg(long)]
        install: bool,
        /// Seconds to wait for a decision before falling back to the normal prompt.
        #[arg(long, default_value_t = 120, env = "ARGUS_HOOK_WAIT")]
        wait: u64,
        /// Record a lifecycle event (used by the installed hooks) and exit.
        #[arg(long)]
        event: bool,
    },
    /// Suggest team rules from the agents' names (`<role>-<team>-<work>`).
    Init {
        /// Save the suggestion to <claude dir>/argus/teams.json (a backup is kept).
        #[arg(long)]
        write: bool,
    },
    /// Print one snapshot as JSON and exit (for scripts and tests).
    Snapshot {
        /// Seconds to keep polling before printing (lets remote hosts connect).
        #[arg(long, default_value_t = 2)]
        wait: u64,
    },
}

const FRAME: Duration = Duration::from_millis(50);
const POLL: Duration = Duration::from_millis(1000);

fn main() -> Result<()> {
    let cli = Cli::parse();
    let claude_dir = cli
        .claude_dir
        .clone()
        .unwrap_or_else(source::claude::default_dir);
    match cli.command {
        Some(Cmd::Collect) => {
            return source::remote::collect(Box::new(ClaudeSource::new(claude_dir)), POLL)
        }
        Some(Cmd::Hook { event: true, .. }) => {
            let mut input = String::new();
            let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
            source::events::record(&claude_dir, &input);
            return Ok(());
        }
        Some(Cmd::Hook {
            install: true,
            wait,
            ..
        }) => {
            let path = source::hook::install(&claude_dir, wait)?;
            println!("Argus approval and event hooks added to {}", path.display());
            return Ok(());
        }
        Some(Cmd::Hook {
            install: false,
            wait,
            ..
        }) => return source::hook::run(&claude_dir, Duration::from_secs(wait)),
        Some(Cmd::Init { write }) => return init(&claude_dir, write, cli.lang.as_deref()),
        _ => {}
    }
    let i18n = I18n::new(cli.lang.as_deref());
    let source: Box<dyn Source> = if cli.demo {
        Box::new(DemoSource::new(&i18n.lang))
    } else {
        let mut sources: Vec<Box<dyn Source>> = Vec::new();
        if !cli.no_local || cli.hosts.is_empty() {
            sources.push(Box::new(ClaudeSource::new(claude_dir)));
        }
        for h in &cli.hosts {
            sources.push(Box::new(RemoteSource::new(h.clone())));
        }
        // Always multi, so hosts can be added at runtime with `:connect`.
        Box::new(MultiSource::new(sources))
    };
    if let Some(Cmd::Snapshot { wait }) = cli.command {
        return print_snapshot(source, wait);
    }
    let channels = source::spawn(source, POLL);
    let mut app = App::new(i18n, channels, cli.demo);
    app.debug_keys = cli.debug_keys;

    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = run(&mut terminal, &mut app);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

/// `argus init`: proposes (and with `--write` saves) team rules.
fn init(claude_dir: &std::path::Path, write: bool, lang: Option<&str>) -> Result<()> {
    use source::groups;
    let i18n = I18n::new(lang);
    // A teams_file that does not exist silently drops codes and labels.
    let current = std::fs::read_to_string(groups::path(claude_dir))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok());
    if let Some(file) = current
        .as_ref()
        .and_then(|v| v.get("teams_file"))
        .and_then(|f| f.as_str())
    {
        if !std::path::Path::new(file).exists() {
            println!("{}\n", i18n.t_args("init-missing-file", &[("path", file)]));
        }
    }
    let mut src = ClaudeSource::new(claude_dir.to_path_buf());
    let _ = src.poll();
    std::thread::sleep(Duration::from_millis(300));
    let snap = src.poll();
    let names: Vec<String> = snap.agents.iter().map(|a| a.name.clone()).collect();
    let Some(suggested) = groups::infer(&names, &i18n.t("init-default-team")) else {
        println!(
            "{}",
            i18n.t_args("init-none", &[("n", names.len().to_string().as_str())])
        );
        return Ok(());
    };
    // Existing rules are kept and only extended.
    let rules = match &current {
        Some(existing) => groups::Rules::merge_suggestion(existing, &suggested),
        None => suggested,
    };
    println!("{}\n", serde_json::to_string_pretty(&rules)?);
    let parsed = groups::Rules::from_value(&rules);
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    for n in &names {
        let team = parsed.team_of(n).unwrap_or_default();
        *counts.entry(team).or_default() += 1;
    }
    println!("{}", i18n.t("init-preview"));
    for (team, n) in counts {
        println!("  {team:<28} {n}");
    }
    let path = groups::path(claude_dir);
    if write {
        if path.exists() {
            std::fs::copy(&path, path.with_extension("json.bak"))?;
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(&rules)?)?;
        println!(
            "\n{}",
            i18n.t_args(
                "init-written",
                &[("path", path.display().to_string().as_str())]
            )
        );
    } else {
        println!(
            "\n{}",
            i18n.t_args(
                "init-hint",
                &[("path", path.display().to_string().as_str())]
            )
        );
    }
    Ok(())
}

fn print_snapshot(mut source: Box<dyn Source>, wait: u64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(wait);
    let mut snap = source.poll();
    while Instant::now() < deadline {
        std::thread::sleep(POLL);
        snap = source.poll();
    }
    use std::io::Write;
    // A closed pipe (`argus snapshot | head`) is not an error.
    let mut out = std::io::stdout().lock();
    match serde_json::to_writer_pretty(&mut out, &snap).map_err(std::io::Error::from) {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => return Ok(()),
        r => r?,
    }
    let _ = writeln!(out);
    Ok(())
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let mut last = Instant::now();
    let mut shown = (
        app.view,
        app.team.clone(),
        app.host.clone(),
        app.show_finished,
    );
    while !app.should_quit {
        // Repaint everything when the screen changes, so a terminal that draws a
        // glyph wider than expected cannot leave stale cells behind.
        if shown
            != (
                app.view,
                app.team.clone(),
                app.host.clone(),
                app.show_finished,
            )
        {
            terminal.clear()?;
            shown = (
                app.view,
                app.team.clone(),
                app.host.clone(),
                app.show_finished,
            );
        }
        terminal.draw(|frame| ui::draw(frame, app))?;
        let timeout = FRAME.saturating_sub(last.elapsed());
        if event::poll(timeout)? {
            match event::read()? {
                // Windows reports both press and release; act on press (and auto-repeat).
                Event::Key(key) if key.kind != KeyEventKind::Release => app.on_key(key),
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(i) = ui::tab_at(app, m.row, m.column) {
                            app.set_view_index(i);
                        }
                    }
                    MouseEventKind::ScrollDown => app.scroll(1),
                    MouseEventKind::ScrollUp => app.scroll(-1),
                    _ => {}
                },
                _ => {}
            }
        }
        if last.elapsed() >= FRAME {
            app.on_tick();
            last = Instant::now();
        }
    }
    Ok(())
}
