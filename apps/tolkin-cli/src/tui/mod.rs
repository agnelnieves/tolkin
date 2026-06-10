//! Tolkin dashboard.
//!
//! Three tabs (Project default, Machine, Spend), synchronous event loop with
//! crossterm, ratatui 0.29 widgets. Terminal setup and teardown is panic-safe:
//! a panic hook restores the cooked terminal before re-raising, so a crash in
//! a widget never leaves the user in raw-mode no-echo with a hidden cursor.

use std::env;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::Terminal;

use crate::commands::stats_data::StatsSnapshot;
use crate::project::{self, ProjectOptions, ProjectReport};

// Exception flagged in the I5 report-implementation plan: `commands::report`
// reuses the 30-day spend bar merge from `tui::data` instead of duplicating
// the calendar logic. The whole module stays pub for that reason; nothing else
// reaches into it.
pub mod data;
mod ui;

use ui::{DashboardView, ProjectScanState, Tab};

const TICK_MS: u64 = 1_000;

/// Open the dashboard. Assumes the caller has already verified a TTY.
pub fn run() -> Result<()> {
    install_panic_hook();
    let mut terminal = enter_raw()?;
    let outcome = event_loop(&mut terminal);
    restore_terminal(&mut terminal)?;
    outcome
}

/// Render a single static frame to a string. Used by `tolkin stats --compact`
/// for screenshot/share artifacts. No raw mode, no alternate screen, no event
/// loop; works under non-TTY stdio.
pub fn render_compact_frame() -> Result<String> {
    let snapshot = match StatsSnapshot::load() {
        Some(s) => s,
        // No data dir at all: render the same setup card the dashboard uses.
        None => return render_setup_only(),
    };
    let frame = render_static_frame(&snapshot, 100, 30)?;
    Ok(frame)
}

fn render_setup_only() -> Result<String> {
    let backend = TestBackend::new(100, 30);
    let mut terminal: Terminal<TestBackend> = Terminal::new(backend)?;
    terminal.draw(|frame| {
        let view = DashboardView {
            tab: Tab::Project,
            project_key: "",
            records: &[],
            scan: &ProjectScanState::None,
            project_report: &empty_report(),
            global_report: &empty_report(),
            global_projects: &[],
            spend_days: &[],
            spend_models: &[],
            ingestion_on: false,
            setup_needed: true,
            rate_model_display: "",
            prices_observed: "",
        };
        ui::render(frame, &view);
    })?;
    Ok(buffer_to_string(terminal.backend().buffer()))
}

fn empty_report() -> crate::tiers::TierReport {
    crate::tiers::TierReport {
        identified: None,
        realized: None,
        measured: None,
        notes: Vec::new(),
    }
}

/// Render exactly one frame of the live dashboard into a TestBackend buffer,
/// then return its textual flattening. This is the entry point for the
/// compact frame artifact and the snapshot-style test.
fn render_static_frame(snapshot: &StatsSnapshot, w: u16, h: u16) -> Result<String> {
    let project_report = snapshot.compute_project();
    let global_report = snapshot.compute_global();
    let sessions = data::sessions_by_project(snapshot);
    let machine = data::machine_projects(&snapshot.records, &sessions);
    let today = data::day_string(snapshot.now);
    let spend_days = data::spend_day_bars(snapshot, &today);
    let spend_models = data::top_model_rows(&global_report);

    // Static frame: try the project scan inline (the compact frame is a
    // one-shot snapshot, no background thread). If it fails for any reason we
    // fall back to "no scan" rather than aborting the artifact.
    let scan_state = inline_scan_or_none(&snapshot.project_key);

    let backend = TestBackend::new(w, h);
    let mut terminal: Terminal<TestBackend> = Terminal::new(backend)?;
    let setup_needed = snapshot.config.is_none() && snapshot.records.is_empty();
    terminal.draw(|frame| {
        let view = DashboardView {
            tab: Tab::Project,
            project_key: snapshot.project_key.as_str(),
            records: &snapshot.records,
            scan: &scan_state,
            project_report: &project_report,
            global_report: &global_report,
            global_projects: &machine,
            spend_days: &spend_days,
            spend_models: &spend_models,
            ingestion_on: snapshot.ingestion_on,
            setup_needed,
            rate_model_display: snapshot.rate_model_display,
            prices_observed: tolkin_core::pricing::PRICES_OBSERVED,
        };
        ui::render(frame, &view);
    })?;
    Ok(buffer_to_string(terminal.backend().buffer()))
}

fn inline_scan_or_none(project_key: &str) -> ProjectScanState {
    let path = PathBuf::from(project_key);
    if !path.is_dir() {
        return ProjectScanState::None;
    }
    let opts = ProjectOptions {
        experimental: false,
        max_file_bytes: 2_000_000,
        top: 16,
    };
    ProjectScanState::Ready(Box::new(project::analyze(&path, &opts)))
}

fn buffer_to_string(buf: &Buffer) -> String {
    let mut out = String::with_capacity(buf.area.width as usize * buf.area.height as usize);
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let cell = &buf[(x, y)];
            out.push_str(cell.symbol());
        }
        out.push('\n');
    }
    out
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let mut snapshot = StatsSnapshot::load();
    let mut tab = Tab::Project;
    let mut scan_state = ProjectScanState::Scanning;
    let mut scan_rx = spawn_scan(snapshot.as_ref());

    loop {
        // Drain any pending scan results before drawing.
        if let Some(rx) = scan_rx.as_ref() {
            match rx.try_recv() {
                Ok(Ok(report)) => {
                    scan_state = ProjectScanState::Ready(Box::new(report));
                    scan_rx = None;
                }
                Ok(Err(msg)) => {
                    scan_state = ProjectScanState::Failed(msg);
                    scan_rx = None;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    scan_state = ProjectScanState::Failed("scan worker disconnected".into());
                    scan_rx = None;
                }
            }
        }

        terminal.draw(|frame| {
            draw_with_state(frame, snapshot.as_ref(), &scan_state, tab);
        })?;

        if event::poll(Duration::from_millis(TICK_MS))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q') | KeyCode::Esc, _) => break,
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => break,
                    (KeyCode::Tab | KeyCode::Right, _) => tab = tab.next(),
                    (KeyCode::BackTab | KeyCode::Left, _) => tab = tab.prev(),
                    (KeyCode::Char('r'), _) => {
                        snapshot = StatsSnapshot::load();
                        scan_state = ProjectScanState::Scanning;
                        scan_rx = spawn_scan(snapshot.as_ref());
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn draw_with_state(
    frame: &mut ratatui::Frame,
    snapshot: Option<&StatsSnapshot>,
    scan_state: &ProjectScanState,
    tab: Tab,
) {
    match snapshot {
        Some(s) => {
            let project_report = s.compute_project();
            let global_report = s.compute_global();
            let sessions = data::sessions_by_project(s);
            let machine = data::machine_projects(&s.records, &sessions);
            let today = data::day_string(s.now);
            let spend_days = data::spend_day_bars(s, &today);
            let spend_models = data::top_model_rows(&global_report);
            let setup_needed = s.config.is_none() && s.records.is_empty();
            let view = DashboardView {
                tab,
                project_key: s.project_key.as_str(),
                records: &s.records,
                scan: scan_state,
                project_report: &project_report,
                global_report: &global_report,
                global_projects: &machine,
                spend_days: &spend_days,
                spend_models: &spend_models,
                ingestion_on: s.ingestion_on,
                setup_needed,
                rate_model_display: s.rate_model_display,
                prices_observed: tolkin_core::pricing::PRICES_OBSERVED,
            };
            ui::render(frame, &view);
        }
        None => {
            let view = DashboardView {
                tab,
                project_key: "",
                records: &[],
                scan: scan_state,
                project_report: &empty_report(),
                global_report: &empty_report(),
                global_projects: &[],
                spend_days: &[],
                spend_models: &[],
                ingestion_on: false,
                setup_needed: true,
                rate_model_display: "",
                prices_observed: "",
            };
            ui::render(frame, &view);
        }
    }
}

/// Spawn a background project scan. Returns None if the cwd is not a
/// directory (the receiver-half is never created and the UI shows fallback
/// state).
fn spawn_scan(snapshot: Option<&StatsSnapshot>) -> Option<Receiver<Result<ProjectReport, String>>> {
    let project_key = snapshot.map(|s| s.project_key.clone()).unwrap_or_default();
    if project_key.is_empty() {
        return None;
    }
    let path = PathBuf::from(&project_key);
    if !path.is_dir() {
        return None;
    }
    let (tx, rx) = mpsc::channel();
    let cwd = env::current_dir().ok();
    thread::spawn(move || {
        let root = match cwd.and_then(|c| c.canonicalize().ok()) {
            Some(p) => p,
            None => {
                let _ = tx.send(Err("cwd not resolvable".to_string()));
                return;
            }
        };
        let opts = ProjectOptions {
            experimental: false,
            max_file_bytes: 2_000_000,
            top: 16,
        };
        let report = project::analyze(&root, &opts);
        let _ = tx.send(Ok(report));
    });
    Some(rx)
}

fn enter_raw() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    );
    let _ = terminal.show_cursor();
    Ok(())
}

type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static>;

/// Install a panic hook that puts the terminal back into a usable state before
/// delegating to the previously-installed hook. Idempotent: subsequent calls
/// in the same process leave the prior hook chain in place.
fn install_panic_hook() {
    static HOOK: OnceLock<()> = OnceLock::new();
    static PREV: Mutex<Option<PanicHook>> = Mutex::new(None);
    HOOK.get_or_init(|| {
        let prev = std::panic::take_hook();
        *PREV.lock().expect("panic hook mutex poisoned") = Some(prev);
        std::panic::set_hook(Box::new(|info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
            if let Some(prev) = PREV.lock().expect("panic hook mutex poisoned").as_ref() {
                prev(info);
            }
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{self, Config};
    use serde_json::json;
    use std::fs;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tolkin-tui-mod-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn render_static_frame_contains_chrome_and_honesty_line() {
        // Seed a ledger + config via the public seams (no env mutation).
        let dir = tmp_dir("frame-chrome");
        let cfg = Config::new(true, false);
        ledger::save_config_to(&dir, &cfg).expect("save config");
        ledger::append_in(
            &dir,
            "project",
            &dir,
            json!({
                "always_tokens": 12_000,
                "reclaimable_min": 100,
                "reclaimable_max": 400,
                "files_scanned": 200,
                "agent_context_files": 3,
                "on_invocation_tokens": 5_000,
                "on_demand_tokens": 1_200,
                "docs_tokens": 200,
            }),
        )
        .expect("append");
        let snapshot = StatsSnapshot::load_in(&dir);
        let frame = render_static_frame(&snapshot, 100, 30).expect("render");
        assert!(frame.contains("tolkin"), "header missing: {frame}");
        assert!(frame.contains("Project"));
        assert!(frame.contains("Machine"));
        assert!(frame.contains("Spend"));
        assert!(
            frame.contains("input savings, output may vary"),
            "honesty line missing"
        );
        // A tier label must reach the buffer (advisory estimate is the
        // identified tier label and shows up on the Project tab summary line).
        assert!(
            frame.contains("advisory"),
            "tier label missing from frame: {frame}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
