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
        Some(Cmd::Hook {
            install: true,
            wait,
        }) => {
            let path = source::hook::install(&claude_dir, wait)?;
            println!("Argus approval hook added to {}", path.display());
            return Ok(());
        }
        Some(Cmd::Hook {
            install: false,
            wait,
        }) => return source::hook::run(&claude_dir, Duration::from_secs(wait)),
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

fn print_snapshot(mut source: Box<dyn Source>, wait: u64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(wait);
    let mut snap = source.poll();
    while Instant::now() < deadline {
        std::thread::sleep(POLL);
        snap = source.poll();
    }
    serde_json::to_writer_pretty(std::io::stdout(), &snap)?;
    println!();
    Ok(())
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let mut last = Instant::now();
    while !app.should_quit {
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
