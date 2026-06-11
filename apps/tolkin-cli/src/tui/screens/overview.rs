//! Overview: the default tab. Four hero cards, the 30-day spend spark,
//! advisories, and the machine summary, every number with its tier tag.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::anim::AnimKey;
use crate::tui::app::{Model, ScanState};
use crate::tui::components::bars::{self, DayRole};
use crate::tui::components::card::{render_stat_card, value_line, StatCard};
use crate::tui::components::gauge;
use crate::tui::components::list::render_select_list;
use crate::tui::format;

use super::{kv_line, muted_line, panel, panel_with_accessory};

const CONSENT_HINT: &str = "consent to ingestion: tolkin init";

pub fn render(frame: &mut Frame, area: Rect, model: &Model) {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(6),
    ])
    .split(area);

    render_cards(frame, rows[0], model);
    render_spark(frame, rows[2], model);

    let bottom = Layout::horizontal([
        Constraint::Percentage(55),
        Constraint::Length(1),
        Constraint::Min(28),
    ])
    .split(rows[4]);
    render_advisories(frame, bottom[0], model);
    render_machine_summary(frame, bottom[2], model);
}

fn render_cards(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let derived = &model.derived;
    let cols = Layout::horizontal([
        Constraint::Percentage(22),
        Constraint::Percentage(22),
        Constraint::Percentage(28),
        Constraint::Percentage(28),
    ])
    .spacing(1)
    .split(area);

    let today = if derived.ingestion_on {
        StatCard {
            title: "today",
            value: value_line(
                format::usd(
                    model
                        .animator
                        .value(AnimKey::Card(0), derived.today_cost as f32)
                        as f64,
                ),
                theme,
            ),
            tier: "measured",
            hint: None,
        }
    } else {
        no_data_card("today")
    };
    render_stat_card(frame, cols[0], &today, theme);

    let last30 = if derived.ingestion_on {
        StatCard {
            title: "30 days",
            value: value_line(
                format::usd(
                    model
                        .animator
                        .value(AnimKey::Card(1), derived.last30_cost as f32)
                        as f64,
                ),
                theme,
            ),
            tier: "measured",
            hint: None,
        }
    } else {
        no_data_card("30 days")
    };
    render_stat_card(frame, cols[1], &last30, theme);

    let cache = match derived.cache_pct() {
        Some(pct) => {
            let sampled = model.animator.value(AnimKey::Card(2), pct as f32);
            let frac = model
                .animator
                .value(AnimKey::Gauge(0), (pct / 100.0) as f32);
            let (fill, rest) = gauge::gauge_spans(frac, 7, theme);
            StatCard {
                title: "cache hit",
                value: Line::from(vec![
                    Span::styled(
                        format!("{sampled:.1}%  "),
                        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
                    ),
                    fill,
                    rest,
                ]),
                tier: "measured",
                hint: None,
            }
        }
        None => no_data_card("cache hit"),
    };
    render_stat_card(frame, cols[2], &cache, theme);

    let reclaimable = match derived.reclaimable() {
        Some((min, max)) => {
            let smin = model.animator.value(AnimKey::Card(3), min as f32).max(0.0) as u64;
            let smax = model.animator.value(AnimKey::Card(4), max as f32).max(0.0) as u64;
            StatCard {
                title: "reclaimable",
                value: value_line(
                    format!(
                        "{} - {} tok",
                        format::humanize_tokens(smin),
                        format::humanize_tokens(smax)
                    ),
                    theme,
                ),
                tier: "advisory",
                hint: None,
            }
        }
        None => StatCard {
            title: "reclaimable",
            value: Line::default(),
            tier: "",
            hint: Some("run: tolkin project"),
        },
    };
    render_stat_card(frame, cols[3], &reclaimable, theme);
}

fn no_data_card(title: &str) -> StatCard<'_> {
    StatCard {
        title,
        value: Line::default(),
        tier: "",
        hint: Some(CONSENT_HINT),
    }
}

fn render_spark(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let derived = &model.derived;
    let max_cost = derived
        .day_details
        .iter()
        .map(|d| d.cost_usd)
        .fold(0.0f64, f64::max);
    let block = panel_with_accessory(
        "spend, last 30 days".to_string(),
        format!("max {}/day", format::usd(max_cost)),
        false,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if !derived.ingestion_on {
        frame.render_widget(Paragraph::new(muted_line(CONSENT_HINT, theme)), inner);
        return;
    }
    if derived.day_details.is_empty() {
        frame.render_widget(Paragraph::new(muted_line("no usage data", theme)), inner);
        return;
    }
    let len = derived.day_details.len();
    let cell = if inner.width >= len as u16 * 2 { 2 } else { 1 };
    let mut levels: Vec<f32> = Vec::with_capacity(len);
    let mut roles: Vec<DayRole> = Vec::with_capacity(len);
    for (i, day) in derived.day_details.iter().enumerate() {
        let target = if max_cost > 0.0 {
            (day.cost_usd / max_cost) as f32
        } else {
            0.0
        };
        levels.push(model.animator.value(
            AnimKey::Bar {
                panel: 3,
                row: i as u8,
            },
            target,
        ));
        roles.push(if i == len - 1 {
            DayRole::Today
        } else {
            DayRole::Plain
        });
    }
    let spans = bars::strip_spans(&levels, &roles, 1.0, cell, theme);
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

fn render_advisories(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let block = panel("advisories (j/k)".to_string(), true, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if !model.derived.ingestion_on {
        frame.render_widget(Paragraph::new(muted_line(CONSENT_HINT, theme)), inner);
        return;
    }
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
        Some(model.sel_overview.idx),
        true,
        theme,
    );
}

fn render_machine_summary(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let block = panel("this machine".to_string(), false, theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = vec![
        kv_line(
            "projects tracked",
            model.derived.machine.len().to_string(),
            inner.width,
            theme,
        ),
        kv_line(
            "sessions ingested",
            if model.derived.ingestion_on {
                model.derived.sessions_total.to_string()
            } else {
                "off".to_string()
            },
            inner.width,
            theme,
        ),
    ];
    let scan_value = match (&model.scan, model.last_scan) {
        (ScanState::Scanning, _) => "running...".to_string(),
        (_, Some(meta)) => format!(
            "{} ({})",
            format::relative_time_ago(model.now_epoch, meta.at_epoch),
            format::duration_short(meta.elapsed_ms)
        ),
        (ScanState::Failed(_), None) => "failed".to_string(),
        _ => "none yet".to_string(),
    };
    lines.push(kv_line("last scan", scan_value, inner.width, theme));
    frame.render_widget(Paragraph::new(lines), inner);
}
