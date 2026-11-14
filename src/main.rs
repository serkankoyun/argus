//! Argus — watch and steer your Claude Code agents live, from the terminal.

mod app;
mod i18n;
mod model;
mod ui;

use std::time::{Duration, Instant};

use anyhow::Result;
use clap::Parser;
use ratatui::crossterm::event::{self, Event, KeyEventKind};

use crate::app::App;
use crate::i18n::I18n;

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
}

const TICK: Duration = Duration::from_millis(100);

fn main() -> Result<()> {
    let cli = Cli::parse();
    let i18n = I18n::new(cli.lang.as_deref());
    let mut app = App::new(i18n, cli.demo);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> Result<()> {
    let mut last_tick = Instant::now();
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, app))?;

        let timeout = TICK.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    app.on_key(key);
                }
            }
        }
        if last_tick.elapsed() >= TICK {
            app.on_tick();
            last_tick = Instant::now();
        }
    }
    Ok(())
}
