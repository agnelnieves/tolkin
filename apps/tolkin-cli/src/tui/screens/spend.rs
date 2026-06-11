//! Spend tab: the 30-day strip with a selectable day cursor, the models
//! table, cache health, and the advisories list. One friendly empty state
//! covers the whole tab when ingestion is off.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::cache_analysis::CacheReport;
use crate::tui::anim::AnimKey;
use crate::tui::app::{Model, SpendFocus};
use crate::tui::components::bars::{self, DayRole};
use crate::tui::components::empty;
use crate::tui::components::gauge;
use crate::tui::components::list::render_select_list;
use crate::tui::data::SPEND_DAYS_WINDOW;
use crate::tui::format;
use crate::tui::theme::Theme;

use super::{muted_line, panel, panel_with_accessory};

/// Vertical body split: day strip, models, cache, advisories (with gaps).
/// Shared by render and the mouse hit-testing helper below.
fn rows(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Length(9),
        Constraint::Length(1),
        Constraint::Length(5),
        Constraint::Length(1),
        Constraint::Min(0),
    ])
    .split(area)
}

/// Inner rect of the advisories select-list for mouse hit-testing; only
/// meaningful while ingestion is on (the caller guards). Mirrors
/// render()'s layout exactly.
pub fn advisory_list_inner(body: Rect, theme: &Theme) -> Rect {
    panel("advisories (j/k, [ ] focus)".to_string(), true, theme).inner(rows(body)[6])
}

pub fn render(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    if !model.derived.ingestion_on {
        let block = panel("spend".to_string(), false, theme);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(empty::consent_lines(theme)), inner);
        return;
    }

    let rows = rows(area);

    render_day_strip(frame, rows[0], model);
    render_models(frame, rows[2], model);
    render_cache(frame, rows[4], model);
    render_advisories(frame, rows[6], model);
}

fn render_day_strip(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let derived = &model.derived;
    let focused = model.spend_focus == SpendFocus::Days;
    let max = derived
        .day_details
        .iter()
        .map(|d| d.input_side)
        .max()
        .unwrap_or(0);
    let block = panel_with_accessory(
        format!("daily input, last {SPEND_DAYS_WINDOW} UTC days (h/l)"),
        format!("max {} tok/day", format::humanize_tokens(max)),
        focused,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if derived.day_details.is_empty() {
        frame.render_widget(Paragraph::new(muted_line("no usage data", theme)), inner);
        return;
    }
    let len = derived.day_details.len();
    let cursor = model.day_cursor.min(len - 1);
    let cell = if inner.width >= len as u16 * 2 { 2 } else { 1 };
    let mut levels: Vec<f32> = Vec::with_capacity(len);
    let mut roles: Vec<DayRole> = Vec::with_capacity(len);
    for (i, day) in derived.day_details.iter().enumerate() {
        let target = if max > 0 {
            day.input_side as f32 / max as f32
        } else {
            0.0
        };
        levels.push(model.animator.value(
            AnimKey::Bar {
                panel: 2,
                row: i as u8,
            },
            target,
        ));
        roles.push(if i == cursor {
            DayRole::Selected
        } else if i == len - 1 {
            DayRole::Today
        } else {
            DayRole::Plain
        });
    }
    let strip = Line::from(bars::strip_spans(&levels, &roles, 1.0, cell, theme));

    let day = &derived.day_details[cursor];
    let caption = Line::from(vec![
        Span::styled(
            format!("{}  ", day.day),
            Style::default()
                .fg(theme.accent_bright)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "in {} (fresh {}, cache {})   out {}   ",
                format::humanize_tokens(day.input_side),
                format::humanize_tokens(day.input_fresh),
                format::humanize_tokens(day.cache_read),
                format::humanize_tokens(day.output),
            ),
            Style::default().fg(theme.muted),
        ),
        Span::styled(
            format!("~{}", format::usd(day.cost_usd)),
            Style::default().fg(theme.text),
        ),
    ]);
    let lines = vec![strip, caption];
    frame.render_widget(Paragraph::new(lines), inner);
}

const COL_IN: usize = 16;
const COL_OUT: usize = 13;
const COL_COST: usize = 11;

/// Model rows the table shows before the "+N more" footer.
const MODELS_SHOWN: usize = 5;

fn render_models(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let derived = &model.derived;
    // The sort indicator lives in the title; `,` cycles it.
    let block = panel_with_accessory(
        format!("models, top {MODELS_SHOWN} by {}", model.model_sort.label()),
        "sort (,)".to_string(),
        false,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if derived.spend_models.is_empty() {
        frame.render_widget(
            Paragraph::new(muted_line("no measured sessions in scope", theme)),
            inner,
        );
        return;
    }
    let name_w = (inner.width as usize).saturating_sub(COL_IN + COL_OUT + COL_COST);
    let mut lines: Vec<Line> = Vec::with_capacity(7);
    lines.push(Line::from(vec![
        Span::styled(
            format!("{:<name_w$}", "model"),
            Style::default().fg(theme.faint),
        ),
        Span::styled(
            format!("{:>COL_IN$}", "in (incl cache)"),
            Style::default().fg(theme.faint),
        ),
        Span::styled(
            format!("{:>COL_OUT$}", "out"),
            Style::default().fg(theme.faint),
        ),
        Span::styled(
            format!("{:>COL_COST$}", "cost"),
            Style::default().fg(theme.faint),
        ),
    ]));
    for row in derived.spend_models.iter().take(MODELS_SHOWN) {
        let name = format::truncate_left(&row.model, name_w.saturating_sub(1));
        let cost_span = match row.cost_usd {
            Some(c) => Span::styled(
                format!("{:>COL_COST$}", format::usd(c)),
                Style::default().fg(theme.text),
            ),
            None => Span::styled(
                format!("{:>COL_COST$}", "unpriced"),
                Style::default().fg(theme.faint),
            ),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{name:<name_w$}"), Style::default().fg(theme.text)),
            Span::styled(
                format!("{:>COL_IN$}", format::commas(row.totals.input_side())),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                format!("{:>COL_OUT$}", format::commas(row.totals.output_tokens)),
                Style::default().fg(theme.muted),
            ),
            cost_span,
        ]));
    }
    let shown = derived.spend_models.len().min(MODELS_SHOWN);
    if derived.models_total > shown {
        lines.push(muted_line(
            &format!("+{} more", derived.models_total - shown),
            theme,
        ));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_cache(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let block = panel("cache".to_string(), false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(pct) = model.derived.cache_pct() else {
        frame.render_widget(
            Paragraph::new(muted_line("no measured sessions yet", theme)),
            inner,
        );
        return;
    };
    let mut lines: Vec<Line> = Vec::with_capacity(4);
    let frac = model
        .animator
        .value(AnimKey::Gauge(0), (pct / 100.0) as f32);
    let (fill, rest) = gauge::gauge_spans(frac, inner.width.saturating_sub(32).min(24), theme);
    lines.push(Line::from(vec![
        Span::styled("hit rate  ", Style::default().fg(theme.muted)),
        fill,
        rest,
        Span::styled(
            format!("  {pct:>5.1}% of input"),
            Style::default().fg(theme.text),
        ),
    ]));
    if let Some(report) = model.derived.cache.as_ref() {
        lines.push(cache_health_line(report, theme));
        lines.push(Line::from(Span::styled(
            report.ttl_counterfactual.verdict.clone(),
            Style::default().fg(theme.faint),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

/// The one-line cache health row. The threshold wording is unchanged from
/// the previous dashboard (`tolkin cache` prints the full sentence).
fn cache_health_line(report: &CacheReport, theme: &Theme) -> Line<'static> {
    let (text, color) = if report.hit_rate.advisory.is_some() {
        (
            format!(
                "hit rate under {:.2} on {} active day(s): caching likely broken; levers: prefix stability, session shape",
                report.hit_rate.threshold,
                report.hit_rate.days_below_threshold.len()
            ),
            theme.warn,
        )
    } else {
        (
            format!(
                "ok, no active day under the {:.2} threshold; levers: prefix stability, session shape",
                report.hit_rate.threshold
            ),
            theme.muted,
        )
    };
    Line::from(vec![
        Span::styled("health    ", Style::default().fg(theme.muted)),
        Span::styled(text, Style::default().fg(color)),
    ])
}

fn render_advisories(frame: &mut Frame, area: Rect, model: &Model) {
    if area.height == 0 {
        return;
    }
    let theme = &model.theme;
    let focused = model.spend_focus == SpendFocus::Advisories;
    let block = panel("advisories (j/k, [ ] focus)".to_string(), focused, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if model.derived.advisory_lines.is_empty() {
        frame.render_widget(
            Paragraph::new(muted_line("no advisories for this window", theme)),
            inner,
        );
        return;
    }
    let rows: Vec<Line> = model
        .derived
        .advisory_lines
        .iter()
        .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(theme.text))))
        .collect();
    render_select_list(
        frame,
        inner,
        &rows,
        Some(model.sel_spend.idx),
        focused,
        theme,
    );
}
