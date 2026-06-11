//! Elm-shaped app core: the Model, message handling (`update`), derived
//! data, and the top-level `view` routing.
//!
//! The derived-data rule lives here: everything renderable is recomputed
//! ONLY on data messages (`SnapshotLoaded`, `ScanDone`), never per frame.
//! `view` is pure; the animator (with its injected clock) is its only
//! source of motion.

use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::KeyEventKind;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::Line;
use ratatui::widgets::Block;
use ratatui::Frame;

use crate::advisories::{self, AdvisoryBlock};
use crate::cache_analysis::CacheReport;
use crate::commands::stats_data::StatsSnapshot;
use crate::project::ProjectReport;
use crate::tiers::TierReport;
use crate::usage::cost;

use super::anim::{AnimKey, Animator, Ease};
use super::components::chrome::{self, FooterProps, HeaderProps};
use super::components::empty;
use super::components::list::Selection;
use super::components::modal::{self, ModalWidth};
use super::components::spinner;
use super::data::{self, DayDetail, HeavyRow, MachineProject, ModelRow};
use super::event::Msg;
use super::format;
use super::keymap::{self, Action, Context, TabId};
use super::screens;
use super::theme::{self, Theme, ThemeEnv};

/// Side effects requested by `update`; the event loop dispatches them.
#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    SpawnScan(PathBuf),
    ReloadSnapshot,
    Quit,
}

/// Status of the cwd project scan.
pub enum ScanState {
    /// No scan possible or none started (compact frame, non-dir cwd).
    Idle,
    Scanning,
    Ready(Box<ProjectReport>),
    Failed(String),
}

/// Completion metadata of the last successful scan.
#[derive(Clone, Copy, Debug)]
pub struct ScanMeta {
    pub at_epoch: u64,
    pub elapsed_ms: u64,
}

/// Which Spend panel owns navigation keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpendFocus {
    Days,
    Advisories,
}

/// A stacked overlay. Wave 1 ships the dialog shell; detail content
/// arrives with the next wave.
pub struct Overlay {
    pub title: String,
    pub body: Vec<String>,
    pub width: ModalWidth,
}

/// Everything derived from the snapshot and scan, recomputed on data
/// messages only (the 30 fps render path reads it, never rebuilds it).
pub struct Derived {
    pub setup_needed: bool,
    pub ingestion_on: bool,
    pub project_key: String,
    pub rate_model_display: &'static str,
    /// Epoch seconds when the snapshot was loaded ("data 2m" age source).
    pub loaded_at: Option<u64>,
    pub project_report: TierReport,
    pub global_report: TierReport,
    pub machine: Vec<MachineProject>,
    pub day_details: Vec<DayDetail>,
    pub spend_models: Vec<ModelRow>,
    pub models_total: usize,
    pub cache: Option<CacheReport>,
    /// Full advisory block; the compact lines below render today, the
    /// detail modal (next wave) reads the block itself.
    #[allow(dead_code)]
    pub advisories: Option<AdvisoryBlock>,
    pub advisory_lines: Vec<String>,
    pub heavy: Vec<HeavyRow>,
    pub realized_spark: Vec<u64>,
    /// Load profile buckets from the live scan: (label, tokens).
    pub load: [(&'static str, u64); 4],
    pub scan_files: u64,
    pub today_cost: f64,
    pub last30_cost: f64,
    pub sessions_total: usize,
}

fn empty_report() -> TierReport {
    TierReport {
        identified: None,
        realized: None,
        measured: None,
        notes: Vec::new(),
    }
}

const LOAD_LABELS: [&str; 4] = ["always", "on-invocation", "on-demand", "docs"];

impl Derived {
    pub fn empty() -> Derived {
        Derived {
            setup_needed: true,
            ingestion_on: false,
            project_key: String::new(),
            rate_model_display: "",
            loaded_at: None,
            project_report: empty_report(),
            global_report: empty_report(),
            machine: Vec::new(),
            day_details: Vec::new(),
            spend_models: Vec::new(),
            models_total: 0,
            cache: None,
            advisories: None,
            advisory_lines: Vec::new(),
            heavy: Vec::new(),
            realized_spark: Vec::new(),
            load: [
                (LOAD_LABELS[0], 0),
                (LOAD_LABELS[1], 0),
                (LOAD_LABELS[2], 0),
                (LOAD_LABELS[3], 0),
            ],
            scan_files: 0,
            today_cost: 0.0,
            last30_cost: 0.0,
            sessions_total: 0,
        }
    }

    pub fn compute(snapshot: Option<&StatsSnapshot>, scan: &ScanState) -> Derived {
        let Some(s) = snapshot else {
            return Derived::empty();
        };
        let project_report = s.compute_project();
        let global_report = s.compute_global();
        let sessions = data::sessions_by_project(s);
        let mut machine = data::machine_projects(&s.records, &sessions);
        // Default order with the deterministic tiebreak; the `,` sort
        // cycle (a later wave) swaps the variant.
        data::sort_machine(&mut machine, data::MachineSort::Weight);
        let today = data::day_string(s.now);
        let day_details = data::spend_day_details(s, &today, &cost::cost_usd);
        let spend_models = data::top_model_rows(&global_report);
        let models_total = global_report
            .measured
            .as_ref()
            .map(|m| m.by_model.len())
            .unwrap_or(0);
        let cache = s.compute_cache(None);
        let advisory_block = s.compute_advisories(&global_report);
        let advisory_lines = advisory_block
            .as_ref()
            .map(advisories::tui_compact_lines)
            .unwrap_or_default();
        let (heavy, load, scan_files) = match scan {
            ScanState::Ready(report) => (
                data::heavy_file_rows(report),
                [
                    (LOAD_LABELS[0], report.profiles.always.tokens),
                    (LOAD_LABELS[1], report.profiles.on_invocation.tokens),
                    (LOAD_LABELS[2], report.profiles.on_demand.tokens),
                    (LOAD_LABELS[3], report.profiles.docs.tokens),
                ],
                report.totals.files_scanned,
            ),
            _ => (Vec::new(), Derived::empty().load, 0),
        };
        let today_cost = day_details.last().map(|d| d.cost_usd).unwrap_or(0.0);
        let last30_cost = day_details.iter().map(|d| d.cost_usd).sum();
        let realized_spark = data::realized_sparkline_for_project(&s.records, &s.project_key);
        Derived {
            setup_needed: s.config.is_none() && s.records.is_empty(),
            ingestion_on: s.ingestion_on,
            project_key: s.project_key.clone(),
            rate_model_display: s.rate_model_display,
            loaded_at: Some(s.now),
            project_report,
            global_report,
            machine,
            day_details,
            spend_models,
            models_total,
            cache,
            advisories: advisory_block,
            advisory_lines,
            heavy,
            realized_spark,
            load,
            scan_files,
            today_cost,
            last30_cost,
            sessions_total: s.usage_data.as_ref().map(|d| d.sessions.len()).unwrap_or(0),
        }
    }

    /// Global identified reclaimable range, the Overview card source.
    pub fn reclaimable(&self) -> Option<(u64, u64)> {
        let id = self.global_report.identified.as_ref()?;
        match (id.project_reclaimable_min, id.project_reclaimable_max) {
            (Some(min), Some(max)) => Some((min, max)),
            _ => None,
        }
    }

    /// Global measured cache hit rate in percent.
    pub fn cache_pct(&self) -> Option<f64> {
        self.global_report
            .measured
            .as_ref()
            .map(|m| m.cache_hit_rate * 100.0)
    }
}

/// Half-page size for ctrl+d / ctrl+u (viewport heights vary per panel; a
/// fixed page keeps update independent of layout).
const PAGE: usize = 16;

pub struct Model {
    pub snapshot: Option<Box<StatsSnapshot>>,
    pub derived: Derived,
    pub scan: ScanState,
    pub tab: TabId,
    pub theme: Theme,
    pub theme_env: ThemeEnv,
    pub animator: Animator,
    pub overlays: Vec<Overlay>,
    pub sel_overview: Selection,
    pub sel_heavy: Selection,
    pub sel_machine: Selection,
    pub sel_spend: Selection,
    pub spend_focus: SpendFocus,
    pub day_cursor: usize,
    pub refreshing: bool,
    /// Accumulated busy milliseconds, drives the spinner frame.
    pub busy_ms: u64,
    pub now_epoch: u64,
    pub last_scan: Option<ScanMeta>,
    pub version: &'static str,
    pub prices_observed: &'static str,
}

impl Model {
    pub fn new(
        snapshot: Option<Box<StatsSnapshot>>,
        theme: Theme,
        theme_env: ThemeEnv,
        animator: Animator,
        now_epoch: u64,
    ) -> Model {
        let derived = Derived::compute(snapshot.as_deref(), &ScanState::Idle);
        let day_cursor = derived.day_details.len().saturating_sub(1);
        let mut model = Model {
            snapshot,
            derived,
            scan: ScanState::Idle,
            tab: TabId::Overview,
            theme,
            theme_env,
            animator,
            overlays: Vec::new(),
            sel_overview: Selection::default(),
            sel_heavy: Selection::default(),
            sel_machine: Selection::default(),
            sel_spend: Selection::default(),
            spend_focus: SpendFocus::Days,
            day_cursor,
            refreshing: false,
            busy_ms: 0,
            now_epoch,
            last_scan: None,
            version: env!("CARGO_PKG_VERSION"),
            prices_observed: tolkin_core::pricing::PRICES_OBSERVED,
        };
        model
            .animator
            .go(AnimKey::TabUnderline, 0.0, Duration::ZERO, Ease::Linear);
        model.fire_data_animations();
        model
    }

    /// Deterministic model for the compact frame and static tests: disabled
    /// animator (every tween snaps), clock pinned to the snapshot's load
    /// time so relative ages render stable.
    pub fn compact(snapshot: Option<Box<StatsSnapshot>>, theme: Theme) -> Model {
        let now = snapshot.as_ref().map(|s| s.now).unwrap_or(0);
        Model::new(
            snapshot,
            theme,
            ThemeEnv::default(),
            Animator::disabled(),
            now,
        )
    }

    pub fn is_busy(&self) -> bool {
        self.refreshing || matches!(self.scan, ScanState::Scanning)
    }

    /// Kick the startup scan if the cwd is scannable.
    pub fn start_initial_scan(&mut self) -> Vec<Cmd> {
        self.begin_scan()
    }

    fn begin_scan(&mut self) -> Vec<Cmd> {
        if matches!(self.scan, ScanState::Scanning) {
            return Vec::new();
        }
        if self.derived.project_key.is_empty() {
            return Vec::new();
        }
        let root = PathBuf::from(&self.derived.project_key);
        if !root.is_dir() {
            return Vec::new();
        }
        self.scan = ScanState::Scanning;
        self.busy_ms = 0;
        vec![Cmd::SpawnScan(root)]
    }

    /// The input context for keymap resolution.
    pub fn context(&self) -> Context {
        if !self.overlays.is_empty() {
            return Context::Modal;
        }
        match self.tab {
            TabId::Spend if self.spend_focus == SpendFocus::Days => Context::DayStrip,
            _ => Context::List,
        }
    }

    fn set_tab(&mut self, tab: TabId) {
        self.tab = tab;
        self.animator.go(
            AnimKey::TabUnderline,
            tab.index() as f32,
            Duration::from_millis(150),
            Ease::OutCubic,
        );
    }

    /// The selection and row count for the focused list on the active tab.
    fn active_list(&mut self) -> Option<(&mut Selection, usize)> {
        match self.tab {
            TabId::Overview => Some((&mut self.sel_overview, self.derived.advisory_lines.len())),
            TabId::Project => Some((&mut self.sel_heavy, self.derived.heavy.len())),
            TabId::Machine => Some((&mut self.sel_machine, self.derived.machine.len())),
            TabId::Spend if self.spend_focus == SpendFocus::Advisories => {
                Some((&mut self.sel_spend, self.derived.advisory_lines.len()))
            }
            TabId::Spend => None,
        }
    }

    fn recompute_derived(&mut self) {
        self.derived = Derived::compute(self.snapshot.as_deref(), &self.scan);
        let lens = [
            self.derived.advisory_lines.len(),
            self.derived.heavy.len(),
            self.derived.machine.len(),
        ];
        self.sel_overview.clamp(lens[0]);
        self.sel_heavy.clamp(lens[1]);
        self.sel_machine.clamp(lens[2]);
        self.sel_spend.clamp(lens[0]);
        let days = self.derived.day_details.len();
        if self.day_cursor >= days {
            self.day_cursor = days.saturating_sub(1);
        }
        self.fire_data_animations();
    }

    /// Retarget every data-driven tween: hero count-ups, bar grow-ins, the
    /// cache gauge, the day strip. Runs on data messages only.
    fn fire_data_animations(&mut self) {
        let d400 = Duration::from_millis(400);
        let d300 = Duration::from_millis(300);
        self.animator.go(
            AnimKey::Card(0),
            self.derived.today_cost as f32,
            d400,
            Ease::OutCubic,
        );
        self.animator.go(
            AnimKey::Card(1),
            self.derived.last30_cost as f32,
            d400,
            Ease::OutCubic,
        );
        self.animator.go(
            AnimKey::Card(2),
            self.derived.cache_pct().unwrap_or(0.0) as f32,
            d400,
            Ease::OutCubic,
        );
        let (rmin, rmax) = self.derived.reclaimable().unwrap_or((0, 0));
        self.animator
            .go(AnimKey::Card(3), rmin as f32, d400, Ease::OutCubic);
        self.animator
            .go(AnimKey::Card(4), rmax as f32, d400, Ease::OutCubic);
        self.animator.go(
            AnimKey::Gauge(0),
            (self.derived.cache_pct().unwrap_or(0.0) / 100.0) as f32,
            d400,
            Ease::OutCubic,
        );

        // Load profile bars (panel 0): fill fraction per bucket. The row
        // stagger is approximated by lengthening duration per row.
        let max_load = self.derived.load.iter().map(|(_, v)| *v).max().unwrap_or(0);
        for (i, (_, value)) in self.derived.load.iter().enumerate() {
            let frac = if max_load > 0 {
                *value as f32 / max_load as f32
            } else {
                0.0
            };
            self.animator.go(
                AnimKey::Bar {
                    panel: 0,
                    row: i as u8,
                },
                frac,
                d300 + Duration::from_millis(30 * i as u64),
                Ease::OutCubic,
            );
        }

        // Machine project weight bars (panel 1), capped at the key space.
        let max_weight = self
            .derived
            .machine
            .iter()
            .map(|p| p.always_tokens)
            .max()
            .unwrap_or(0);
        for (i, p) in self.derived.machine.iter().take(64).enumerate() {
            let frac = if max_weight > 0 {
                p.always_tokens as f32 / max_weight as f32
            } else {
                0.0
            };
            self.animator.go(
                AnimKey::Bar {
                    panel: 1,
                    row: i as u8,
                },
                frac,
                d300 + Duration::from_millis(30 * (i as u64).min(8)),
                Ease::OutCubic,
            );
        }

        // Day strip levels (panel 2): one tween per day cell.
        let max_day = self
            .derived
            .day_details
            .iter()
            .map(|d| d.input_side)
            .max()
            .unwrap_or(0);
        for (i, day) in self.derived.day_details.iter().enumerate() {
            let frac = if max_day > 0 {
                day.input_side as f32 / max_day as f32
            } else {
                0.0
            };
            self.animator.go(
                AnimKey::Bar {
                    panel: 2,
                    row: i as u8,
                },
                frac,
                d300,
                Ease::OutCubic,
            );
        }
        // Overview spend spark (panel 3): cost levels.
        let max_cost = self
            .derived
            .day_details
            .iter()
            .map(|d| d.cost_usd)
            .fold(0.0f64, f64::max);
        for (i, day) in self.derived.day_details.iter().enumerate() {
            let frac = if max_cost > 0.0 {
                (day.cost_usd / max_cost) as f32
            } else {
                0.0
            };
            self.animator.go(
                AnimKey::Bar {
                    panel: 3,
                    row: i as u8,
                },
                frac,
                d300,
                Ease::OutCubic,
            );
        }
    }
}

/// Mutate the model for one message and return the side effects to run.
pub fn update(model: &mut Model, msg: Msg) -> Vec<Cmd> {
    match msg {
        Msg::Key(key) => {
            if key.kind != KeyEventKind::Press {
                return Vec::new();
            }
            let Some(action) = keymap::resolve(&key, model.context()) else {
                return Vec::new();
            };
            handle_action(model, action)
        }
        Msg::Tick {
            now_epoch,
            delta_ms,
        } => {
            model.now_epoch = now_epoch;
            if model.is_busy() {
                model.busy_ms = model.busy_ms.saturating_add(delta_ms);
            }
            model.animator.prune();
            Vec::new()
        }
        Msg::Resize(_, _) => Vec::new(),
        // Mouse wiring is a later wave; capture is on, events are dropped.
        Msg::Mouse(_) => Vec::new(),
        Msg::ScanDone {
            result,
            at_epoch,
            elapsed_ms,
        } => {
            match result {
                Ok(report) => {
                    model.scan = ScanState::Ready(report);
                    model.last_scan = Some(ScanMeta {
                        at_epoch,
                        elapsed_ms,
                    });
                }
                Err(msg) => model.scan = ScanState::Failed(msg),
            }
            model.recompute_derived();
            Vec::new()
        }
        Msg::SnapshotLoaded(snapshot) => {
            model.snapshot = (*snapshot).map(Box::new);
            model.refreshing = false;
            model.recompute_derived();
            Vec::new()
        }
    }
}

fn handle_action(model: &mut Model, action: Action) -> Vec<Cmd> {
    match action {
        Action::Quit => return vec![Cmd::Quit],
        Action::Back => {
            // Pop the overlay stack; with nothing stacked, esc is a no-op
            // (it never quits).
            model.overlays.pop();
        }
        Action::GoTab(tab) => model.set_tab(tab),
        Action::NextTab => model.set_tab(model.tab.next()),
        Action::PrevTab => model.set_tab(model.tab.prev()),
        Action::Down => {
            if let Some((sel, len)) = model.active_list() {
                sel.down(len);
            }
        }
        Action::Up => {
            if let Some((sel, _)) = model.active_list() {
                sel.up();
            }
        }
        Action::Top => {
            if let Some((sel, _)) = model.active_list() {
                sel.top();
            }
        }
        Action::Bottom => {
            if let Some((sel, len)) = model.active_list() {
                sel.bottom(len);
            }
        }
        Action::HalfPageDown => {
            if let Some((sel, len)) = model.active_list() {
                sel.half_down(len, PAGE);
            }
        }
        Action::HalfPageUp => {
            if let Some((sel, _)) = model.active_list() {
                sel.half_up(PAGE);
            }
        }
        Action::DayLeft => {
            if model.tab == TabId::Spend {
                model.day_cursor = model.day_cursor.saturating_sub(1);
            }
        }
        Action::DayRight => {
            if model.tab == TabId::Spend {
                let len = model.derived.day_details.len();
                if len > 0 && model.day_cursor + 1 < len {
                    model.day_cursor += 1;
                }
            }
        }
        Action::PanelNext | Action::PanelPrev => {
            if model.tab == TabId::Spend {
                model.spend_focus = match model.spend_focus {
                    SpendFocus::Days => SpendFocus::Advisories,
                    SpendFocus::Advisories => SpendFocus::Days,
                };
            }
        }
        Action::Refresh => {
            if !model.refreshing {
                model.refreshing = true;
                model.busy_ms = 0;
                return vec![Cmd::ReloadSnapshot];
            }
        }
        Action::Rescan => return model.begin_scan(),
        Action::CycleTheme => {
            model.theme = theme::cycle(model.theme.name, &model.theme_env);
        }
        // Wired in later waves: detail modals, audit, report, copy, filter,
        // sort, help, palette. The bindings exist so help and the palette
        // stay complete; pressing them is a deliberate no-op today.
        Action::OpenDetail
        | Action::AuditSelected
        | Action::GenerateReport
        | Action::CopySelection
        | Action::FilterList
        | Action::CycleSort
        | Action::Help
        | Action::Palette => {}
    }
    Vec::new()
}

/// Render the full app: chrome, the active screen (or setup), overlays.
pub fn view(frame: &mut Frame, model: &Model) {
    let area = frame.area();
    frame.render_widget(
        Block::new().style(Style::default().bg(model.theme.bg)),
        area,
    );
    if area.width < 80 || area.height < 24 {
        empty::render_too_small(frame, area, &model.theme);
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .split(area);

    let data_age = model
        .derived
        .loaded_at
        .map(|t| format::relative_time(model.now_epoch, t));
    let busy_label = if model.refreshing {
        Some("reloading")
    } else if matches!(model.scan, ScanState::Scanning) {
        Some("scanning repo")
    } else {
        None
    };
    let spinner_frame = spinner::frame(model.busy_ms, model.animator.enabled());
    let header = HeaderProps {
        active: model.tab,
        underline_pos: model
            .animator
            .value(AnimKey::TabUnderline, model.tab.index() as f32),
        ingestion_on: model.derived.ingestion_on,
        data_age: data_age.as_deref(),
        version: model.version,
        busy: busy_label.map(|l| (spinner_frame, l)),
    };
    chrome::render_header(frame, rows[0], &header, &model.theme);

    let body = inset(rows[1]);
    if model.derived.setup_needed {
        empty::render_setup(frame, body, &model.theme);
    } else {
        screens::render(frame, body, model);
    }

    let footer = FooterProps {
        context: model.context(),
        rate_model_display: model.derived.rate_model_display,
        prices_observed: model.prices_observed,
    };
    chrome::render_footer(frame, rows[2], &footer, &model.theme);

    for overlay in &model.overlays {
        let body: Vec<Line> = overlay
            .body
            .iter()
            .map(|s| {
                Line::from(ratatui::text::Span::styled(
                    s.clone(),
                    Style::default().fg(model.theme.text),
                ))
            })
            .collect();
        modal::render_modal(frame, &overlay.title, &body, overlay.width, &model.theme);
    }
}

/// One blank row above the body plus a one-column margin: panels breathe
/// instead of touching the chrome.
fn inset(area: Rect) -> Rect {
    Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::anim::ManualClock;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn test_model() -> (Model, ManualClock) {
        let clock = ManualClock::new();
        let animator = Animator::new(Box::new(clock.clone()), true);
        let theme = theme::by_name("tolkin-dark", true).unwrap();
        let model = Model::new(None, theme, ThemeEnv::default(), animator, 1_780_000_000);
        (model, clock)
    }

    fn key(code: KeyCode) -> Msg {
        Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn ctrl(c: char) -> Msg {
        Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    #[test]
    fn tab_navigation_with_numbers_tab_and_shift_tab() {
        let (mut model, _clock) = test_model();
        assert_eq!(model.tab, TabId::Overview);
        update(&mut model, key(KeyCode::Char('4')));
        assert_eq!(model.tab, TabId::Spend);
        update(&mut model, key(KeyCode::Tab));
        assert_eq!(model.tab, TabId::Overview, "tab wraps");
        update(&mut model, key(KeyCode::BackTab));
        assert_eq!(model.tab, TabId::Spend);
        update(&mut model, key(KeyCode::Char('2')));
        assert_eq!(model.tab, TabId::Project);
        // Arrows do NOT switch tabs anymore.
        update(&mut model, key(KeyCode::Right));
        assert_eq!(model.tab, TabId::Project);
    }

    #[test]
    fn underline_animates_toward_the_active_tab() {
        let (mut model, clock) = test_model();
        update(&mut model, key(KeyCode::Char('3')));
        assert!(model.animator.active());
        clock.advance(Duration::from_millis(200));
        let pos = model.animator.value(AnimKey::TabUnderline, 0.0);
        assert!((pos - 2.0).abs() < 0.001, "underline at {pos}");
    }

    #[test]
    fn list_selection_moves_with_j_k_g_and_shift_g() {
        let (mut model, _clock) = test_model();
        model.derived.advisory_lines = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        update(&mut model, key(KeyCode::Char('j')));
        update(&mut model, key(KeyCode::Char('j')));
        assert_eq!(model.sel_overview.idx, 2);
        update(&mut model, key(KeyCode::Char('j')));
        assert_eq!(model.sel_overview.idx, 2, "clamped at end");
        update(&mut model, key(KeyCode::Char('k')));
        assert_eq!(model.sel_overview.idx, 1);
        update(&mut model, key(KeyCode::Char('g')));
        assert_eq!(model.sel_overview.idx, 0);
        update(&mut model, key(KeyCode::Char('G')));
        assert_eq!(model.sel_overview.idx, 2);
    }

    #[test]
    fn spend_day_strip_navigation_and_panel_cycle() {
        let (mut model, _clock) = test_model();
        model.derived.day_details = (0..5)
            .map(|i| DayDetail {
                day: format!("2026-06-0{}", i + 1),
                input_side: 100,
                input_fresh: 50,
                cache_read: 50,
                output: 10,
                cost_usd: 0.5,
            })
            .collect();
        model.day_cursor = 4;
        update(&mut model, key(KeyCode::Char('4')));
        assert_eq!(model.context(), Context::DayStrip);
        update(&mut model, key(KeyCode::Char('h')));
        assert_eq!(model.day_cursor, 3);
        update(&mut model, key(KeyCode::Char('l')));
        assert_eq!(model.day_cursor, 4);
        update(&mut model, key(KeyCode::Char('l')));
        assert_eq!(model.day_cursor, 4, "clamped at today");
        // Panel cycle moves focus to the advisories list.
        update(&mut model, key(KeyCode::Char(']')));
        assert_eq!(model.spend_focus, SpendFocus::Advisories);
        assert_eq!(model.context(), Context::List);
        update(&mut model, key(KeyCode::Char('[')));
        assert_eq!(model.spend_focus, SpendFocus::Days);
    }

    #[test]
    fn quit_via_q_and_ctrl_c_but_never_esc() {
        let (mut model, _clock) = test_model();
        assert_eq!(update(&mut model, key(KeyCode::Char('q'))), vec![Cmd::Quit]);
        assert_eq!(update(&mut model, ctrl('c')), vec![Cmd::Quit]);
        assert_eq!(update(&mut model, key(KeyCode::Esc)), Vec::<Cmd>::new());
    }

    #[test]
    fn esc_pops_the_overlay_stack() {
        let (mut model, _clock) = test_model();
        model.overlays.push(Overlay {
            title: "detail".to_string(),
            body: vec!["x".to_string()],
            width: ModalWidth::Medium,
        });
        assert_eq!(model.context(), Context::Modal);
        update(&mut model, key(KeyCode::Esc));
        assert!(model.overlays.is_empty());
        // Second esc is a no-op, not a quit.
        assert_eq!(update(&mut model, key(KeyCode::Esc)), Vec::<Cmd>::new());
    }

    #[test]
    fn refresh_flow_sets_busy_and_clears_on_snapshot() {
        let (mut model, _clock) = test_model();
        let cmds = update(&mut model, key(KeyCode::Char('r')));
        assert_eq!(cmds, vec![Cmd::ReloadSnapshot]);
        assert!(model.refreshing && model.is_busy());
        // A second r while busy is a no-op.
        assert!(update(&mut model, key(KeyCode::Char('r'))).is_empty());
        update(&mut model, Msg::SnapshotLoaded(Box::new(None)));
        assert!(!model.refreshing);
        assert!(model.derived.setup_needed);
    }

    #[test]
    fn scan_done_updates_state_and_meta() {
        let (mut model, _clock) = test_model();
        model.scan = ScanState::Scanning;
        update(
            &mut model,
            Msg::ScanDone {
                result: Err("boom".to_string()),
                at_epoch: 1_780_000_100,
                elapsed_ms: 1_200,
            },
        );
        assert!(matches!(model.scan, ScanState::Failed(_)));
        assert!(model.last_scan.is_none(), "failed scans leave no meta");
    }

    #[test]
    fn tick_advances_spinner_only_while_busy() {
        let (mut model, _clock) = test_model();
        update(
            &mut model,
            Msg::Tick {
                now_epoch: 1_780_000_001,
                delta_ms: 100,
            },
        );
        assert_eq!(model.busy_ms, 0);
        model.refreshing = true;
        update(
            &mut model,
            Msg::Tick {
                now_epoch: 1_780_000_002,
                delta_ms: 100,
            },
        );
        assert_eq!(model.busy_ms, 100);
        assert_eq!(model.now_epoch, 1_780_000_002);
    }

    #[test]
    fn theme_cycles_at_runtime() {
        let (mut model, _clock) = test_model();
        assert_eq!(model.theme.name, "tolkin-dark");
        update(&mut model, key(KeyCode::Char('t')));
        assert_eq!(model.theme.name, "tolkin-light");
    }

    #[test]
    fn unwired_actions_are_safe_no_ops() {
        let (mut model, _clock) = test_model();
        for code in [
            KeyCode::Enter,
            KeyCode::Char('a'),
            KeyCode::Char('o'),
            KeyCode::Char('y'),
            KeyCode::Char('/'),
            KeyCode::Char(','),
            KeyCode::Char('?'),
        ] {
            assert!(update(&mut model, key(code)).is_empty());
        }
    }
}
