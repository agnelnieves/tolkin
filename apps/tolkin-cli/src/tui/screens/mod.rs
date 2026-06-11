//! Screen routing plus the shared panel primitives every tab uses.

pub mod machine;
pub mod overview;
pub mod project;
pub mod spend;

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding};
use ratatui::Frame;

use super::app::Model;
use super::keymap::TabId;
use super::theme::Theme;

/// Route the body to the active tab's screen.
pub fn render(frame: &mut Frame, area: Rect, model: &Model) {
    match model.tab {
        TabId::Overview => overview::render(frame, area, model),
        TabId::Project => project::render(frame, area, model),
        TabId::Machine => machine::render(frame, area, model),
        TabId::Spend => spend::render(frame, area, model),
    }
}

/// A surface panel: layered background, one row of breathing room under
/// the title, one column of padding. No borders; surfaces separate by
/// background per the design contract.
pub fn panel(title: String, focused: bool, theme: &Theme) -> Block<'static> {
    let title_style = if focused {
        Style::default()
            .fg(theme.accent)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.muted)
    };
    Block::new()
        .style(Style::default().bg(theme.surface))
        .padding(Padding::new(1, 1, 1, 0))
        .title(Line::from(Span::styled(format!(" {title} "), title_style)))
}

/// Same panel with a right-aligned accessory in the title row (e.g. the
/// strip's max label).
pub fn panel_with_accessory(
    title: String,
    accessory: String,
    focused: bool,
    theme: &Theme,
) -> Block<'static> {
    panel(title, focused, theme).title_top(
        Line::from(Span::styled(
            format!(" {accessory} "),
            Style::default().fg(theme.faint),
        ))
        .right_aligned(),
    )
}

/// Label left, value right-aligned at the panel's inner width.
pub fn kv_line(label: &str, value: String, width: u16, theme: &Theme) -> Line<'static> {
    let label_len = label.chars().count();
    let value_width = (width as usize).saturating_sub(label_len);
    Line::from(vec![
        Span::styled(label.to_string(), Style::default().fg(theme.muted)),
        Span::styled(
            format!("{value:>value_width$}"),
            Style::default().fg(theme.text),
        ),
    ])
}

/// One muted body line (empty states inside panels).
pub fn muted_line(text: &str, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(theme.faint),
    ))
}

/// The list area under an active filter line (the line takes the first
/// inner row). Shared by render and mouse hit-testing.
pub fn below_filter_line(inner: Rect) -> Rect {
    Rect {
        x: inner.x,
        y: inner.y + 1,
        width: inner.width,
        height: inner.height.saturating_sub(1),
    }
}

/// The inline `/` filter line rendered above a filtered list: accent while
/// typing, muted once confirmed.
pub fn filter_line(query: &str, typing: bool, theme: &Theme) -> Line<'static> {
    let color = if typing { theme.accent } else { theme.muted };
    let cursor = if typing { "▌" } else { "" };
    Line::from(vec![
        Span::styled("/ ".to_string(), Style::default().fg(theme.accent)),
        Span::styled(format!("{query}{cursor}"), Style::default().fg(color)),
        Span::styled(
            if typing {
                "  (enter keep, esc clear)"
            } else {
                "  (esc clear)"
            },
            Style::default().fg(theme.faint),
        ),
    ])
}
