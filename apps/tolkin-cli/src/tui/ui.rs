//! Rendering for the three dashboard tabs. Every numeric surface here carries
//! its tier label (advisory estimate, measured structure, ground truth) so the
//! honesty design holds at the UI layer too.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Row, Sparkline, Table},
    Frame,
};

use crate::advisories::AdvisoryBlock;
use crate::cache_analysis::CacheReport;
use crate::ledger::LedgerRecord;
use crate::project::ProjectReport;
use crate::tiers::TierReport;

use super::data::{self, MachineProject, HONESTY_LINE, REALIZED_SPARK_POINTS, SPEND_DAYS_WINDOW};

/// Which dashboard tab is in front. The order here is the navigation order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Project,
    Machine,
    Spend,
}

impl Tab {
    pub fn titles() -> [&'static str; 3] {
        ["Project", "Machine", "Spend"]
    }
    pub fn index(&self) -> usize {
        match self {
            Tab::Project => 0,
            Tab::Machine => 1,
            Tab::Spend => 2,
        }
    }
    pub fn next(&self) -> Tab {
        match self {
            Tab::Project => Tab::Machine,
            Tab::Machine => Tab::Spend,
            Tab::Spend => Tab::Project,
        }
    }
    pub fn prev(&self) -> Tab {
        match self {
            Tab::Project => Tab::Spend,
            Tab::Machine => Tab::Project,
            Tab::Spend => Tab::Machine,
        }
    }
}

/// Status of the cwd project scan, owned by the dashboard state.
#[derive(Debug)]
pub enum ProjectScanState {
    /// Pure-empty case: no scan ever started (used for the compact frame).
    None,
    Scanning,
    Ready(Box<ProjectReport>),
    Failed(String),
}

/// Aggregate model for rendering. The renderer is supplied with everything it
/// needs and never reaches into the snapshot or env directly.
pub struct DashboardView<'a> {
    pub tab: Tab,
    pub project_key: &'a str,
    pub records: &'a [LedgerRecord],
    pub scan: &'a ProjectScanState,
    pub project_report: &'a TierReport,
    pub global_report: &'a TierReport,
    pub global_projects: &'a [MachineProject],
    pub spend_days: &'a [data::DayBar],
    pub spend_models: &'a [data::ModelRow],
    /// Global cache health report; `None` exactly when ingestion is off.
    pub cache: Option<&'a CacheReport>,
    /// Advisory block (model mix, output share, cap runway); `None` when
    /// ingestion is off or measured tier is absent.
    pub advisories: Option<&'a AdvisoryBlock>,
    pub ingestion_on: bool,
    /// True when the data dir has neither a config nor any ledger records;
    /// the dashboard renders a friendly setup card across every tab.
    pub setup_needed: bool,
    pub rate_model_display: &'a str,
    pub prices_observed: &'a str,
}

/// Render the dashboard chrome plus the active tab.
pub fn render(frame: &mut Frame, view: &DashboardView) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // tabs + project key
            Constraint::Min(0),    // body
            Constraint::Length(2), // footer
        ])
        .split(area);

    render_header(frame, view, chunks[0]);
    if view.setup_needed {
        render_setup(frame, chunks[1]);
    } else {
        match view.tab {
            Tab::Project => render_project_tab(frame, view, chunks[1]),
            Tab::Machine => render_machine_tab(frame, view, chunks[1]),
            Tab::Spend => render_spend_tab(frame, view, chunks[1]),
        }
    }
    render_footer(frame, view, chunks[2]);
}

fn render_header(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let titles = Tab::titles();
    let current = view.tab.index();
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        "tolkin ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));
    for (i, title) in titles.iter().enumerate() {
        let style = if i == current {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(format!(" {title} "), style));
    }
    let tabs_line = Paragraph::new(Line::from(spans));
    frame.render_widget(tabs_line, rows[0]);

    let project = Paragraph::new(Line::from(Span::styled(
        format!("cwd: {}", view.project_key),
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(project, rows[1]);
}

fn render_footer(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let hints = Paragraph::new(Line::from(Span::styled(
        "Tab/arrows tabs, r refresh, q quit",
        Style::default().fg(Color::DarkGray),
    )));
    frame.render_widget(hints, rows[0]);

    let labels = format!(
        "{HONESTY_LINE}. tiers: identified (advisory), realized (measured structure), measured (ground truth). rate: {} prices {}.",
        view.rate_model_display, view.prices_observed
    );
    let bar = Paragraph::new(Line::from(Span::styled(
        labels,
        Style::default().fg(Color::Yellow),
    )));
    frame.render_widget(bar, rows[1]);
}

fn render_setup(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(Span::styled(
            "No ledger or config yet",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::default(),
        Line::from("Tolkin runs entirely on your machine. Nothing leaves."),
        Line::default(),
        Line::from(Span::styled(
            "Try these next:",
            Style::default().fg(Color::White),
        )),
        Line::from("  tolkin init       set up the local savings ledger"),
        Line::from("  tolkin scan       discover local agent configs"),
        Line::from("  tolkin project    audit this repo's agent-context weight"),
        Line::default(),
        Line::from(Span::styled(
            "Press q to quit.",
            Style::default().fg(Color::DarkGray),
        )),
    ];
    let block = Block::default().borders(Borders::ALL).title(" Setup ");
    let para = Paragraph::new(text).block(block);
    frame.render_widget(para, area);
}

fn render_project_tab(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // load profile bars
            Constraint::Min(6),    // heavy files
            Constraint::Length(7), // tier summary + sparkline
        ])
        .split(area);

    render_load_profiles(frame, view, rows[0]);
    render_heavy_files(frame, view, rows[1]);
    render_project_savings(frame, view, rows[2]);
}

fn render_load_profiles(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Load profile (always, on-invocation, on-demand, docs) ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Pull from the live scan if ready, else from the latest ledger snapshot
    // for the cwd. Either way the four buckets exist with a tokens count.
    let (always, invocation, demand, docs, source_label) = match view.scan {
        ProjectScanState::Ready(report) => (
            report.profiles.always.tokens,
            report.profiles.on_invocation.tokens,
            report.profiles.on_demand.tokens,
            report.profiles.docs.tokens,
            "live scan",
        ),
        ProjectScanState::Scanning => {
            let para = Paragraph::new(Line::from(Span::styled(
                "scanning the repo...",
                Style::default().fg(Color::DarkGray),
            )));
            frame.render_widget(para, inner);
            return;
        }
        ProjectScanState::Failed(msg) => {
            let para = Paragraph::new(Line::from(Span::styled(
                format!("scan failed: {msg}"),
                Style::default().fg(Color::Red),
            )));
            frame.render_widget(para, inner);
            return;
        }
        ProjectScanState::None => {
            let para = Paragraph::new(Line::from(Span::styled(
                "no scan",
                Style::default().fg(Color::DarkGray),
            )));
            frame.render_widget(para, inner);
            return;
        }
    };
    let max = always.max(invocation).max(demand).max(docs);
    let bar_width = inner.width.saturating_sub(28);
    let label_lines = vec![
        bar_line("always       ", always, max, bar_width),
        bar_line("on-invocation", invocation, max, bar_width),
        bar_line("on-demand    ", demand, max, bar_width),
        bar_line("docs         ", docs, max, bar_width),
        Line::from(Span::styled(
            format!("source: {source_label}"),
            Style::default().fg(Color::DarkGray),
        )),
    ];
    frame.render_widget(Paragraph::new(label_lines), inner);
}

fn bar_line(label: &str, value: u64, max: u64, bar_width: u16) -> Line<'static> {
    let filled = data::scale_bar(value, max, bar_width);
    let empty = bar_width.saturating_sub(filled);
    let mut spans = vec![
        Span::styled(format!("{label}  "), Style::default().fg(Color::White)),
        Span::styled(
            "\u{2588}".repeat(filled as usize),
            Style::default().fg(Color::Cyan),
        ),
        Span::styled(
            "\u{2591}".repeat(empty as usize),
            Style::default().fg(Color::DarkGray),
        ),
    ];
    spans.push(Span::styled(
        format!("  {} tokens", commas(value)),
        Style::default().fg(Color::White),
    ));
    Line::from(spans)
}

fn render_heavy_files(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Heaviest agent-context files ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines: Vec<Line> = match view.scan {
        ProjectScanState::Ready(report) => data::top_heavy_files(report)
            .into_iter()
            .map(|(path, tokens)| {
                Line::from(vec![
                    Span::styled(
                        format!(
                            "{:<width$}",
                            path,
                            width = (inner.width as usize).saturating_sub(16)
                        ),
                        Style::default().fg(Color::White),
                    ),
                    Span::styled(
                        format!("{:>10} tok", commas(tokens)),
                        Style::default().fg(Color::Cyan),
                    ),
                ])
            })
            .collect(),
        ProjectScanState::Scanning => vec![Line::from(Span::styled(
            "scanning...",
            Style::default().fg(Color::DarkGray),
        ))],
        ProjectScanState::Failed(msg) => vec![Line::from(Span::styled(
            format!("scan failed: {msg}"),
            Style::default().fg(Color::Red),
        ))],
        ProjectScanState::None => vec![Line::from(Span::styled(
            "no live scan",
            Style::default().fg(Color::DarkGray),
        ))],
    };
    let para = if lines.is_empty() {
        Paragraph::new(Line::from(Span::styled(
            "no agent-context files found",
            Style::default().fg(Color::DarkGray),
        )))
    } else {
        Paragraph::new(lines)
    };
    frame.render_widget(para, inner);
}

fn render_project_savings(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Savings (this project) ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(2)])
        .split(inner);

    let mut header_lines: Vec<Line> = Vec::new();
    match &view.project_report.identified {
        Some(id) => {
            let min = id.project_reclaimable_min.unwrap_or(0);
            let max = id.project_reclaimable_max.unwrap_or(0);
            header_lines.push(Line::from(vec![
                Span::styled("identified ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("({})  ", id.label),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("~{}-{} tokens reclaimable", commas(min), commas(max)),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
        }
        None => header_lines.push(Line::from(Span::styled(
            "identified: nothing recorded yet",
            Style::default().fg(Color::DarkGray),
        ))),
    }
    match &view.project_report.realized {
        Some(r) => {
            let sign = if r.tokens >= 0 { "" } else { "-" };
            header_lines.push(Line::from(vec![
                Span::styled("realized   ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("({})  ", r.label),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!(
                        "{sign}{} input tok over {} sessions, basis: {}",
                        commas(r.tokens.unsigned_abs()),
                        r.sessions_count,
                        r.sessions_basis
                    ),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
        }
        None => header_lines.push(Line::from(Span::styled(
            "realized: needs two project snapshots (re-run tolkin project)",
            Style::default().fg(Color::DarkGray),
        ))),
    }
    frame.render_widget(Paragraph::new(header_lines), layout[0]);

    let series = data::realized_sparkline_for_project(view.records, view.project_key);
    if series.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "no realized trend (one snapshot or fewer)",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(para, layout[1]);
    } else {
        let tail: Vec<u64> = series
            .iter()
            .rev()
            .take(REALIZED_SPARK_POINTS)
            .copied()
            .collect::<Vec<u64>>()
            .into_iter()
            .rev()
            .collect();
        let spark = Sparkline::default()
            .data(&tail)
            .max(*tail.iter().max().unwrap_or(&1))
            .bar_set(symbols::bar::NINE_LEVELS)
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(spark, layout[1]);
    }
}

fn render_machine_tab(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(4), Constraint::Min(0)])
        .split(area);
    render_global_summary(frame, view, rows[0]);
    render_projects_list(frame, view, rows[1]);
}

fn render_global_summary(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Global totals ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines: Vec<Line> = Vec::new();
    match &view.global_report.identified {
        Some(id) => {
            let min = id.project_reclaimable_min.unwrap_or(0);
            let max = id.project_reclaimable_max.unwrap_or(0);
            lines.push(Line::from(vec![
                Span::styled("identified ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("({})  ", id.label),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!(
                        "~{}-{} tokens across {} projects",
                        commas(min),
                        commas(max),
                        id.projects
                    ),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
        }
        None => lines.push(Line::from(Span::styled(
            "identified: no projects recorded yet",
            Style::default().fg(Color::DarkGray),
        ))),
    }
    match &view.global_report.realized {
        Some(r) => {
            let sign = if r.tokens >= 0 { "" } else { "-" };
            lines.push(Line::from(vec![
                Span::styled("realized   ", Style::default().fg(Color::White)),
                Span::styled(
                    format!("({})  ", r.label),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!(
                        "{sign}{} input tok across {} sessions",
                        commas(r.tokens.unsigned_abs()),
                        r.sessions_count
                    ),
                    Style::default().fg(Color::Cyan),
                ),
            ]));
        }
        None => lines.push(Line::from(Span::styled(
            "realized: needs two snapshots per project",
            Style::default().fg(Color::DarkGray),
        ))),
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_projects_list(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Projects by latest always-loaded weight ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if view.global_projects.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "no projects recorded yet",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(para, inner);
        return;
    }
    let max = view
        .global_projects
        .iter()
        .map(|p| p.always_tokens)
        .max()
        .unwrap_or(0);
    let bar_width = inner.width.saturating_sub(48);
    let lines: Vec<Line> = view
        .global_projects
        .iter()
        .take(inner.height as usize)
        .map(|p| {
            let filled = data::scale_bar(p.always_tokens, max, bar_width);
            let empty = bar_width.saturating_sub(filled);
            let trimmed = truncate_left(&p.key, 24);
            let sessions = if view.ingestion_on {
                format!(" sess {:>4}", p.sessions)
            } else {
                String::new()
            };
            Line::from(vec![
                Span::styled(
                    format!("{:<24} ", trimmed),
                    Style::default().fg(Color::White),
                ),
                Span::styled(
                    "\u{2588}".repeat(filled as usize),
                    Style::default().fg(Color::Cyan),
                ),
                Span::styled(
                    "\u{2591}".repeat(empty as usize),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!(" {:>10} tok{sessions}", commas(p.always_tokens)),
                    Style::default().fg(Color::White),
                ),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_spend_tab(frame: &mut Frame, view: &DashboardView, area: Rect) {
    if !view.ingestion_on {
        let block = Block::default().borders(Borders::ALL).title(" Spend ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let lines = vec![
            Line::from(Span::styled(
                "Usage-log ingestion is off.",
                Style::default().fg(Color::Yellow),
            )),
            Line::default(),
            Line::from("Run `tolkin init` and consent to log ingestion to populate this tab."),
            Line::from("Logs read from ~/.claude/projects and ~/.codex/sessions, locally only."),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7), // daily bars
            Constraint::Length(9), // model table
            Constraint::Length(5), // cache hit rate + health + cost
            Constraint::Min(2),    // advisories compact lines
        ])
        .split(area);

    render_daily_bars(frame, view, rows[0]);
    render_model_table(frame, view, rows[1]);
    render_cache_and_cost(frame, view, rows[2]);
    render_advisories_compact(frame, view, rows[3]);
}

/// The one-line cache health row for the Spend tab: the broken-cache
/// advisory when any active day fell under the threshold, otherwise an ok
/// line, always closing with the abbreviated lever clause from the scope
/// line (`tolkin cache` prints the full sentence and every number).
fn cache_health_line(report: &CacheReport) -> Line<'static> {
    let (text, color) = if report.hit_rate.advisory.is_some() {
        (
            format!(
                "hit rate under {:.2} on {} active day(s): caching likely broken; levers: prefix stability, session shape",
                report.hit_rate.threshold,
                report.hit_rate.days_below_threshold.len()
            ),
            Color::Yellow,
        )
    } else {
        (
            format!(
                "ok, no active day under the {:.2} threshold; levers: prefix stability, session shape",
                report.hit_rate.threshold
            ),
            Color::White,
        )
    };
    Line::from(vec![
        Span::styled("cache      ", Style::default().fg(Color::White)),
        Span::styled(text, Style::default().fg(color)),
    ])
}

fn render_daily_bars(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let block = Block::default().borders(Borders::ALL).title(format!(
        " Daily input tokens (last {SPEND_DAYS_WINDOW} UTC days) "
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if view.spend_days.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "no usage data",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(para, inner);
        return;
    }
    let values: Vec<u64> = view.spend_days.iter().map(|d| d.input_side).collect();
    let max = *values.iter().max().unwrap_or(&0);
    let spark = Sparkline::default()
        .data(&values)
        .max(max.max(1))
        .bar_set(symbols::bar::NINE_LEVELS)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(spark, inner);
}

fn render_model_table(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Models (top 5 by input volume) ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if view.spend_models.is_empty() {
        let para = Paragraph::new(Line::from(Span::styled(
            "no measured sessions in scope",
            Style::default().fg(Color::DarkGray),
        )));
        frame.render_widget(para, inner);
        return;
    }
    let header = Row::new(vec!["model", "in (incl cache)", "out", "cost"]).style(
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    );
    let rows: Vec<Row> = view
        .spend_models
        .iter()
        .map(|m| {
            let cost = match m.cost_usd {
                Some(c) => format_usd(c),
                None => "unpriced".to_string(),
            };
            Row::new(vec![
                m.model.clone(),
                commas(m.totals.input_side()),
                commas(m.totals.output_tokens),
                cost,
            ])
        })
        .collect();
    let widths = [
        Constraint::Percentage(40),
        Constraint::Percentage(22),
        Constraint::Percentage(18),
        Constraint::Percentage(20),
    ];
    let table = Table::new(rows, widths).header(header);
    frame.render_widget(table, inner);
}

fn render_cache_and_cost(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Cache hit rate, cost ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines: Vec<Line> = Vec::new();
    if let Some(m) = &view.global_report.measured {
        let hit = m.cache_hit_rate * 100.0;
        let bar_width = inner.width.saturating_sub(16);
        let filled = data::scale_bar((hit as u64).min(100), 100, bar_width);
        let empty = bar_width.saturating_sub(filled);
        lines.push(Line::from(vec![
            Span::styled("cache hit  ", Style::default().fg(Color::White)),
            Span::styled(
                "\u{2588}".repeat(filled as usize),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                "\u{2591}".repeat(empty as usize),
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(format!(" {hit:>5.1}%"), Style::default().fg(Color::White)),
        ]));
        if let Some(cache_report) = view.cache {
            lines.push(cache_health_line(cache_report));
        }
        lines.push(Line::from(vec![
            Span::styled("cost       ", Style::default().fg(Color::White)),
            Span::styled(
                format_usd(m.cost_usd_total),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!(" (priced models only, prices {})", view.prices_observed),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            "no measured sessions yet",
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_advisories_compact(frame: &mut Frame, view: &DashboardView, area: Rect) {
    let Some(block) = view.advisories else {
        return;
    };
    let compact = crate::advisories::tui_compact_lines(block);
    if compact.is_empty() {
        return;
    }
    let lines: Vec<Line> = compact
        .iter()
        .map(|l| Line::from(Span::styled(l.as_str(), Style::default().fg(Color::White))))
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

fn commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(char::from(*b));
    }
    out
}

fn format_usd(v: f64) -> String {
    if v == 0.0 {
        "$0".to_string()
    } else if v < 0.01 {
        format!("${v:.5}")
    } else if v < 1.0 {
        format!("${v:.3}")
    } else {
        format!("${v:.2}")
    }
}

/// Show the rightmost characters of a path that does not fit, with a leading
/// ellipsis. Plain ASCII for portability with terminals that lack the U+2026.
fn truncate_left(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let take = width.saturating_sub(3);
    let suffix: String = s
        .chars()
        .rev()
        .take(take)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_left_keeps_short_strings() {
        assert_eq!(truncate_left("abc", 10), "abc");
    }

    #[test]
    fn truncate_left_ellipsizes_long_strings() {
        let out = truncate_left("/very/deep/nested/path/to/repo", 12);
        assert!(out.starts_with("..."));
        assert_eq!(out.chars().count(), 12);
    }

    #[test]
    fn format_usd_thresholds() {
        assert_eq!(format_usd(0.0), "$0");
        assert_eq!(format_usd(0.004), "$0.00400");
        assert_eq!(format_usd(0.5), "$0.500");
        assert_eq!(format_usd(12.345), "$12.35");
    }
}
