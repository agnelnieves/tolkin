//! Machine tab: totals cards plus the ranked, selectable project list with
//! animated weight bars and relative "as of" timestamps.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::anim::AnimKey;
use crate::tui::app::Model;
use crate::tui::components::card::{render_stat_card, value_line, StatCard};
use crate::tui::components::list::render_select_list;
use crate::tui::format;

use super::{muted_line, panel};

pub fn render(frame: &mut Frame, area: Rect, model: &Model) {
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(5),
    ])
    .split(area);
    render_cards(frame, rows[0], model);
    render_projects(frame, rows[2], model);
}

fn render_cards(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let report = &model.derived.global_report;
    let cols = Layout::horizontal([
        Constraint::Percentage(30),
        Constraint::Percentage(30),
        Constraint::Percentage(20),
        Constraint::Percentage(20),
    ])
    .spacing(1)
    .split(area);

    let identified = match report.identified.as_ref() {
        Some(id) => StatCard {
            title: "identified",
            value: value_line(
                format!(
                    "~{} - {} tok",
                    format::humanize_tokens(id.project_reclaimable_min.unwrap_or(0)),
                    format::humanize_tokens(id.project_reclaimable_max.unwrap_or(0))
                ),
                theme,
            ),
            tier: "advisory",
            hint: None,
        },
        None => StatCard {
            title: "identified",
            value: Line::default(),
            tier: "",
            hint: Some("run: tolkin project"),
        },
    };
    render_stat_card(frame, cols[0], &identified, theme);

    let realized = match report.realized.as_ref() {
        Some(r) => {
            let sign = if r.tokens >= 0 { "" } else { "-" };
            StatCard {
                title: "realized",
                value: value_line(
                    format!(
                        "{sign}{} tok",
                        format::humanize_tokens(r.tokens.unsigned_abs())
                    ),
                    theme,
                ),
                tier: "measured structure",
                hint: None,
            }
        }
        None => StatCard {
            title: "realized",
            value: Line::default(),
            tier: "",
            hint: Some("needs two snapshots"),
        },
    };
    render_stat_card(frame, cols[1], &realized, theme);

    let projects = StatCard {
        title: "projects",
        value: value_line(model.derived.machine.len().to_string(), theme),
        tier: "ledger",
        hint: None,
    };
    render_stat_card(frame, cols[2], &projects, theme);

    let sessions = match report.measured.as_ref() {
        Some(m) => StatCard {
            title: "sessions",
            value: value_line(format::commas(m.sessions), theme),
            tier: "measured",
            hint: None,
        },
        None => StatCard {
            title: "sessions",
            value: Line::default(),
            tier: "",
            hint: Some("ingestion off"),
        },
    };
    render_stat_card(frame, cols[3], &sessions, theme);
}

const NAME_W: usize = 26;
const VALUE_W: usize = 30;

fn render_projects(frame: &mut Frame, area: Rect, model: &Model) {
    let theme = &model.theme;
    let block = panel(
        "projects by always-loaded weight (j/k)".to_string(),
        true,
        theme,
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if model.derived.machine.is_empty() {
        let lines = vec![
            muted_line("no projects recorded yet", theme),
            muted_line("run tolkin project inside a repo to add one", theme),
        ];
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let max = model
        .derived
        .machine
        .iter()
        .map(|p| p.always_tokens)
        .max()
        .unwrap_or(0);
    let bar_width = inner
        .width
        .saturating_sub((3 + NAME_W + 1 + VALUE_W) as u16);
    let rows: Vec<Line> = model
        .derived
        .machine
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let target = if max > 0 {
                p.always_tokens as f32 / max as f32
            } else {
                0.0
            };
            // Rows beyond the animated key space render at rest.
            let frac = if i < 64 {
                model.animator.value(
                    AnimKey::Bar {
                        panel: 1,
                        row: i as u8,
                    },
                    target,
                )
            } else {
                target
            };
            let shown = format::truncate_left(&p.key, NAME_W);
            let (dir, name) = format::split_path(&shown);
            let pad = NAME_W.saturating_sub(shown.chars().count()) + 1;
            let sessions = if model.derived.ingestion_on {
                format!("{:>5} sess", p.sessions)
            } else {
                " ".repeat(10)
            };
            let value = format!(
                "{:>11} tok {sessions} {:>3} ",
                format::commas(p.always_tokens),
                format::relative_time(model.now_epoch, p.last_ts)
            );
            let frac = frac.clamp(0.0, 1.0);
            let filled = ((frac * bar_width as f32).round() as u16).min(bar_width);
            Line::from(vec![
                Span::styled(dir.to_string(), Style::default().fg(theme.faint)),
                Span::styled(name.to_string(), Style::default().fg(theme.text)),
                Span::raw(" ".repeat(pad)),
                Span::styled(
                    "█".repeat(filled as usize),
                    Style::default().fg(theme.bar_fill),
                ),
                Span::styled(
                    "░".repeat((bar_width - filled) as usize),
                    Style::default().fg(theme.bar_empty),
                ),
                Span::styled(
                    format!("{value:>VALUE_W$}"),
                    Style::default().fg(theme.muted),
                ),
            ])
        })
        .collect();
    render_select_list(
        frame,
        inner,
        &rows,
        Some(model.sel_machine.idx),
        true,
        theme,
    );
}
