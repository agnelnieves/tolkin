//! Event sources for the dashboard: the input thread and worker threads.
//!
//! Everything funnels into one `mpsc::Sender<Msg>`; the main loop blocks on
//! `recv_timeout` and the model's `update` consumes messages. Threads are
//! detached and exit when their send fails (the receiver dropped because
//! the loop ended).

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{Event, KeyEvent, MouseEvent};

use crate::commands::stats_data::StatsSnapshot;
use crate::project::{self, ProjectOptions, ProjectReport};

/// Everything the update loop can receive.
pub enum Msg {
    Key(KeyEvent),
    /// Payload read when mouse wiring ships (next wave); capture is on.
    Mouse(#[allow(dead_code)] MouseEvent),
    /// Relayout happens naturally per frame; the payload is informational.
    Resize(#[allow(dead_code)] u16, #[allow(dead_code)] u16),
    /// Synthesized by the main loop on recv timeout: wall-clock epoch for
    /// relative times plus elapsed milliseconds for spinner cadence.
    Tick {
        now_epoch: u64,
        delta_ms: u64,
    },
    /// Project scan worker finished.
    ScanDone {
        result: Result<Box<ProjectReport>, String>,
        at_epoch: u64,
        elapsed_ms: u64,
    },
    /// Snapshot reload worker finished. `None` means no data dir exists.
    SnapshotLoaded(Box<Option<StatsSnapshot>>),
}

/// Current unix epoch seconds. Clock reads live here and in workers, never
/// in view.
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Input thread: blocks on crossterm reads forever, forwarding key, mouse,
/// and resize events.
pub fn spawn_input(tx: Sender<Msg>) {
    thread::spawn(move || loop {
        let msg = match crossterm::event::read() {
            Ok(Event::Key(key)) => Msg::Key(key),
            Ok(Event::Mouse(mouse)) => Msg::Mouse(mouse),
            Ok(Event::Resize(w, h)) => Msg::Resize(w, h),
            Ok(_) => continue,
            Err(_) => return,
        };
        if tx.send(msg).is_err() {
            return;
        }
    });
}

/// Scan worker: analyze `root` off the main thread and report timing so
/// the Overview "last scan" line stays honest.
pub fn spawn_scan(tx: Sender<Msg>, root: PathBuf) {
    thread::spawn(move || {
        let started = Instant::now();
        let opts = ProjectOptions {
            experimental: false,
            max_file_bytes: 2_000_000,
            top: 16,
        };
        let result = if root.is_dir() {
            Ok(Box::new(project::analyze(&root, &opts)))
        } else {
            Err(format!("not a directory: {}", root.display()))
        };
        let _ = tx.send(Msg::ScanDone {
            result,
            at_epoch: now_epoch(),
            elapsed_ms: started.elapsed().as_millis() as u64,
        });
    });
}

/// Snapshot reload worker: ledger plus usage cache read off the main
/// thread (the cold read can take a beat on big logs).
pub fn spawn_reload(tx: Sender<Msg>) {
    thread::spawn(move || {
        let snapshot = StatsSnapshot::load();
        let _ = tx.send(Msg::SnapshotLoaded(Box::new(snapshot)));
    });
}
