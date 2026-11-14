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
use ratatui::crossterm::event::{self, Event, KeyEventKind};

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

    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Stream this machine's snapshots as JSON lines (used over SSH by `--host`).
    Collect,
}

const FRAME: Duration = Duration::from_millis(50);
const POLL: Duration = Duration::from_millis(1000);

fn main() -> Result<()> {
    let cli = Cli::parse();
    let i18n = I18n::new(cli.lang.as_deref());
    let source: Box<dyn Source> = if cli.demo {
        Box::new(DemoSource::new(&i18n.lang))
    } else {
        Box::new(ClaudeSource::new(
            cli.claude_dir.unwrap_or_else(source::claude::default_dir),
        ))
    };
    let rx = source::spawn(source, POLL);
    let mut app = App::new(i18n, rx, cli.demo);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let mut last = Instant::now();
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, app))?;
        let timeout = FRAME.saturating_sub(last.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key);
                }
            }
        }
        if last.elapsed() >= FRAME {
            app.on_tick();
            last = Instant::now();
        }
    }
    Ok(())
}
