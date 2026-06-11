//! StatCard: a hero number on a layered surface. Title muted on top, big
//! value, tier tag underneath (the honesty design at the component level).
//! Cards with no data show a muted hint instead of a value.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

pub struct StatCard<'a> {
    pub title: &'a str,
    /// Styled value line. Empty when `hint` carries the body instead.
    pub value: Line<'a>,
    /// Tier tag rendered faint under the value ("measured", "advisory").
    pub tier: &'a str,
    /// No-data hint (e.g. "consent to ingestion: tolkin init").
    pub hint: Option<&'a str>,
}

/// Convenience: a plain bright bold value line.
pub fn value_line(text: String, theme: &Theme) -> Line<'static> {
    Line::from(Span::styled(
        text,
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    ))
}

pub fn render_stat_card(frame: &mut Frame, area: Rect, card: &StatCard, theme: &Theme) {
    let block = Block::new()
        .style(Style::default().bg(theme.element))
        .padding(Padding::horizontal(1));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::with_capacity(3);
    lines.push(Line::from(Span::styled(
        card.title.to_string(),
        Style::default().fg(theme.muted),
    )));
    match card.hint {
        Some(hint) => {
            lines.push(Line::from(Span::styled(
                hint.to_string(),
                Style::default().fg(theme.faint),
            )));
        }
        None => {
            lines.push(card.value.clone());
            if !card.tier.is_empty() {
                lines.push(Line::from(Span::styled(
                    card.tier.to_string(),
                    Style::default().fg(theme.faint),
                )));
            }
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn card_renders_title_value_and_tier() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let backend = TestBackend::new(24, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let card = StatCard {
                    title: "today",
                    value: value_line("$4.12".to_string(), &t),
                    tier: "measured",
                    hint: None,
                };
                render_stat_card(f, f.area(), &card, &t);
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = (0..4)
            .map(|y| {
                (0..24)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("today"));
        assert!(text.contains("$4.12"));
        assert!(text.contains("measured"));
    }

    #[test]
    fn card_hint_replaces_value() {
        let t = theme::by_name("mono", false).unwrap();
        let backend = TestBackend::new(40, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let card = StatCard {
                    title: "cache hit",
                    value: Line::default(),
                    tier: "measured",
                    hint: Some("consent to ingestion: tolkin init"),
                };
                render_stat_card(f, f.area(), &card, &t);
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let text: String = (0..4)
            .map(|y| {
                (0..40)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("tolkin init"));
        assert!(!text.contains("measured"), "tier hidden when no data");
    }
}
