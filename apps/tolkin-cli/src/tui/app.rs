//! Elm-shaped app core: the Model, message handling (`update`), derived
//! data, and the top-level `view` routing.
//!
//! The derived-data rule lives here: everything renderable is recomputed
//! ONLY on data messages (`SnapshotLoaded`, `ScanDone`), never per frame.
//! `view` is pure; the animator (with its injected clock) is its only
//! source of motion.

use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Block;
use ratatui::Frame;
use tolkin_core::audit::AuditReport;

use crate::advisories::{self, AdvisoryBlock};
use crate::cache_analysis::CacheReport;
use crate::commands::stats_data::StatsSnapshot;
use crate::project::ProjectReport;
use crate::tiers::TierReport;
use crate::update;
use crate::usage::cost;

use super::anim::{AnimKey, Animator, Ease};
use super::components::bars;
use super::components::chrome::{self, FooterProps, HeaderProps};
use super::components::empty;
use super::components::help;
use super::components::list::Selection;
use super::components::modal::{self, ModalWidth};
use super::components::palette::{self, PaletteState};
use super::components::spinner;
use super::components::toast::{self, ToastKind, ToastStack};
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
    RunAudit {
        root: PathBuf,
        path: String,
    },
    RunReport,
    /// Emit the OSC52 clipboard escape for this payload through the
    /// terminal handle.
    CopyOsc52(String),
    /// Write the cycled theme into the existing config (never creates one).
    PersistTheme(String),
    Quit,
}

/// Which select-list a `/` filter narrows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterTarget {
    Heavy,
    Machine,
}

/// Inline list filter state: a plain case-insensitive substring over the
/// row's path. `matches` indexes into the underlying derived vector and is
/// recomputed on every edit and data refresh, never in view.
pub struct FilterState {
    pub target: FilterTarget,
    pub query: String,
    /// True while the filter line owns printable keys.
    pub typing: bool,
    pub matches: Vec<usize>,
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

/// A stacked overlay. Detail overlays hold plain data computed once at
/// open time (in `update`); `view` styles it per frame so theme cycling
/// stays live and the derived-data rule holds.
pub enum Overlay {
    File(FileDetailState),
    Project(ProjectDetailState),
    Advisory(AdvisoryDetailState),
    Help,
    Palette(PaletteState),
}

/// Lifecycle of the single-file audit a file-detail modal owns.
pub enum AuditState {
    NotRun,
    Running,
    Done(Box<AuditReport>),
    Failed(String),
}

/// File detail for one heavy-files row. The audit worker reports back
/// keyed on `path`; results land here even while the modal stays open.
pub struct FileDetailState {
    pub path: String,
    pub tokens: u64,
    pub pct_always: f64,
    /// Finding rollup from the live scan (count, savings range).
    pub scan_findings: usize,
    pub scan_savings: (u64, u64),
    pub audit: AuditState,
}

/// Project detail for one Machine-list row.
pub struct ProjectDetailState {
    pub key: String,
    /// Always-loaded tokens per snapshot, oldest first.
    pub series: Vec<u64>,
    pub first_ts: u64,
    pub last_ts: u64,
    pub sessions: u64,
}

/// Advisory detail: one section from the advisory block, resolved at open.
pub struct AdvisoryDetailState {
    pub title: String,
    pub paragraphs: Vec<String>,
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
    /// Full advisory block; the compact lines below render in the lists,
    /// the detail modal resolves its section from the block at open time.
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
    /// A cached known-newer version from the passive update check, read
    /// straight off the config. Zero network from the TUI.
    pub update_available: Option<String>,
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
            update_available: None,
        }
    }

    pub fn compute(
        snapshot: Option<&StatsSnapshot>,
        scan: &ScanState,
        machine_sort: data::MachineSort,
        model_sort: data::ModelSort,
    ) -> Derived {
        let Some(s) = snapshot else {
            return Derived::empty();
        };
        let project_report = s.compute_project();
        let global_report = s.compute_global();
        let sessions = data::sessions_by_project(s);
        let mut machine = data::machine_projects(&s.records, &sessions);
        data::sort_machine(&mut machine, machine_sort);
        let today = data::day_string(s.now);
        let day_details = data::spend_day_details(s, &today, &cost::cost_usd);
        let mut spend_models = data::model_rows(&global_report);
        data::sort_models(&mut spend_models, model_sort);
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
            update_available: s
                .config
                .as_ref()
                .and_then(|c| {
                    update::known_newer_version(
                        env!("CARGO_PKG_VERSION"),
                        c.last_seen_latest.as_deref(),
                    )
                })
                .map(str::to_string),
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
    pub machine_sort: data::MachineSort,
    pub model_sort: data::ModelSort,
    /// Active `/` filter, at most one list at a time.
    pub filter: Option<FilterState>,
    pub refreshing: bool,
    /// True while the report worker runs (one artifact at a time).
    pub reporting: bool,
    /// Accumulated busy milliseconds, drives the spinner frame.
    pub busy_ms: u64,
    pub now_epoch: u64,
    pub last_scan: Option<ScanMeta>,
    pub toasts: ToastStack,
    /// Last known terminal size, kept for mouse hit-testing.
    pub viewport: (u16, u16),
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
        let derived = Derived::compute(
            snapshot.as_deref(),
            &ScanState::Idle,
            data::MachineSort::Weight,
            data::ModelSort::Input,
        );
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
            machine_sort: data::MachineSort::Weight,
            model_sort: data::ModelSort::Input,
            filter: None,
            refreshing: false,
            reporting: false,
            busy_ms: 0,
            now_epoch,
            last_scan: None,
            toasts: ToastStack::default(),
            viewport: (0, 0),
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
        self.refreshing || self.reporting || matches!(self.scan, ScanState::Scanning)
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
        match self.overlays.last() {
            Some(Overlay::Palette(_)) => return Context::PaletteInput,
            Some(_) => return Context::Modal,
            None => {}
        }
        if self.filter.as_ref().is_some_and(|f| f.typing) {
            return Context::FilterInput;
        }
        match self.tab {
            TabId::Spend if self.spend_focus == SpendFocus::Days => Context::DayStrip,
            _ => Context::List,
        }
    }

    fn set_tab(&mut self, tab: TabId) {
        if tab != self.tab {
            // Filters are list-local; leaving the tab drops them.
            self.clear_filter();
        }
        self.tab = tab;
        self.animator.go(
            AnimKey::TabUnderline,
            tab.index() as f32,
            Duration::from_millis(150),
            Ease::OutCubic,
        );
    }

    /// The selection and row count for the focused list on the active tab,
    /// honoring an active filter.
    fn active_list(&mut self) -> Option<(&mut Selection, usize)> {
        match self.tab {
            TabId::Overview => Some((&mut self.sel_overview, self.derived.advisory_lines.len())),
            TabId::Project => {
                let len = self.visible_len(FilterTarget::Heavy, self.derived.heavy.len());
                Some((&mut self.sel_heavy, len))
            }
            TabId::Machine => {
                let len = self.visible_len(FilterTarget::Machine, self.derived.machine.len());
                Some((&mut self.sel_machine, len))
            }
            TabId::Spend if self.spend_focus == SpendFocus::Advisories => {
                Some((&mut self.sel_spend, self.derived.advisory_lines.len()))
            }
            TabId::Spend => None,
        }
    }

    /// Visible row count for a filterable list.
    fn visible_len(&self, target: FilterTarget, raw: usize) -> usize {
        match &self.filter {
            Some(f) if f.target == target => f.matches.len(),
            _ => raw,
        }
    }

    /// Map a visible (possibly filtered) row index to the derived index.
    fn filtered_index(&self, target: FilterTarget, visible: usize) -> Option<usize> {
        match &self.filter {
            Some(f) if f.target == target => f.matches.get(visible).copied(),
            _ => Some(visible),
        }
    }

    /// The heavy-files selection resolved through the active filter.
    fn selected_heavy(&self) -> Option<&HeavyRow> {
        let idx = self.filtered_index(FilterTarget::Heavy, self.sel_heavy.idx)?;
        self.derived.heavy.get(idx)
    }

    /// The machine-projects selection resolved through the active filter.
    fn selected_machine(&self) -> Option<&MachineProject> {
        let idx = self.filtered_index(FilterTarget::Machine, self.sel_machine.idx)?;
        self.derived.machine.get(idx)
    }

    /// Visible heavy-file rows in render order (filtered when active).
    pub fn visible_heavy(&self) -> Vec<&HeavyRow> {
        match &self.filter {
            Some(f) if f.target == FilterTarget::Heavy => f
                .matches
                .iter()
                .filter_map(|&i| self.derived.heavy.get(i))
                .collect(),
            _ => self.derived.heavy.iter().collect(),
        }
    }

    /// Visible machine projects in render order (filtered when active).
    pub fn visible_machine(&self) -> Vec<&MachineProject> {
        match &self.filter {
            Some(f) if f.target == FilterTarget::Machine => f
                .matches
                .iter()
                .filter_map(|&i| self.derived.machine.get(i))
                .collect(),
            _ => self.derived.machine.iter().collect(),
        }
    }

    /// The filter line for a list panel: (query, typing) when one targets
    /// it. Screens render it above the rows.
    pub fn filter_line(&self, target: FilterTarget) -> Option<(&str, bool)> {
        match &self.filter {
            Some(f) if f.target == target => Some((f.query.as_str(), f.typing)),
            _ => None,
        }
    }

    /// Recompute the active filter's matches against the current derived
    /// rows and re-clamp the affected selection.
    fn apply_filter(&mut self) {
        let Some(filter) = self.filter.as_mut() else {
            return;
        };
        let q = filter.query.to_lowercase();
        filter.matches = match filter.target {
            FilterTarget::Heavy => self
                .derived
                .heavy
                .iter()
                .enumerate()
                .filter(|(_, r)| q.is_empty() || r.path.to_lowercase().contains(&q))
                .map(|(i, _)| i)
                .collect(),
            FilterTarget::Machine => self
                .derived
                .machine
                .iter()
                .enumerate()
                .filter(|(_, p)| q.is_empty() || p.key.to_lowercase().contains(&q))
                .map(|(i, _)| i)
                .collect(),
        };
        let len = filter.matches.len();
        match filter.target {
            FilterTarget::Heavy => self.sel_heavy.clamp(len),
            FilterTarget::Machine => self.sel_machine.clamp(len),
        }
    }

    /// Drop the filter and restore the unfiltered selection bounds.
    fn clear_filter(&mut self) {
        if self.filter.take().is_some() {
            self.sel_heavy.clamp(self.derived.heavy.len());
            self.sel_machine.clamp(self.derived.machine.len());
        }
    }

    fn recompute_derived(&mut self) {
        self.derived = Derived::compute(
            self.snapshot.as_deref(),
            &self.scan,
            self.machine_sort,
            self.model_sort,
        );
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
        // The filter's indices point into the rebuilt vectors now.
        self.apply_filter();
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
            // The palette owns printable keys while open; everything it
            // does not consume falls through to the keymap (esc, ctrl+c).
            if let Some(cmds) = handle_palette_key(model, &key) {
                return cmds;
            }
            // Same deal for an actively-typing list filter.
            if let Some(cmds) = handle_filter_key(model, &key) {
                return cmds;
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
            model.toasts.prune(model.animator.now());
            Vec::new()
        }
        Msg::Resize(w, h) => {
            model.viewport = (w, h);
            Vec::new()
        }
        Msg::Mouse(mouse) => handle_mouse(model, &mouse),
        Msg::ScanDone {
            result,
            at_epoch,
            elapsed_ms,
        } => {
            match result {
                Ok(report) => {
                    let files = report.totals.files_scanned;
                    let warnings = report.warnings.len();
                    model.scan = ScanState::Ready(report);
                    model.last_scan = Some(ScanMeta {
                        at_epoch,
                        elapsed_ms,
                    });
                    let (kind, text) = if warnings == 0 {
                        (
                            ToastKind::Ok,
                            format!(
                                "scanned {} files in {}",
                                format::commas(files),
                                format::duration_short(elapsed_ms)
                            ),
                        )
                    } else {
                        (
                            ToastKind::Warn,
                            format!(
                                "scanned {} files in {}, {warnings} warning(s)",
                                format::commas(files),
                                format::duration_short(elapsed_ms)
                            ),
                        )
                    };
                    model.toasts.push(kind, text, model.animator.now());
                }
                Err(msg) => {
                    model.toasts.push(
                        ToastKind::Err,
                        format!("error scan failed: {msg}"),
                        model.animator.now(),
                    );
                    model.scan = ScanState::Failed(msg);
                }
            }
            model.recompute_derived();
            Vec::new()
        }
        Msg::SnapshotLoaded(snapshot) => {
            let was_refreshing = model.refreshing;
            model.snapshot = (*snapshot).map(Box::new);
            model.refreshing = false;
            model.recompute_derived();
            if was_refreshing {
                model.toasts.push(
                    ToastKind::Ok,
                    "refreshed ledger and usage reloaded".to_string(),
                    model.animator.now(),
                );
            }
            Vec::new()
        }
        Msg::AuditDone { path, result } => {
            let state = match result {
                Ok(report) => AuditState::Done(report),
                Err(msg) => {
                    model.toasts.push(
                        ToastKind::Err,
                        format!("error audit failed: {msg}"),
                        model.animator.now(),
                    );
                    AuditState::Failed(msg)
                }
            };
            // Land the result in the matching file-detail overlay; a closed
            // modal simply drops the result (the worker stays detached).
            if let Some(f) = model.overlays.iter_mut().find_map(|o| match o {
                Overlay::File(f) if f.path == path => Some(f),
                _ => None,
            }) {
                f.audit = state;
            }
            Vec::new()
        }
        Msg::ReportDone(result) => {
            model.reporting = false;
            match result {
                Ok(path) => model.toasts.push(
                    ToastKind::Ok,
                    format!("report written to {}", path.display()),
                    model.animator.now(),
                ),
                Err(msg) => model.toasts.push(
                    ToastKind::Err,
                    format!("error report failed: {msg}"),
                    model.animator.now(),
                ),
            }
            Vec::new()
        }
    }
}

fn handle_action(model: &mut Model, action: Action) -> Vec<Cmd> {
    match action {
        Action::Quit => return vec![Cmd::Quit],
        Action::Back => {
            // Pop the overlay stack, then clear an active filter; with
            // neither, esc is a no-op (it never quits).
            if model.overlays.pop().is_none() {
                model.clear_filter();
            }
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
            model.toasts.push(
                ToastKind::Info,
                format!("theme {} active", model.theme.name),
                model.animator.now(),
            );
            // Persist through the existing config save path, but only when
            // a config already exists (consent stays init's job).
            if model
                .snapshot
                .as_deref()
                .and_then(|s| s.config.as_ref())
                .is_some()
            {
                return vec![Cmd::PersistTheme(model.theme.name.to_string())];
            }
        }
        Action::OpenDetail => return open_detail(model),
        Action::AuditSelected => return audit_selected(model),
        Action::Help => {
            // Stack help over whatever is open (help over a detail modal
            // works); a second ? while it is topmost stays a single copy.
            if !matches!(model.overlays.last(), Some(Overlay::Help)) {
                model.overlays.push(Overlay::Help);
            }
        }
        Action::Palette => {
            let suggested = keymap::footer_actions(model.context());
            model
                .overlays
                .push(Overlay::Palette(PaletteState::new(suggested)));
        }
        Action::GenerateReport => {
            if !model.reporting {
                model.reporting = true;
                model.busy_ms = 0;
                return vec![Cmd::RunReport];
            }
        }
        Action::CopySelection => return copy_selection(model),
        Action::FilterList => {
            let target = match model.tab {
                TabId::Project => Some(FilterTarget::Heavy),
                TabId::Machine => Some(FilterTarget::Machine),
                _ => None,
            };
            if let Some(target) = target {
                model.filter = Some(FilterState {
                    target,
                    query: String::new(),
                    typing: true,
                    matches: Vec::new(),
                });
                model.apply_filter();
            }
        }
        Action::CycleSort => match model.tab {
            TabId::Machine => {
                model.machine_sort = model.machine_sort.next();
                data::sort_machine(&mut model.derived.machine, model.machine_sort);
                model.apply_filter();
                model.fire_data_animations();
            }
            TabId::Spend => {
                model.model_sort = model.model_sort.next();
                data::sort_models(&mut model.derived.spend_models, model.model_sort);
            }
            _ => {}
        },
    }
    Vec::new()
}

/// `y`: copy the selection's subject over OSC52. Files and projects copy
/// their path; advisory lines copy their text. A toast confirms.
fn copy_selection(model: &mut Model) -> Vec<Cmd> {
    let payload = match model.overlays.last() {
        Some(Overlay::File(f)) => {
            Some((absolute_path(&model.derived.project_key, &f.path), "path"))
        }
        Some(Overlay::Project(p)) => Some((p.key.clone(), "path")),
        Some(Overlay::Advisory(a)) => Some((a.paragraphs.join("\n"), "advisory")),
        Some(_) => None,
        None => match model.tab {
            TabId::Project => model
                .selected_heavy()
                .map(|r| (absolute_path(&model.derived.project_key, &r.path), "path")),
            TabId::Machine => model.selected_machine().map(|p| (p.key.clone(), "path")),
            TabId::Overview => model
                .derived
                .advisory_lines
                .get(model.sel_overview.idx)
                .map(|l| (l.clone(), "advisory")),
            TabId::Spend if model.spend_focus == SpendFocus::Advisories => model
                .derived
                .advisory_lines
                .get(model.sel_spend.idx)
                .map(|l| (l.clone(), "advisory")),
            TabId::Spend => None,
        },
    };
    let Some((payload, label)) = payload else {
        return Vec::new();
    };
    model.toasts.push(
        ToastKind::Ok,
        format!("copied {label} to clipboard"),
        model.animator.now(),
    );
    vec![Cmd::CopyOsc52(payload)]
}

/// Join a project-relative path onto the project root for copying; paths
/// that are already absolute (or a missing root) pass through.
fn absolute_path(root: &str, path: &str) -> String {
    if root.is_empty() || path.starts_with('/') {
        return path.to_string();
    }
    format!("{}/{}", root.trim_end_matches('/'), path)
}

/// Palette typing rules, applied before keymap resolution while a palette
/// is topmost. Returns `None` for keys the palette does not consume (esc
/// and ctrl+c resolve through the PaletteInput context).
fn handle_palette_key(model: &mut Model, key: &KeyEvent) -> Option<Vec<Cmd>> {
    let Some(Overlay::Palette(state)) = model.overlays.last_mut() else {
        return None;
    };
    match key.code {
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
            state.query.push(c);
            state.refilter();
            Some(Vec::new())
        }
        KeyCode::Backspace => {
            state.query.pop();
            state.refilter();
            Some(Vec::new())
        }
        KeyCode::Down => {
            if state.sel + 1 < state.matches.len() {
                state.sel += 1;
            }
            Some(Vec::new())
        }
        KeyCode::Up => {
            state.sel = state.sel.saturating_sub(1);
            Some(Vec::new())
        }
        KeyCode::Enter => {
            let action = state.matches.get(state.sel).copied();
            model.overlays.pop();
            match action {
                Some(action) => Some(handle_action(model, action)),
                None => Some(Vec::new()),
            }
        }
        _ => None,
    }
}

/// Filter typing rules, applied before keymap resolution while a filter
/// line owns input. Returns `None` for keys it does not consume (esc and
/// ctrl+c resolve through the FilterInput context).
fn handle_filter_key(model: &mut Model, key: &KeyEvent) -> Option<Vec<Cmd>> {
    if !model.overlays.is_empty() {
        return None;
    }
    {
        let filter = model.filter.as_mut()?;
        if !filter.typing {
            return None;
        }
        match key.code {
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                filter.query.push(c);
            }
            KeyCode::Backspace => {
                filter.query.pop();
            }
            KeyCode::Enter => {
                // Confirm: keep the filter applied, hand keys back to the
                // list (esc clears it from there).
                filter.typing = false;
                return Some(Vec::new());
            }
            _ => return None,
        }
    }
    model.apply_filter();
    Some(Vec::new())
}

/// Wheel lines per scroll notch.
const WHEEL_LINES: usize = 3;

/// Mouse wiring: wheel scrolls the focused list, click selects a row or
/// switches tabs, click outside a dialog closes it. Hit-testing recomputes
/// the same layout math the screens render with.
fn handle_mouse(model: &mut Model, mouse: &MouseEvent) -> Vec<Cmd> {
    match mouse.kind {
        MouseEventKind::ScrollDown if model.overlays.is_empty() => {
            if let Some((sel, len)) = model.active_list() {
                for _ in 0..WHEEL_LINES {
                    sel.down(len);
                }
            }
        }
        MouseEventKind::ScrollUp if model.overlays.is_empty() => {
            if let Some((sel, _)) = model.active_list() {
                for _ in 0..WHEEL_LINES {
                    sel.up();
                }
            }
        }
        MouseEventKind::Down(MouseButton::Left) => {
            return handle_click(model, mouse.column, mouse.row)
        }
        _ => {}
    }
    Vec::new()
}

fn handle_click(model: &mut Model, x: u16, y: u16) -> Vec<Cmd> {
    let (w, h) = model.viewport;
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let area = Rect::new(0, 0, w, h);
    let pos = Position { x, y };

    // Overlay open: a click outside the topmost dialog closes it; clicks
    // inside stay with the dialog.
    if !model.overlays.is_empty() {
        if !top_overlay_rect(model, area).contains(pos) {
            model.overlays.pop();
        }
        return Vec::new();
    }

    // Header tab strip (row 0 carries the labels).
    if y == area.y {
        if let Some(tab) = chrome::tab_at(x) {
            model.set_tab(tab);
        }
        return Vec::new();
    }

    // Below the minimum size the body renders the too-small card.
    if area.width < 80 || area.height < 24 || model.derived.setup_needed {
        return Vec::new();
    }
    let body = inset(chrome_rows(area)[1]);
    match model.tab {
        TabId::Overview => {
            let inner = screens::overview::advisory_list_inner(body, &model.theme);
            click_list(
                pos,
                inner,
                &mut model.sel_overview,
                model.derived.advisory_lines.len(),
            );
        }
        TabId::Project => {
            let len = model.visible_len(FilterTarget::Heavy, model.derived.heavy.len());
            let filtered = model.filter_line(FilterTarget::Heavy).is_some();
            let inner = screens::project::heavy_list_inner(body, filtered, &model.theme);
            click_list(pos, inner, &mut model.sel_heavy, len);
        }
        TabId::Machine => {
            let len = model.visible_len(FilterTarget::Machine, model.derived.machine.len());
            let filtered = model.filter_line(FilterTarget::Machine).is_some();
            let inner = screens::machine::projects_list_inner(body, filtered, &model.theme);
            click_list(pos, inner, &mut model.sel_machine, len);
        }
        TabId::Spend if model.derived.ingestion_on => {
            let inner = screens::spend::advisory_list_inner(body, &model.theme);
            if inner.contains(pos) {
                model.spend_focus = SpendFocus::Advisories;
                click_list(
                    pos,
                    inner,
                    &mut model.sel_spend,
                    model.derived.advisory_lines.len(),
                );
            }
        }
        TabId::Spend => {}
    }
    Vec::new()
}

/// Move a list selection to the clicked row, honoring the same scroll
/// window the renderer used.
fn click_list(pos: Position, inner: Rect, sel: &mut Selection, len: usize) {
    if len == 0 || !inner.contains(pos) {
        return;
    }
    let (start, _) = super::components::list::visible_window(sel.idx, len, inner.height as usize);
    let row = start + (pos.y - inner.y) as usize;
    if row < len {
        sel.idx = row;
    }
}

/// The topmost overlay's dialog rectangle, recomputed with the exact
/// geometry the render path uses.
fn top_overlay_rect(model: &Model, area: Rect) -> Rect {
    let overlay = model.overlays.last().expect("caller checks non-empty");
    if let Overlay::Palette(state) = overlay {
        return palette::palette_rect(area, state.matches.len());
    }
    let (_, width) = overlay_chrome(overlay);
    let body = overlay_body(overlay, area, model.now_epoch, &model.theme);
    modal::dialog_rect(area, width, body.len() as u16)
}

/// Enter: with a dialog open it confirms (dismisses) it; otherwise it opens
/// the detail for the focused list's selection.
fn open_detail(model: &mut Model) -> Vec<Cmd> {
    if model.overlays.pop().is_some() {
        return Vec::new();
    }
    match model.tab {
        TabId::Project => {
            if let Some(row) = model.selected_heavy().cloned() {
                model.overlays.push(Overlay::File(FileDetailState {
                    path: row.path,
                    tokens: row.tokens,
                    pct_always: row.pct_always,
                    scan_findings: row.findings,
                    scan_savings: (row.savings_min, row.savings_max),
                    audit: AuditState::NotRun,
                }));
            }
        }
        TabId::Machine => {
            if let Some(p) = model.selected_machine().cloned() {
                let records = model
                    .snapshot
                    .as_deref()
                    .map(|s| s.records.as_slice())
                    .unwrap_or(&[]);
                if let Some(h) = data::project_history(records, &p.key) {
                    model.overlays.push(Overlay::Project(ProjectDetailState {
                        key: p.key,
                        series: h.series,
                        first_ts: h.first_ts,
                        last_ts: h.last_ts,
                        sessions: p.sessions,
                    }));
                }
            }
        }
        TabId::Overview => open_advisory_detail(model, model.sel_overview.idx),
        TabId::Spend if model.spend_focus == SpendFocus::Advisories => {
            open_advisory_detail(model, model.sel_spend.idx)
        }
        // The day strip's detail is its caption line; enter stays quiet.
        TabId::Spend => {}
    }
    Vec::new()
}

fn open_advisory_detail(model: &mut Model, idx: usize) {
    let Some(block) = model.derived.advisories.as_ref() else {
        return;
    };
    // Sections are index-aligned with the compact lines the lists render.
    let mut sections = advisories::tui_detail_sections(block);
    if idx < sections.len() {
        let section = sections.swap_remove(idx);
        model.overlays.push(Overlay::Advisory(AdvisoryDetailState {
            title: section.title,
            paragraphs: section.paragraphs,
        }));
    }
}

/// `a`: audit the file under the open file-detail modal, or the heavy-files
/// selection (opening its detail). The worker reports back via AuditDone.
fn audit_selected(model: &mut Model) -> Vec<Cmd> {
    let root = PathBuf::from(&model.derived.project_key);
    if let Some(overlay) = model.overlays.last_mut() {
        if let Overlay::File(f) = overlay {
            if !matches!(f.audit, AuditState::Running) {
                f.audit = AuditState::Running;
                return vec![Cmd::RunAudit {
                    root,
                    path: f.path.clone(),
                }];
            }
        }
        return Vec::new();
    }
    if model.tab != TabId::Project {
        return Vec::new();
    }
    let Some(row) = model.selected_heavy().cloned() else {
        return Vec::new();
    };
    let path = row.path.clone();
    model.overlays.push(Overlay::File(FileDetailState {
        path: path.clone(),
        tokens: row.tokens,
        pct_always: row.pct_always,
        scan_findings: row.findings,
        scan_savings: (row.savings_min, row.savings_max),
        audit: AuditState::Running,
    }));
    vec![Cmd::RunAudit { root, path }]
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
    let rows = chrome_rows(area);

    let data_age = model
        .derived
        .loaded_at
        .map(|t| format::relative_time(model.now_epoch, t));
    let busy_label = if model.refreshing {
        Some("reloading")
    } else if matches!(model.scan, ScanState::Scanning) {
        Some("scanning repo")
    } else if model.reporting {
        Some("rendering report")
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
        update: model.derived.update_available.as_deref(),
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
        if let Overlay::Palette(state) = overlay {
            palette::render_palette(frame, state, &model.theme);
            continue;
        }
        let (title, width) = overlay_chrome(overlay);
        let body = overlay_body(overlay, area, model.now_epoch, &model.theme);
        modal::render_modal(frame, &title, &body, width, &model.theme);
    }

    // Toasts stack above everything, including dialog scrims.
    if !model.toasts.is_empty() {
        toast::render_toasts(frame, &model.toasts, &model.theme);
    }
}

/// The header/body/footer vertical split, shared by view and the mouse
/// hit-testing path so a click can never disagree with the layout.
fn chrome_rows(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(2),
    ])
    .split(area)
}

/// Title and dialog width per overlay kind. Shared by render and (later)
/// mouse hit-testing. The palette draws its own dialog; its entry here
/// exists only to keep the function total.
fn overlay_chrome(overlay: &Overlay) -> (String, ModalWidth) {
    match overlay {
        Overlay::File(f) => {
            let (_, name) = format::split_path(&f.path);
            (format!("file: {name}"), ModalWidth::Large)
        }
        Overlay::Project(p) => {
            let (_, name) = format::split_path(&p.key);
            (format!("project: {name}"), ModalWidth::Medium)
        }
        // Advisory paragraphs carry the longest copy; the xlarge width keeps
        // wrapping comfortable on wide terminals and clamps on narrow ones.
        Overlay::Advisory(a) => (format!("advisory: {}", a.title), ModalWidth::XLarge),
        Overlay::Help => ("help".to_string(), ModalWidth::Large),
        Overlay::Palette(_) => ("commands".to_string(), ModalWidth::Medium),
    }
}

/// Build one overlay's body lines from its plain state. Styling happens
/// here per frame; the data itself was resolved when the overlay opened.
fn overlay_body(
    overlay: &Overlay,
    area: Rect,
    now_epoch: u64,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let (_, width) = overlay_chrome(overlay);
    let wrap = (width.cols().min(area.width.saturating_sub(2)) as usize).saturating_sub(4);
    match overlay {
        Overlay::File(f) => file_detail_body(f, theme),
        Overlay::Project(p) => project_detail_body(p, wrap, now_epoch, theme),
        Overlay::Advisory(a) => advisory_detail_body(a, wrap, theme),
        Overlay::Help => help::lines(theme),
        // The palette renders through its own dialog, never this path.
        Overlay::Palette(_) => Vec::new(),
    }
}

fn path_line(path: &str, theme: &Theme) -> Line<'static> {
    let (dir, name) = format::split_path(path);
    Line::from(vec![
        Span::styled(dir.to_string(), Style::default().fg(theme.faint)),
        Span::styled(
            name.to_string(),
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        ),
    ])
}

fn kv_span(label: &str, value: String, theme: &Theme) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{label:<11}"), Style::default().fg(theme.muted)),
        Span::styled(value, Style::default().fg(theme.text)),
    ])
}

/// Findings shown inline in the file detail before the list elides.
const DETAIL_FINDINGS_CAP: usize = 6;

fn file_detail_body(f: &FileDetailState, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = vec![path_line(&f.path, theme), Line::default()];
    lines.push(kv_span(
        "tokens",
        format!(
            "{} tok ({:.1}% of always-loaded)",
            format::commas(f.tokens),
            f.pct_always
        ),
        theme,
    ));
    let scan_value = if f.scan_findings == 0 {
        "no findings from the last scan".to_string()
    } else {
        format!(
            "{} finding(s), ~{}-{} tok reclaimable (advisory)",
            f.scan_findings,
            format::commas(f.scan_savings.0),
            format::commas(f.scan_savings.1)
        )
    };
    lines.push(kv_span("scan", scan_value, theme));
    lines.push(Line::default());
    match &f.audit {
        AuditState::NotRun => lines.push(Line::from(Span::styled(
            "audit not run for this file (press a to audit now)".to_string(),
            Style::default().fg(theme.muted),
        ))),
        AuditState::Running => lines.push(Line::from(Span::styled(
            "auditing...".to_string(),
            Style::default().fg(theme.accent),
        ))),
        AuditState::Failed(msg) => lines.push(Line::from(Span::styled(
            format!("audit failed: {msg}"),
            Style::default().fg(theme.err),
        ))),
        AuditState::Done(report) => {
            if report.findings.is_empty() {
                lines.push(Line::from(Span::styled(
                    "audit: clean, no findings".to_string(),
                    Style::default().fg(theme.ok),
                )));
            } else {
                lines.push(Line::from(Span::styled(
                    format!(
                        "audit: {} finding(s), ~{}-{} tok reclaimable (advisory)",
                        report.findings.len(),
                        format::commas(report.total_savings_min),
                        format::commas(report.total_savings_max)
                    ),
                    Style::default().fg(theme.accent),
                )));
                for finding in report.findings.iter().take(DETAIL_FINDINGS_CAP) {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  [{}] ", finding.severity),
                            Style::default().fg(severity_color(&finding.severity, theme)),
                        ),
                        Span::styled(finding.rule.clone(), Style::default().fg(theme.text)),
                        Span::styled(
                            format!(
                                "  ~{}-{} tok",
                                format::commas(finding.savings_min),
                                format::commas(finding.savings_max)
                            ),
                            Style::default().fg(theme.muted),
                        ),
                    ]));
                }
                if report.findings.len() > DETAIL_FINDINGS_CAP {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "  +{} more (tolkin audit {})",
                            report.findings.len() - DETAIL_FINDINGS_CAP,
                            f.path
                        ),
                        Style::default().fg(theme.faint),
                    )));
                }
            }
        }
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "next: a audit   y copy path   esc close".to_string(),
        Style::default().fg(theme.faint),
    )));
    lines
}

fn severity_color(severity: &str, theme: &Theme) -> ratatui::style::Color {
    match severity {
        "high" => theme.err,
        "medium" => theme.warn,
        _ => theme.muted,
    }
}

fn project_detail_body(
    p: &ProjectDetailState,
    wrap: usize,
    now_epoch: u64,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut lines = vec![path_line(&p.key, theme), Line::default()];
    let first = p.series.first().copied().unwrap_or(0);
    let last = p.series.last().copied().unwrap_or(0);
    lines.push(Line::from(vec![
        Span::styled(
            bars::spark_string(&p.series, wrap.saturating_sub(24)),
            Style::default().fg(theme.bar_fill),
        ),
        Span::styled(
            format!(
                "  {} -> {} tok always",
                format::humanize_tokens(first),
                format::humanize_tokens(last)
            ),
            Style::default().fg(theme.muted),
        ),
    ]));
    lines.push(Line::default());
    lines.push(kv_span(
        "first seen",
        format!(
            "{} ({})",
            format::relative_time_ago(now_epoch, p.first_ts),
            data::day_string(p.first_ts)
        ),
        theme,
    ));
    lines.push(kv_span(
        "last seen",
        format::relative_time_ago(now_epoch, p.last_ts),
        theme,
    ));
    lines.push(kv_span("snapshots", p.series.len().to_string(), theme));
    lines.push(kv_span("sessions", p.sessions.to_string(), theme));
    lines.push(Line::default());
    for hint in format::wrap_text(
        &format!("next: cd {} && tolkin project", p.key),
        wrap.max(8),
    ) {
        lines.push(Line::from(Span::styled(
            hint,
            Style::default().fg(theme.faint),
        )));
    }
    lines
}

fn advisory_detail_body(a: &AdvisoryDetailState, wrap: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    for (i, paragraph) in a.paragraphs.iter().enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        // The first paragraph is the load-bearing copy; lever sentences
        // after it render muted.
        let color = if i == 0 { theme.text } else { theme.muted };
        for row in format::wrap_text(paragraph, wrap.max(8)) {
            lines.push(Line::from(Span::styled(row, Style::default().fg(color))));
        }
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "no advisory detail".to_string(),
            Style::default().fg(theme.faint),
        )));
    }
    lines
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
        model.overlays.push(Overlay::Advisory(AdvisoryDetailState {
            title: "detail".to_string(),
            paragraphs: vec!["x".to_string()],
        }));
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
    fn actions_without_data_are_safe_no_ops() {
        let (mut model, _clock) = test_model();
        // Enter, audit, and copy need a selection; filter and sort are
        // wired later in this wave. None of them may panic or dialog.
        for code in [
            KeyCode::Enter,
            KeyCode::Char('a'),
            KeyCode::Char('y'),
            KeyCode::Char('/'),
            KeyCode::Char(','),
        ] {
            assert!(update(&mut model, key(code)).is_empty());
            assert!(model.overlays.is_empty(), "{code:?} must not open a dialog");
        }
    }

    fn seeded_heavy_row() -> HeavyRow {
        HeavyRow {
            path: ".claude/rules.md".to_string(),
            tokens: 6_300,
            pct_always: 63.0,
            findings: 2,
            savings_min: 100,
            savings_max: 400,
        }
    }

    #[test]
    fn enter_on_heavy_list_opens_file_detail_and_esc_closes() {
        let (mut model, _clock) = test_model();
        model.tab = TabId::Project;
        model.derived.heavy = vec![seeded_heavy_row()];
        model.derived.project_key = "/repo".to_string();
        let cmds = update(&mut model, key(KeyCode::Enter));
        assert!(cmds.is_empty());
        let Some(Overlay::File(f)) = model.overlays.last() else {
            panic!("file detail must open");
        };
        assert_eq!(f.path, ".claude/rules.md");
        assert_eq!(f.tokens, 6_300);
        assert!(matches!(f.audit, AuditState::NotRun));
        // Enter inside the dialog confirms (dismisses) it.
        update(&mut model, key(KeyCode::Enter));
        assert!(model.overlays.is_empty());
    }

    #[test]
    fn audit_key_opens_file_detail_running_and_routes_the_result() {
        let (mut model, _clock) = test_model();
        model.tab = TabId::Project;
        model.derived.heavy = vec![seeded_heavy_row()];
        model.derived.project_key = "/repo".to_string();
        let cmds = update(&mut model, key(KeyCode::Char('a')));
        assert_eq!(
            cmds,
            vec![Cmd::RunAudit {
                root: PathBuf::from("/repo"),
                path: ".claude/rules.md".to_string(),
            }]
        );
        assert!(matches!(
            model.overlays.last(),
            Some(Overlay::File(f)) if matches!(f.audit, AuditState::Running)
        ));
        // A second `a` while running stays quiet (Modal context binding).
        assert!(update(&mut model, key(KeyCode::Char('a'))).is_empty());
        // The worker result lands in the open modal, keyed on the path.
        update(
            &mut model,
            Msg::AuditDone {
                path: ".claude/rules.md".to_string(),
                result: Err("io error".to_string()),
            },
        );
        assert!(matches!(
            model.overlays.last(),
            Some(Overlay::File(f)) if matches!(&f.audit, AuditState::Failed(m) if m == "io error")
        ));
        // After failure, `a` retries from inside the modal.
        let cmds = update(&mut model, key(KeyCode::Char('a')));
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn enter_on_machine_list_opens_project_detail_with_history() {
        use crate::ledger::{self, Config};
        use serde_json::json;
        use std::fs;

        let dir = std::env::temp_dir().join(format!(
            "tolkin-app-detail-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        let cfg = Config::new(true, false);
        ledger::save_config_to(&dir, &cfg).unwrap();
        let project = dir.join("repo");
        fs::create_dir_all(&project).unwrap();
        ledger::append_in(&dir, "project", &project, json!({ "always_tokens": 9_000 })).unwrap();
        ledger::append_in(&dir, "project", &project, json!({ "always_tokens": 7_500 })).unwrap();
        let snapshot = StatsSnapshot::load_in(&dir);

        let clock = ManualClock::new();
        let animator = Animator::new(Box::new(clock.clone()), true);
        let theme = theme::by_name("tolkin-dark", true).unwrap();
        let mut model = Model::new(
            Some(Box::new(snapshot)),
            theme,
            ThemeEnv::default(),
            animator,
            1_780_000_000,
        );
        model.tab = TabId::Machine;
        assert_eq!(model.derived.machine.len(), 1);
        update(&mut model, key(KeyCode::Enter));
        let Some(Overlay::Project(p)) = model.overlays.last() else {
            panic!("project detail must open");
        };
        assert_eq!(p.series, vec![9_000, 7_500]);
        assert!(p.first_ts <= p.last_ts);
        let _ = fs::remove_dir_all(&dir);
    }

    fn advisory_fixture() -> AdvisoryBlock {
        use crate::advisories::{ModelMix, OutputShare};
        AdvisoryBlock {
            label: "measured advisories (ground truth basis)",
            model_mix: ModelMix {
                label: "ground truth",
                priced_spend_total: 10.0,
                unpriced_models_excluded: vec![],
                frontier: None,
                cheap: None,
                frontier_advisory: Some(
                    "advisory (community heuristic): the frontier model opus carries 90.0% of priced spend."
                        .to_string(),
                ),
                cheap_advisory: None,
            },
            output_share: OutputShare {
                label: "ground truth",
                output_tokens: 200,
                output_cost_usd: 1.5,
                priced_bill_usd: 10.0,
                share: 0.15,
                framing: "Output tokens are 15.0% of the priced bill.".to_string(),
            },
            cap_runway: None,
        }
    }

    #[test]
    fn enter_on_advisory_lists_opens_the_aligned_section() {
        let (mut model, _clock) = test_model();
        let block = advisory_fixture();
        model.derived.advisory_lines = advisories::tui_compact_lines(&block);
        model.derived.advisories = Some(block);
        assert_eq!(model.derived.advisory_lines.len(), 1);

        // Overview list.
        update(&mut model, key(KeyCode::Enter));
        let Some(Overlay::Advisory(a)) = model.overlays.last() else {
            panic!("advisory detail must open from Overview");
        };
        assert_eq!(a.title, "output share");
        assert!(a.paragraphs[0].contains("Output tokens are"));
        assert!(a.paragraphs[1].contains("frontier model"));
        update(&mut model, key(KeyCode::Esc));

        // Spend advisories panel (after cycling focus to it).
        update(&mut model, key(KeyCode::Char('4')));
        update(&mut model, key(KeyCode::Char(']')));
        update(&mut model, key(KeyCode::Enter));
        assert!(matches!(model.overlays.last(), Some(Overlay::Advisory(_))));
    }

    #[test]
    fn file_detail_modal_renders_tokens_findings_and_next_actions() {
        let (mut model, _clock) = test_model();
        model.tab = TabId::Project;
        model.derived.heavy = vec![seeded_heavy_row()];
        model.derived.project_key = "/repo".to_string();
        update(&mut model, key(KeyCode::Enter));

        let backend = ratatui::backend::TestBackend::new(110, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| view(frame, &model)).unwrap();
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..30 {
            for x in 0..110 {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.contains("file: rules.md"), "title missing: {text}");
        assert!(text.contains("6,300 tok"), "tokens missing");
        assert!(text.contains("2 finding(s)"), "scan findings missing");
        assert!(text.contains("press a to audit"), "audit hint missing");
        assert!(text.contains("esc close"), "esc affordance missing");
    }

    #[test]
    fn palette_opens_filters_and_executes_actions() {
        let (mut model, _clock) = test_model();
        // ctrl+k opens; the context flips to PaletteInput.
        update(&mut model, ctrl('k'));
        assert!(matches!(model.overlays.last(), Some(Overlay::Palette(_))));
        assert_eq!(model.context(), Context::PaletteInput);
        // Typing q must NOT quit: it edits the query.
        assert!(update(&mut model, key(KeyCode::Char('q'))).is_empty());
        let Some(Overlay::Palette(p)) = model.overlays.last() else {
            panic!("palette stays open");
        };
        assert_eq!(p.query, "q");
        // Backspace then type a filter that lands on cycle theme.
        update(&mut model, key(KeyCode::Backspace));
        for c in "theme".chars() {
            update(&mut model, key(KeyCode::Char(c)));
        }
        let Some(Overlay::Palette(p)) = model.overlays.last() else {
            panic!("palette stays open");
        };
        assert_eq!(p.matches.first(), Some(&Action::CycleTheme));
        // Enter executes the action and closes the palette.
        let before = model.theme.name;
        update(&mut model, key(KeyCode::Enter));
        assert!(model.overlays.is_empty(), "palette closes on execute");
        assert_ne!(model.theme.name, before, "action ran");
    }

    #[test]
    fn palette_opens_with_colon_and_esc_closes_it() {
        let (mut model, _clock) = test_model();
        update(&mut model, key(KeyCode::Char(':')));
        assert!(matches!(model.overlays.last(), Some(Overlay::Palette(_))));
        update(&mut model, key(KeyCode::Esc));
        assert!(model.overlays.is_empty());
        // Quitting from the palette works through ctrl+c.
        update(&mut model, ctrl('k'));
        assert_eq!(update(&mut model, ctrl('c')), vec![Cmd::Quit]);
    }

    #[test]
    fn help_stacks_over_details_and_never_duplicates() {
        let (mut model, _clock) = test_model();
        model.tab = TabId::Project;
        model.derived.heavy = vec![seeded_heavy_row()];
        update(&mut model, key(KeyCode::Enter));
        assert_eq!(model.overlays.len(), 1);
        // ? stacks help over the file detail.
        update(&mut model, key(KeyCode::Char('?')));
        assert_eq!(model.overlays.len(), 2);
        assert!(matches!(model.overlays.last(), Some(Overlay::Help)));
        // A second ? while help is topmost does not stack another copy.
        update(&mut model, key(KeyCode::Char('?')));
        assert_eq!(model.overlays.len(), 2);
        // Esc pops help first, the detail next.
        update(&mut model, key(KeyCode::Esc));
        assert!(matches!(model.overlays.last(), Some(Overlay::File(_))));
        update(&mut model, key(KeyCode::Esc));
        assert!(model.overlays.is_empty());
    }

    #[test]
    fn help_overlay_renders_groups_and_agent_contracts() {
        let (mut model, _clock) = test_model();
        update(&mut model, key(KeyCode::Char('?')));

        let backend = ratatui::backend::TestBackend::new(110, 34);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| view(frame, &model)).unwrap();
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..34 {
            for x in 0..110 {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        for needle in [
            "navigate",
            "act",
            "view",
            "agents",
            "tolkin stats --json --global",
            "tolkin mcp --json",
            "exits 2",
            "NO_COLOR",
        ] {
            assert!(text.contains(needle), "{needle} missing:\n{text}");
        }
    }

    #[test]
    fn palette_overlay_renders_input_and_suggestions() {
        let (mut model, _clock) = test_model();
        update(&mut model, ctrl('k'));
        let backend = ratatui::backend::TestBackend::new(110, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| view(frame, &model)).unwrap();
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..30 {
            for x in 0..110 {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.contains("commands"), "palette title missing");
        assert!(text.contains(">"), "input prompt missing");
        // Suggested actions (List footer hints) float first.
        assert!(
            text.contains("select next row"),
            "suggested action missing:\n{text}"
        );
    }

    fn click(x: u16, y: u16) -> Msg {
        Msg::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn wheel(down: bool) -> Msg {
        Msg::Mouse(MouseEvent {
            kind: if down {
                MouseEventKind::ScrollDown
            } else {
                MouseEventKind::ScrollUp
            },
            column: 10,
            row: 10,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn refresh_and_scan_completion_push_toasts_that_expire() {
        let (mut model, clock) = test_model();
        update(&mut model, key(KeyCode::Char('r')));
        update(&mut model, Msg::SnapshotLoaded(Box::new(None)));
        assert_eq!(model.toasts.iter().count(), 1, "refresh done toast");
        update(
            &mut model,
            Msg::ScanDone {
                result: Err("boom".to_string()),
                at_epoch: 1,
                elapsed_ms: 10,
            },
        );
        assert_eq!(model.toasts.iter().count(), 2, "scan error toast joins");
        // The 5000 ms ttl runs on the animator clock.
        clock.advance(Duration::from_millis(5_001));
        update(
            &mut model,
            Msg::Tick {
                now_epoch: 2,
                delta_ms: 5_001,
            },
        );
        assert!(model.toasts.is_empty(), "toasts pruned after the ttl");
    }

    #[test]
    fn copy_emits_osc52_for_paths_and_advisory_values() {
        let (mut model, _clock) = test_model();
        model.tab = TabId::Project;
        model.derived.project_key = "/repo".to_string();
        model.derived.heavy = vec![seeded_heavy_row()];
        let cmds = update(&mut model, key(KeyCode::Char('y')));
        assert_eq!(
            cmds,
            vec![Cmd::CopyOsc52("/repo/.claude/rules.md".to_string())]
        );
        assert_eq!(model.toasts.iter().count(), 1, "copied toast");

        // Machine rows copy the project key.
        model.tab = TabId::Machine;
        model.derived.machine = vec![MachineProject {
            key: "/repo".to_string(),
            always_tokens: 1,
            last_ts: 0,
            sessions: 2,
        }];
        let cmds = update(&mut model, key(KeyCode::Char('y')));
        assert_eq!(cmds, vec![Cmd::CopyOsc52("/repo".to_string())]);

        // Advisory lines copy their text.
        model.tab = TabId::Overview;
        model.derived.advisory_lines = vec!["output share  15.0% of bill".to_string()];
        let cmds = update(&mut model, key(KeyCode::Char('y')));
        assert_eq!(
            cmds,
            vec![Cmd::CopyOsc52("output share  15.0% of bill".to_string())]
        );

        // Inside the file detail modal, y copies the dialog's path.
        model.tab = TabId::Project;
        update(&mut model, key(KeyCode::Enter));
        let cmds = update(&mut model, key(KeyCode::Char('y')));
        assert_eq!(
            cmds,
            vec![Cmd::CopyOsc52("/repo/.claude/rules.md".to_string())]
        );
    }

    #[test]
    fn report_runs_once_and_toasts_the_artifact_path() {
        let (mut model, _clock) = test_model();
        let cmds = update(&mut model, key(KeyCode::Char('o')));
        assert_eq!(cmds, vec![Cmd::RunReport]);
        assert!(model.reporting && model.is_busy());
        // A second o while the worker runs stays quiet.
        assert!(update(&mut model, key(KeyCode::Char('o'))).is_empty());
        update(
            &mut model,
            Msg::ReportDone(Ok(PathBuf::from("/tmp/tolkin-report.html"))),
        );
        assert!(!model.reporting);
        assert_eq!(model.toasts.iter().count(), 1);
        let text: Vec<&str> = model.toasts.iter().map(|t| t.text.as_str()).collect();
        assert!(text[0].contains("tolkin-report.html"), "{text:?}");
    }

    #[test]
    fn mouse_wheel_scrolls_the_focused_list_three_lines() {
        let (mut model, _clock) = test_model();
        model.derived.advisory_lines = (0..8).map(|i| format!("adv {i}")).collect();
        update(&mut model, wheel(true));
        assert_eq!(model.sel_overview.idx, 3);
        update(&mut model, wheel(true));
        assert_eq!(model.sel_overview.idx, 6);
        update(&mut model, wheel(false));
        assert_eq!(model.sel_overview.idx, 3);
    }

    #[test]
    fn mouse_click_switches_tabs_and_selects_rows() {
        let (mut model, _clock) = test_model();
        model.viewport = (110, 30);
        model.derived.setup_needed = false;
        // Tab strip: click the Machine label.
        let (x, _) = chrome::tab_segments()[2];
        update(&mut model, click(x + 1, 0));
        assert_eq!(model.tab, TabId::Machine);

        // Project heavy list: click the third visible row.
        model.tab = TabId::Project;
        model.derived.heavy = (0..8)
            .map(|i| HeavyRow {
                path: format!("file{i}.md"),
                tokens: 100,
                pct_always: 1.0,
                findings: 0,
                savings_min: 0,
                savings_max: 0,
            })
            .collect();
        let area = Rect::new(0, 0, 110, 30);
        let body = inset(chrome_rows(area)[1]);
        let inner = screens::project::heavy_list_inner(body, false, &model.theme);
        update(&mut model, click(inner.x + 3, inner.y + 2));
        assert_eq!(model.sel_heavy.idx, 2, "row under the cursor selected");
        // A click outside any list leaves the selection alone.
        update(&mut model, click(inner.x + 3, area.height - 1));
        assert_eq!(model.sel_heavy.idx, 2);
    }

    #[test]
    fn mouse_click_outside_a_modal_closes_it_and_inside_keeps_it() {
        let (mut model, _clock) = test_model();
        model.viewport = (110, 30);
        model.tab = TabId::Project;
        model.derived.heavy = vec![seeded_heavy_row()];
        update(&mut model, key(KeyCode::Enter));
        assert_eq!(model.overlays.len(), 1);
        let area = Rect::new(0, 0, 110, 30);
        let rect = top_overlay_rect(&model, area);
        // Click inside the dialog: stays open.
        update(&mut model, click(rect.x + 1, rect.y + 1));
        assert_eq!(model.overlays.len(), 1);
        // Click outside: closes.
        update(&mut model, click(0, area.height - 1));
        assert!(model.overlays.is_empty());
    }

    fn machine_fixture() -> Vec<MachineProject> {
        let mk = |key: &str, always: u64, ts: u64, sessions: u64| MachineProject {
            key: key.to_string(),
            always_tokens: always,
            last_ts: ts,
            sessions,
        };
        vec![
            mk("/a", 10, 300, 2),
            mk("/b", 30, 100, 5),
            mk("/c", 20, 200, 9),
        ]
    }

    #[test]
    fn comma_cycles_machine_and_model_sorts_with_cached_order() {
        let (mut model, _clock) = test_model();
        model.tab = TabId::Machine;
        model.derived.machine = machine_fixture();
        data::sort_machine(&mut model.derived.machine, model.machine_sort);
        let keys = |m: &Model| {
            m.derived
                .machine
                .iter()
                .map(|p| p.key.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(keys(&model), ["/b", "/c", "/a"], "weight default");
        update(&mut model, key(KeyCode::Char(',')));
        assert_eq!(model.machine_sort, data::MachineSort::Sessions);
        assert_eq!(keys(&model), ["/c", "/b", "/a"], "sessions order");
        update(&mut model, key(KeyCode::Char(',')));
        assert_eq!(model.machine_sort, data::MachineSort::Recency);
        assert_eq!(keys(&model), ["/a", "/c", "/b"], "recency order");

        // Spend models cycle independently.
        model.tab = TabId::Spend;
        assert_eq!(model.model_sort, data::ModelSort::Input);
        update(&mut model, key(KeyCode::Char(',')));
        assert_eq!(model.model_sort, data::ModelSort::Output);
        // Machine sort untouched by the spend cycle.
        assert_eq!(model.machine_sort, data::MachineSort::Recency);
    }

    #[test]
    fn slash_filters_the_heavy_list_and_esc_clears_it() {
        let (mut model, _clock) = test_model();
        model.tab = TabId::Project;
        model.derived.project_key = "/repo".to_string();
        model.derived.heavy = vec![
            HeavyRow {
                path: "CLAUDE.md".to_string(),
                tokens: 900,
                pct_always: 9.0,
                findings: 0,
                savings_min: 0,
                savings_max: 0,
            },
            HeavyRow {
                path: "docs/guide.md".to_string(),
                tokens: 500,
                pct_always: 5.0,
                findings: 0,
                savings_min: 0,
                savings_max: 0,
            },
            HeavyRow {
                path: ".claude/rules.md".to_string(),
                tokens: 100,
                pct_always: 1.0,
                findings: 0,
                savings_min: 0,
                savings_max: 0,
            },
        ];
        update(&mut model, key(KeyCode::Char('/')));
        assert_eq!(model.context(), Context::FilterInput);
        // Typing q filters instead of quitting.
        for c in "claude".chars() {
            assert!(update(&mut model, key(KeyCode::Char(c))).is_empty());
        }
        assert_eq!(model.visible_heavy().len(), 2, "case-insensitive substring");
        // Navigation while typing is parked; enter confirms and hands the
        // keys back to the list.
        update(&mut model, key(KeyCode::Enter));
        assert_eq!(model.context(), Context::List);
        update(&mut model, key(KeyCode::Char('j')));
        assert_eq!(model.sel_heavy.idx, 1);
        // Enter opens the detail for the FILTERED selection.
        update(&mut model, key(KeyCode::Enter));
        let Some(Overlay::File(f)) = model.overlays.last() else {
            panic!("detail must open");
        };
        assert_eq!(f.path, ".claude/rules.md", "filter-mapped row");
        update(&mut model, key(KeyCode::Esc));
        // Esc clears the filter; the full list returns.
        update(&mut model, key(KeyCode::Esc));
        assert!(model.filter.is_none());
        assert_eq!(model.visible_heavy().len(), 3);
        // Switching tabs drops a confirmed filter (typing parks tab).
        update(&mut model, key(KeyCode::Char('/')));
        update(&mut model, key(KeyCode::Char('c')));
        update(&mut model, key(KeyCode::Enter));
        update(&mut model, key(KeyCode::Tab));
        assert!(model.filter.is_none());
    }

    #[test]
    fn theme_cycle_persists_only_when_a_config_exists() {
        // No snapshot, no config: cycle without persistence.
        let (mut model, _clock) = test_model();
        assert!(update(&mut model, key(KeyCode::Char('t'))).is_empty());

        // Seeded config: cycle returns the persist command.
        use crate::ledger::{self, Config};
        use std::fs;
        let dir = std::env::temp_dir().join(format!(
            "tolkin-theme-persist-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        ledger::save_config_to(&dir, &Config::new(true, false)).unwrap();
        let snapshot = StatsSnapshot::load_in(&dir);
        let clock = ManualClock::new();
        let animator = Animator::new(Box::new(clock.clone()), true);
        let theme = theme::by_name("tolkin-dark", true).unwrap();
        let mut model = Model::new(
            Some(Box::new(snapshot)),
            theme,
            ThemeEnv::default(),
            animator,
            1_780_000_000,
        );
        let cmds = update(&mut model, key(KeyCode::Char('t')));
        assert_eq!(
            cmds,
            vec![Cmd::PersistTheme("tolkin-light".to_string())],
            "persist through the existing save path"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn update_chip_reads_the_cached_newer_version_without_network() {
        use crate::ledger::{self, Config};
        use std::fs;
        let dir = std::env::temp_dir().join(format!(
            "tolkin-update-chip-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        fs::create_dir_all(&dir).unwrap();
        let mut cfg = Config::new(true, false);
        cfg.last_seen_latest = Some("99.0.0".to_string());
        ledger::save_config_to(&dir, &cfg).unwrap();
        let snapshot = StatsSnapshot::load_in(&dir);
        let theme = theme::by_name("tolkin-dark", true).unwrap();
        let model = Model::compact(Some(Box::new(snapshot)), theme);
        assert_eq!(model.derived.update_available.as_deref(), Some("99.0.0"));

        let backend = ratatui::backend::TestBackend::new(110, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| view(frame, &model)).unwrap();
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..2 {
            for x in 0..110 {
                text.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(text.contains("update 99.0.0"), "chip missing: {text}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn spend_day_caption_shows_the_selected_days_exact_numbers() {
        let (mut model, _clock) = test_model();
        model.derived.setup_needed = false;
        model.derived.ingestion_on = true;
        model.derived.day_details = (0..5)
            .map(|i| DayDetail {
                day: format!("2026-06-0{}", i + 1),
                input_side: 1_000 * (i as u64 + 1),
                input_fresh: 400,
                cache_read: 600,
                output: 50,
                cost_usd: 1.25,
            })
            .collect();
        model.day_cursor = 4;
        model.tab = TabId::Spend;
        update(&mut model, key(KeyCode::Char('h')));
        update(&mut model, key(KeyCode::Char('h')));
        assert_eq!(model.day_cursor, 2);

        let backend = ratatui::backend::TestBackend::new(110, 30);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal.draw(|frame| view(frame, &model)).unwrap();
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..30 {
            for x in 0..110 {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.contains("2026-06-03"), "selected day missing: {text}");
        assert!(text.contains("in 3.0k"), "input total missing");
        assert!(text.contains("fresh 400"), "fresh split missing");
        assert!(text.contains("cache 600"), "cache split missing");
        assert!(text.contains("out 50"), "output missing");
        assert!(text.contains("~$1.25"), "cost missing");
    }
}
