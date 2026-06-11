//! Event sources for the dashboard: the input thread and worker threads.
//!
//! Everything funnels into one `mpsc::Sender<Msg>`; the main loop blocks on
//! `recv_timeout` and the model's `update` consumes messages. Threads are
//! detached and exit when their send fails (the receiver dropped because
//! the loop ended).

use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{Event, KeyEvent, MouseEvent};
use tolkin_core::audit::AuditReport;

use crate::commands::stats_data::StatsSnapshot;
use crate::project::{self, ProjectOptions, ProjectReport};

/// Everything the update loop can receive.
pub enum Msg {
    Key(KeyEvent),
    /// Mouse input: wheel scroll, row and tab clicks (`app::handle_mouse`).
    Mouse(MouseEvent),
    /// Terminal resized; the payload keeps `Model::viewport` honest for
    /// mouse hit-testing (relayout itself happens naturally per frame).
    Resize(u16, u16),
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
    /// Snapshot reload worker finished. `Ok(None)` means no data dir
    /// exists; `Err` is a reload failure (including a panicked worker), so
    /// the busy state resolves without dropping the data already loaded.
    SnapshotLoaded(Result<Box<Option<StatsSnapshot>>, String>),
    /// Single-file audit worker finished. `path` is the project-relative
    /// path the file detail modal is keyed on.
    AuditDone {
        path: String,
        result: Result<Box<AuditReport>, String>,
    },
    /// HTML report worker finished; `Ok` carries the artifact path.
    ReportDone(Result<PathBuf, String>),
}

/// Current unix epoch seconds. Clock reads live here and in workers, never
/// in view.
pub fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Run a worker body, converting an unwind into the worker's `Err` so the
/// busy state always resolves through the message channel. The panic hook
/// (mod.rs) restores the terminal only for main-thread panics; a worker
/// unwinding under the live event loop must never cook the terminal.
fn catch<T>(body: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    panic::catch_unwind(AssertUnwindSafe(body)).unwrap_or_else(|payload| {
        let msg = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "unknown panic".to_string());
        Err(format!("worker panicked: {msg}"))
    })
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
        // Direct clock read, by ruling: this measures worker wall time for
        // the "last scan (1.2s)" line, not animation; the Clock seam stays
        // animation-only.
        let started = Instant::now();
        let result = catch(|| {
            let opts = ProjectOptions {
                experimental: false,
                max_file_bytes: 2_000_000,
                top: 16,
            };
            if root.is_dir() {
                Ok(Box::new(project::analyze(&root, &opts)))
            } else {
                Err(format!("not a directory: {}", root.display()))
            }
        });
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
        let result = catch(|| Ok(Box::new(StatsSnapshot::load())));
        let _ = tx.send(Msg::SnapshotLoaded(result));
    });
}

/// Single-file audit worker: tokenize and audit `root/path` off the main
/// thread, reporting back keyed on the project-relative path.
pub fn spawn_audit(tx: Sender<Msg>, root: PathBuf, path: String) {
    thread::spawn(move || {
        let full = root.join(&path);
        let result = catch(|| {
            crate::commands::audit::audit_file(&full)
                .map(Box::new)
                .map_err(|e| e.to_string())
        });
        let _ = tx.send(Msg::AuditDone { path, result });
    });
}

/// HTML report worker: writes the default project-scope artifact into the
/// current directory, exactly like `tolkin report --html`.
pub fn spawn_report(tx: Sender<Msg>) {
    thread::spawn(move || {
        let result = catch(|| {
            let output = PathBuf::from(crate::commands::report::DEFAULT_OUTPUT);
            crate::commands::report::write_report(&output, false).map_err(|e| e.to_string())
        });
        let _ = tx.send(Msg::ReportDone(result));
    });
}

/// Persist the cycled theme through the existing config save path. Only
/// when a config already exists: the TUI never creates one (consent stays
/// init's job). The env kills (`CI`, `TOLKIN_NO_LEDGER`) leave no state.
pub fn persist_theme(name: &str) {
    if crate::ledger::disabled_by_env() {
        return;
    }
    let Some(mut cfg) = crate::ledger::load_config() else {
        return;
    };
    cfg.ui_theme = Some(name.to_string());
    let _ = crate::ledger::save_config(&cfg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catch_converts_worker_panics_into_err_results() {
        assert_eq!(catch(|| Ok::<u32, String>(7)), Ok(7));
        assert_eq!(
            catch(|| Err::<u32, String>("plain failure".to_string())),
            Err("plain failure".to_string())
        );
        // A &str panic payload (the default hook prints it to stderr in
        // test output; the unwind itself is contained here).
        let err = catch::<u32>(|| panic!("boom")).unwrap_err();
        assert_eq!(err, "worker panicked: boom");
        // A formatted panic carries a String payload.
        let err = catch::<u32>(|| panic!("bad index {}", 3)).unwrap_err();
        assert_eq!(err, "worker panicked: bad index 3");
    }
}
