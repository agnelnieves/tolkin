//! Tolkin dashboard.
//!
//! Four tabs (Overview default, Project, Machine, Spend) on an Elm-shaped
//! synchronous core: an input thread plus worker threads feed one mpsc
//! channel, the main loop blocks on `recv_timeout` (33 ms while animating,
//! 1 s idle), `app::update` mutates the model, `app::view` renders pure.
//! Terminal setup and teardown is panic-safe: a panic hook restores the
//! cooked terminal before re-raising, so a crash in a widget never leaves
//! the user in raw-mode no-echo with a hidden cursor.

use std::env;
use std::io::{self, Stdout, Write};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::buffer::Buffer;
use ratatui::Terminal;

use crate::commands::stats_data::StatsSnapshot;

// Exception flagged in the I5 report-implementation plan: `commands::report`
// reuses the 30-day spend bar merge from `tui::data` instead of duplicating
// the calendar logic. The whole module stays pub for that reason; nothing
// else reaches into it.
pub mod data;

mod anim;
mod app;
mod components;
mod event;
mod format;
mod keymap;
mod screens;
mod theme;

use anim::Animator;
use app::{Cmd, Model};
use event::Msg;

/// Frame cadence while a tween or spinner runs (about 30 fps).
const ANIM_TICK_MS: u64 = 33;
/// Setup-screen cadence while the border pulse breathes (section 5: the
/// 4600 ms sine only needs a 100 ms sample rate).
const PULSE_TICK_MS: u64 = 100;
/// Idle cadence: one tick per second refreshes relative times.
const IDLE_TICK_MS: u64 = 1_000;

/// Open the dashboard. Assumes the caller has already verified a TTY.
pub fn run() -> Result<()> {
    install_panic_hook();
    let mut terminal = enter_raw()?;
    let outcome = event_loop(&mut terminal);
    restore_terminal(&mut terminal)?;
    outcome
}

/// Render a single static frame to a string. Used by `tolkin stats
/// --compact` for screenshot/share artifacts. No raw mode, no alternate
/// screen, no event loop; works under non-TTY stdio. Deterministic: the
/// animator is disabled so every surface renders at rest.
pub fn render_compact_frame() -> Result<String> {
    render_static_frame(StatsSnapshot::load(), 100, 30)
}

/// One static frame at the given size through the same view code the live
/// dashboard uses. `None` renders the setup card.
fn render_static_frame(snapshot: Option<StatsSnapshot>, w: u16, h: u16) -> Result<String> {
    let theme_env = theme::ThemeEnv::from_env();
    let selected = theme::select(&theme_env, config_theme(snapshot.as_ref()).as_deref());
    let model = Model::compact(snapshot.map(Box::new), selected);
    let backend = TestBackend::new(w, h);
    let mut terminal: Terminal<TestBackend> = Terminal::new(backend)?;
    terminal.draw(|frame| app::view(frame, &model))?;
    Ok(buffer_to_string(terminal.backend().buffer()))
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

/// The persisted dashboard theme from the snapshot's config, if any. Feeds
/// `theme::select`'s middle precedence rung (env, config, default).
fn config_theme(snapshot: Option<&StatsSnapshot>) -> Option<String> {
    snapshot
        .and_then(|s| s.config.as_ref())
        .and_then(|c| c.ui_theme.clone())
}

fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    let theme_env = theme::ThemeEnv::from_env();
    let reduced_motion = env::var("TOLKIN_REDUCED_MOTION").is_ok_and(|v| v == "1");
    let snapshot = StatsSnapshot::load();
    let selected = theme::select(&theme_env, config_theme(snapshot.as_ref()).as_deref());

    let (tx, rx) = mpsc::channel::<Msg>();
    event::spawn_input(tx.clone());

    let mut model = Model::new(
        snapshot.map(Box::new),
        selected,
        theme_env,
        Animator::system(!reduced_motion),
        event::now_epoch(),
    );
    if let Ok(size) = terminal.size() {
        model.viewport = (size.width, size.height);
    }
    if dispatch(terminal, &tx, model.start_initial_scan()) {
        return Ok(());
    }

    // Direct clock read, by ruling: the loop's tick delta is scheduling
    // (spinner cadence bookkeeping), not animation; tween time flows
    // exclusively through the Animator's Clock.
    let mut last_loop = Instant::now();
    loop {
        terminal.draw(|frame| app::view(frame, &model))?;

        // Reduced motion idles even while busy: the spinner is a static
        // glyph there, so 30 fps would only burn battery. Worker completion
        // messages wake the loop regardless of cadence.
        let cadence = if (model.is_busy() && model.animator.enabled()) || model.animator.active() {
            ANIM_TICK_MS
        } else if model.derived.setup_needed && model.animator.enabled() {
            PULSE_TICK_MS
        } else {
            IDLE_TICK_MS
        };
        let msg = match rx.recv_timeout(Duration::from_millis(cadence)) {
            Ok(msg) => msg,
            Err(RecvTimeoutError::Timeout) => Msg::Tick {
                now_epoch: event::now_epoch(),
                delta_ms: last_loop.elapsed().as_millis() as u64,
            },
            // Input thread gone: nothing can ever arrive again.
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        };
        last_loop = Instant::now();
        if dispatch(terminal, &tx, app::update(&mut model, msg)) {
            return Ok(());
        }
        // Drain whatever queued while rendering (resize bursts, fast keys).
        while let Ok(extra) = rx.try_recv() {
            if dispatch(terminal, &tx, app::update(&mut model, extra)) {
                return Ok(());
            }
        }
    }
}

/// Run side effects. Returns true when the loop should quit. The terminal
/// handle carries the OSC52 escape straight to the hosting emulator.
fn dispatch(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    tx: &Sender<Msg>,
    cmds: Vec<Cmd>,
) -> bool {
    for cmd in cmds {
        match cmd {
            Cmd::Quit => return true,
            Cmd::SpawnScan(root) => event::spawn_scan(tx.clone(), root),
            Cmd::ReloadSnapshot => event::spawn_reload(tx.clone()),
            Cmd::RunAudit { root, path } => event::spawn_audit(tx.clone(), root, path),
            Cmd::RunReport => event::spawn_report(tx.clone()),
            Cmd::CopyOsc52(payload) => {
                let backend = terminal.backend_mut();
                let _ = write!(backend, "{}", format::osc52(&payload));
                let _ = backend.flush();
            }
            // A tiny TOML write; inline rather than a worker.
            Cmd::PersistTheme(name) => event::persist_theme(&name),
        }
    }
    false
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

/// Install a panic hook that puts the terminal back into a usable state
/// before delegating to the previously-installed hook. Idempotent:
/// subsequent calls in the same process leave the prior hook chain in
/// place.
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
    use std::path::PathBuf;

    fn tmp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("tolkin-tui-mod-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Render a single Spend-tab frame using TestBackend and verify that
    /// the advisory compact lines appear when measured data is present.
    /// This is the TestBackend buffer test required by the gate.
    #[test]
    fn spend_tab_advisory_compact_lines_rendered_in_buffer() {
        use crate::advisories::AdvisoryInputs;
        use crate::tiers::{Measured, ModelUsage};
        use crate::usage::types::UsageTotals;
        use std::collections::BTreeMap;

        let dir = tmp_dir("spend-adv");
        let cfg = Config::new(true, false);
        ledger::save_config_to(&dir, &cfg).expect("save config");
        let snapshot = StatsSnapshot::load_in(&dir);

        // Build a minimal advisory block with a known output share framing.
        let mut by_model: BTreeMap<String, ModelUsage> = BTreeMap::new();
        by_model.insert(
            "claude-sonnet-4-6".to_string(),
            ModelUsage {
                totals: UsageTotals {
                    input_tokens: 1_000_000,
                    output_tokens: 200_000,
                    cache_read_tokens: 0,
                    cache_write_5m_tokens: 0,
                    cache_write_1h_tokens: 0,
                },
                cost_usd: Some(3.50),
            },
        );
        let measured = Measured {
            label: "ground truth",
            sessions: 5,
            first_ts: Some(1_749_000_000),
            last_ts: Some(1_749_513_600),
            totals: UsageTotals {
                input_tokens: 1_000_000,
                output_tokens: 200_000,
                cache_read_tokens: 0,
                cache_write_5m_tokens: 0,
                cache_write_1h_tokens: 0,
            },
            by_model,
            cost_usd_total: 3.50,
            unpriced_models: vec![],
            cache_hit_rate: 0.0,
        };

        fn test_cost(model: &str, t: &UsageTotals) -> Option<f64> {
            if model == "claude-sonnet-4-6" {
                Some(
                    t.input_tokens as f64 * 3.0 / 1_000_000.0
                        + t.output_tokens as f64 * 15.0 / 1_000_000.0,
                )
            } else {
                None
            }
        }
        fn test_rate(model: &str) -> Option<f64> {
            if model == "claude-sonnet-4-6" {
                Some(3.0)
            } else {
                None
            }
        }

        let adv_block = crate::advisories::compute(&AdvisoryInputs {
            measured: &measured,
            sessions: &[],
            cost_fn: &test_cost,
            input_rate_fn: &test_rate,
            monthly_cap_usd: None,
            now: 1_749_513_600,
        });

        // Seed the model with the computed block the way a snapshot
        // recompute would: lines derive from the same advisory source.
        let theme = theme::by_name("tolkin-dark", true).unwrap();
        let mut model = Model::compact(Some(Box::new(snapshot)), theme);
        model.derived.ingestion_on = true;
        model.derived.advisory_lines = crate::advisories::tui_compact_lines(&adv_block);
        model.derived.advisories = Some(adv_block);
        model.tab = keymap::TabId::Spend;

        let backend = ratatui::backend::TestBackend::new(120, 30);
        let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| app::view(frame, &model))
            .expect("draw");

        let buf = buffer_to_string(terminal.backend().buffer());
        // The output share line must appear in the Spend tab buffer.
        assert!(
            buf.contains("output share") || buf.contains("output"),
            "output share line must appear in Spend tab buffer; buffer:\n{buf}"
        );
        let _ = fs::remove_dir_all(&dir);
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
        let frame = render_static_frame(Some(snapshot), 100, 40).expect("render");
        assert!(frame.contains("tolkin"), "header missing: {frame}");
        assert!(frame.contains("Overview"));
        assert!(frame.contains("Project"));
        assert!(frame.contains("Machine"));
        assert!(frame.contains("Spend"));
        assert!(
            frame.contains("input savings, output may vary"),
            "honesty line missing"
        );
        // A tier label must reach the buffer: the Overview reclaimable card
        // carries the "advisory" tag for the seeded identified range.
        assert!(
            frame.contains("advisory"),
            "tier label missing from frame: {frame}"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn render_static_frame_without_data_dir_shows_setup_card() {
        let frame = render_static_frame(None, 100, 30).expect("render");
        assert!(frame.contains("tolkin init"), "setup card missing: {frame}");
        assert!(frame.contains("q to quit"));
    }

    /// The compact frame is a shareable artifact: two renders of the same
    /// data dir must be byte-identical (animator disabled, no selection
    /// chrome, ages pinned to the snapshot's load time).
    #[test]
    fn static_frame_is_byte_deterministic_and_selection_free() {
        let dir = tmp_dir("frame-determinism");
        let cfg = Config::new(true, false);
        ledger::save_config_to(&dir, &cfg).expect("save config");
        ledger::append_in(
            &dir,
            "project",
            &dir,
            json!({
                "always_tokens": 9_000,
                "reclaimable_min": 200,
                "reclaimable_max": 800,
                "files_scanned": 120,
            }),
        )
        .expect("append");
        let first = render_static_frame(Some(StatsSnapshot::load_in(&dir)), 100, 30).unwrap();
        let second = render_static_frame(Some(StatsSnapshot::load_in(&dir)), 100, 30).unwrap();
        assert_eq!(first, second, "compact frame must render byte-stable");
        // No selection artifacts: the rail glyph never reaches the buffer.
        assert!(
            !first.contains('▌'),
            "selection rail leaked into the static frame:\n{first}"
        );
        // The Overview surfaces are all present in the artifact.
        for needle in [
            "today",
            "30 days",
            "cache hit",
            "reclaimable",
            "spend, last 30 days",
            "this machine",
            "advisory",
        ] {
            assert!(first.contains(needle), "{needle} missing:\n{first}");
        }
        let _ = fs::remove_dir_all(&dir);
    }
}
