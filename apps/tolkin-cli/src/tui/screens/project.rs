//! Project tab: load profile bars, the selectable heavy-files list, and
//! the savings panel with tier-tagged lines plus the realized sparkline.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Sparkline};
use ratatui::Frame;

use crate::tui::anim::AnimKey;
use crate::tui::app::{reveal, FilterTarget, Model, ScanState};
use crate::tui::components::bars;
use crate::tui::components::list::render_select_list;
use crate::tui::data::REALIZED_SPARK_POINTS;
use crate::tui::format;

use crate::tui::theme::Theme;

use super::{muted_line, panel};

/// Vertical body split: load profile, gap, heavy files, gap, savings.
/// Shared by render and the mouse hit-testing helper below.
fn rows(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(8),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(7),
    ])
    .split(area)
}

/// Inner rect of the heavy-files select-list for mouse hit-testing;
/// mirrors render()'s layout exactly, including the filter line row.
pub fn heavy_list_inner(body: Rect, filter_active: bool, theme: &Theme) -> Rect {
    let inner = panel(
        "heaviest agent-context files (j/k)".to_string(),
        true,
        theme,
    )
    .inner(rows(body)[2]);
    if filter_active {
        super::below_filter_line(inner)
    } else {
        inner
    }
}

pub fn render(frame: &mut Frame, area: Rect, model: &Model) {
    let rows = rows(area);

    render_load_profile(frame, rows[0], model);
    render_heavy_files(frame, rows[2], model);
    render_savings(frame, rows[4], model);
}

const LABEL_W: usize = 15;
const VALUE_W: usize = 16;

fn render_load_profile(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let block = panel("load profile".to_string(), false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let max = model
        .derived
        .load
        .iter()
        .map(|(_, v)| *v)
        .max()
        .unwrap_or(0);
    let bar_width = inner.width.saturating_sub((LABEL_W + VALUE_W) as u16);
    let mut lines: Vec<Line> = Vec::with_capacity(6);
    for (i, (label, value)) in model.derived.load.iter().enumerate() {
        let target = if max > 0 {
            *value as f32 / max as f32
        } else {
            0.0
        };
        let frac = model.animator.value(
            AnimKey::Bar {
                panel: 0,
                row: i as u8,
            },
            target,
        );
        lines.push(bars::hbar_line(
            label,
            LABEL_W,
            frac,
            bar_width,
            &format!("{} tok", format::commas(*value)),
            VALUE_W,
            theme,
        ));
    }
    lines.push(source_line(model));
    frame.render_widget(Paragraph::new(lines), inner);
}

fn source_line(model: &Model) -> Line<'static> {
    let theme = &model.theme;
    match &model.scan {
        ScanState::Ready(_) => {
            let detail = match model.last_scan {
                Some(meta) => format!(
                    "live scan, {} files in {}",
                    format::commas(model.derived.scan_files),
                    format::duration_short(meta.elapsed_ms)
                ),
                None => "live scan".to_string(),
            };
            Line::from(Span::styled(
                format!("source: {detail}"),
                Style::default().fg(theme.faint),
            ))
        }
        // The same scanner sweep the header shows; reduced motion renders
        // the plain muted label.
        ScanState::Scanning => Line::from(crate::tui::components::spinner::scanner_spans(
            "scanning repo",
            model.busy_ms,
            model.animator.enabled(),
            theme,
        )),
        ScanState::Failed(msg) => Line::from(Span::styled(
            format!("scan failed: {msg}"),
            Style::default().fg(theme.err),
        )),
        ScanState::Idle => Line::from(Span::styled(
            "no scan (press s to scan this repo)".to_string(),
            Style::default().fg(theme.faint),
        )),
    }
}

fn render_heavy_files(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let block = panel(
        "heaviest agent-context files (j/k)".to_string(),
        true,
        theme,
    );
    let mut inner = block.inner(area);
    frame.render_widget(block, area);

    if let Some((query, typing)) = model.filter_line(FilterTarget::Heavy) {
        frame.render_widget(
            Paragraph::new(super::filter_line(query, typing, theme)),
            Rect { height: 1, ..inner },
        );
        inner = super::below_filter_line(inner);
    }

    if model.derived.heavy.is_empty() {
        let text = match &model.scan {
            ScanState::Scanning => "scanning...",
            ScanState::Failed(_) => "scan failed (press s to retry)",
            ScanState::Ready(_) => "no agent-context files found",
            ScanState::Idle => "no scan yet (press s to scan this repo)",
        };
        frame.render_widget(Paragraph::new(muted_line(text, theme)), inner);
        return;
    }
    let visible = model.visible_heavy();
    if visible.is_empty() {
        frame.render_widget(
            Paragraph::new(muted_line("no files match the filter", theme)),
            inner,
        );
        return;
    }

    // Right-aligned numerics: tokens column then percent-of-always.
    let value_w: usize = 21;
    let path_w = (inner.width as usize).saturating_sub(3 + value_w);
    let rows: Vec<Line> = visible
        .iter()
        .map(|h| {
            let shown = format::truncate_left(&h.path, path_w);
            let (dir, name) = format::split_path(&shown);
            let pad = path_w.saturating_sub(shown.chars().count());
            let value = format!(
                "{:>10} tok {:>4.0}% ",
                format::commas(h.tokens),
                h.pct_always.min(999.0)
            );
            let line = Line::from(vec![
                Span::styled(dir.to_string(), Style::default().fg(theme.faint)),
                Span::styled(name.to_string(), Style::default().fg(theme.text)),
                Span::raw(" ".repeat(pad)),
                Span::styled(value, Style::default().fg(theme.muted)),
            ]);
            super::reveal_row(model, reveal::HEAVY, &h.path, line, theme)
        })
        .collect();
    render_select_list(frame, inner, &rows, Some(model.sel_heavy.idx), true, theme);
}

fn render_savings(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let block = panel("savings (this project)".to_string(), false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let parts = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(inner);

    let report = &model.derived.project_report;
    let mut lines: Vec<Line> = Vec::with_capacity(2);
    match &report.identified {
        Some(id) => {
            let min = id.project_reclaimable_min.unwrap_or(0);
            let max = id.project_reclaimable_max.unwrap_or(0);
            lines.push(Line::from(vec![
                Span::styled("identified ", Style::default().fg(theme.text)),
                Span::styled(
                    format!("({})  ", id.label),
                    Style::default().fg(theme.faint),
                ),
                Span::styled(
                    format!(
                        "~{}-{} tokens reclaimable",
                        format::commas(min),
                        format::commas(max)
                    ),
                    Style::default().fg(theme.accent),
                ),
            ]));
        }
        None => lines.push(muted_line(
            "identified: nothing recorded yet (run tolkin project)",
            theme,
        )),
    }
    match &report.realized {
        Some(r) => {
            let sign = if r.tokens >= 0 { "" } else { "-" };
            lines.push(Line::from(vec![
                Span::styled("realized   ", Style::default().fg(theme.text)),
                Span::styled(format!("({})  ", r.label), Style::default().fg(theme.faint)),
                Span::styled(
                    format!(
                        "{sign}{} input tok over {} sessions, basis: {}",
                        format::commas(r.tokens.unsigned_abs()),
                        r.sessions_count,
                        r.sessions_basis
                    ),
                    Style::default().fg(theme.accent),
                ),
            ]));
        }
        None => lines.push(muted_line(
            "realized: needs two project snapshots (re-run tolkin project)",
            theme,
        )),
    }
    frame.render_widget(Paragraph::new(lines), parts[0]);

    let series = &model.derived.realized_spark;
    if series.is_empty() {
        frame.render_widget(
            Paragraph::new(muted_line(
                "no realized trend (one snapshot or fewer)",
                theme,
            )),
            parts[1],
        );
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
            .style(Style::default().fg(theme.bar_fill));
        frame.render_widget(spark, parts[1]);
    }
}
