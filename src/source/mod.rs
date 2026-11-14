//! Data sources. Each source produces a full [`Snapshot`] on every poll; a
//! background thread polls it and hands snapshots to the UI over a channel.

pub mod claude;
pub mod demo;
pub mod multi;
pub mod pricing;
pub mod remote;
pub mod system;

use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use crate::model::Snapshot;

pub trait Source: Send {
    fn poll(&mut self) -> Snapshot;
}

/// Polls `source` every `every` on a background thread.
pub fn spawn(mut source: Box<dyn Source>, every: Duration) -> Receiver<Snapshot> {
    let (tx, rx) = mpsc::channel();
    thread::Builder::new()
        .name("argus-source".into())
        .spawn(move || loop {
            if tx.send(source.poll()).is_err() {
                break;
            }
            thread::sleep(every);
        })
        .expect("spawn source thread");
    rx
}
