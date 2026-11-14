//! Data sources. Each source produces a full [`Snapshot`] on every poll; a
//! background thread polls it, applies [`Control`] commands from the UI and
//! reports their outcome as short notices.

pub mod actions;
pub mod claude;
pub mod demo;
pub mod events;
pub mod groups;
pub mod hook;
pub mod multi;
pub mod pricing;
pub mod remote;
pub mod runs;
pub mod system;

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::model::Snapshot;

/// Commands the UI sends to the data source.
#[derive(Debug, Clone)]
pub enum Control {
    /// Start watching another machine over SSH.
    AddHost(String),
    /// Answer an approval request raised by `argus hook`.
    Decide {
        agent: String,
        request: String,
        allow: bool,
    },
    /// Start a new headless Claude Code run.
    Run {
        host: Option<String>,
        dir: String,
        prompt: String,
    },
    /// Send a follow-up prompt to an existing session (`claude -p --resume`).
    Resume { agent: String, text: String },
}

/// Outcome of a control command, shown briefly in the UI.
pub type Notice = Result<String, String>;

pub trait Source: Send {
    fn poll(&mut self) -> Snapshot;

    /// Applies a control command. Sources that cannot handle it say so.
    fn control(&mut self, _c: &Control) -> Notice {
        Err("not supported by this data source".into())
    }

    /// Whether this source owns the agent with id `agent`.
    fn owns(&self, _agent: &str) -> bool {
        false
    }

    /// Host name this source reports, if it watches a single machine.
    fn host(&self) -> Option<String> {
        None
    }
}

pub struct Channels {
    pub snapshots: Receiver<Snapshot>,
    pub controls: Sender<Control>,
    pub notices: Receiver<Notice>,
}

/// Polls `source` every `every` on a background thread.
pub fn spawn(mut source: Box<dyn Source>, every: Duration) -> Channels {
    let (snap_tx, snap_rx) = mpsc::channel();
    let (ctl_tx, ctl_rx) = mpsc::channel::<Control>();
    let (note_tx, note_rx) = mpsc::channel();
    thread::Builder::new()
        .name("argus-source".into())
        .spawn(move || loop {
            if snap_tx.send(source.poll()).is_err() {
                break;
            }
            let until = Instant::now() + every;
            while let Some(left) = until.checked_duration_since(Instant::now()) {
                match ctl_rx.recv_timeout(left) {
                    Ok(c) => {
                        let _ = note_tx.send(source.control(&c));
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .expect("spawn source thread");
    Channels {
        snapshots: snap_rx,
        controls: ctl_tx,
        notices: note_rx,
    }
}
