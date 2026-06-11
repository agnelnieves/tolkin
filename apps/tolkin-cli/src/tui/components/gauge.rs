//! Slim percent gauge: `▰▰▰▰▱▱▱` plus an optional trailing label. Used by
//! the cache-hit card and the Spend cache panel.

use ratatui::style::Style;
use ratatui::text::Span;

use crate::tui::theme::Theme;

/// Build the gauge cells for a 0.0..=1.0 fraction at `width` cells.
/// Returns (filled span, empty span) so callers compose their own line.
pub fn gauge_spans(frac: f32, width: u16, theme: &Theme) -> (Span<'static>, Span<'static>) {
    let frac = frac.clamp(0.0, 1.0);
    let filled = ((frac * width as f32).round() as u16).min(width);
    let empty = width - filled;
    (
        Span::styled(
            "▰".repeat(filled as usize),
            Style::default().fg(theme.bar_fill),
        ),
        Span::styled(
            "▱".repeat(empty as usize),
            Style::default().fg(theme.bar_empty),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;

    fn test_theme() -> Theme {
        theme::by_name("tolkin-dark", true).unwrap()
    }

    #[test]
    fn gauge_fills_proportionally_and_clamps() {
        let t = test_theme();
        let (filled, empty) = gauge_spans(0.5, 10, &t);
        assert_eq!(filled.content.chars().count(), 5);
        assert_eq!(empty.content.chars().count(), 5);
        let (filled, empty) = gauge_spans(1.5, 10, &t);
        assert_eq!(filled.content.chars().count(), 10);
        assert_eq!(empty.content.chars().count(), 0);
        let (filled, empty) = gauge_spans(-0.2, 10, &t);
        assert_eq!(filled.content.chars().count(), 0);
        assert_eq!(empty.content.chars().count(), 10);
    }
}
