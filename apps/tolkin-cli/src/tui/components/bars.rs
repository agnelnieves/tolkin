//! Horizontal bars and the day strip.
//!
//! `hbar_line` renders one labeled bar row (load profile, machine project
//! weights) with an animation fraction already sampled by the caller, so
//! the component itself never touches the clock. `strip_spans` renders the
//! 30-day chart as one span per day with per-day styling (today, selection),
//! which the stock Sparkline widget cannot do.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::theme::Theme;

/// Eight-level bar glyphs for the day strip, lowest to highest.
const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// One labeled horizontal bar row.
///
/// `frac` is value/max in 0.0..=1.0, already multiplied by the grow-in
/// animation sample. `value_text` renders right-aligned in `value_width`
/// cells after the bar.
pub fn hbar_line(
    label: &str,
    label_width: usize,
    frac: f32,
    bar_width: u16,
    value_text: &str,
    value_width: usize,
    theme: &Theme,
) -> Line<'static> {
    let frac = frac.clamp(0.0, 1.0);
    // Never round a non-zero value down to an invisible bar.
    let mut filled = (frac * bar_width as f32).round() as u16;
    if frac > 0.0 {
        filled = filled.max(1);
    }
    let filled = filled.min(bar_width);
    let empty = bar_width - filled;
    Line::from(vec![
        Span::styled(
            format!("{label:<label_width$}"),
            Style::default().fg(theme.muted),
        ),
        Span::styled(
            "█".repeat(filled as usize),
            Style::default().fg(theme.bar_fill),
        ),
        Span::styled(
            "░".repeat(empty as usize),
            Style::default().fg(theme.bar_empty),
        ),
        Span::styled(
            format!("{value_text:>value_width$}"),
            Style::default().fg(theme.text),
        ),
    ])
}

/// Render a value series as one string of eight-level glyphs. When the
/// series is longer than `width` the newest values win (the tail renders).
/// All-zero series render flat; an empty series renders empty.
pub fn spark_string(values: &[u64], width: usize) -> String {
    if values.is_empty() || width == 0 {
        return String::new();
    }
    let tail = if values.len() > width {
        &values[values.len() - width..]
    } else {
        values
    };
    let max = tail.iter().copied().max().unwrap_or(0);
    tail.iter()
        .map(|v| {
            if max == 0 {
                LEVELS[0]
            } else {
                let idx = ((*v as f64 / max as f64) * (LEVELS.len() - 1) as f64).round() as usize;
                LEVELS[idx.min(LEVELS.len() - 1)]
            }
        })
        .collect()
}

/// Styling role of one day cell in the strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayRole {
    Plain,
    Today,
    Selected,
}

/// Build the day strip spans. `levels` are 0.0..=1.0 per day (oldest
/// first); `grow` is the global grow-in sample. Each day takes `cell`
/// columns (glyph plus padding spaces).
pub fn strip_spans(
    levels: &[f32],
    roles: &[DayRole],
    grow: f32,
    cell: u16,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let pad = " ".repeat(cell.saturating_sub(1) as usize);
    levels
        .iter()
        .zip(roles.iter())
        .map(|(level, role)| {
            let scaled = (level * grow).clamp(0.0, 1.0);
            let glyph = if *level <= 0.0 {
                '▁'
            } else {
                let idx =
                    ((scaled * (LEVELS.len() - 1) as f32).round() as usize).min(LEVELS.len() - 1);
                // A non-zero day never renders flat once grown past zero.
                LEVELS[if scaled > 0.0 { idx.max(1) } else { idx }]
            };
            let mut style = match role {
                DayRole::Plain if *level <= 0.0 => Style::default().fg(theme.bar_empty),
                DayRole::Plain => Style::default().fg(theme.bar_fill),
                DayRole::Today => Style::default()
                    .fg(theme.accent_bright)
                    .add_modifier(Modifier::BOLD),
                DayRole::Selected => Style::default()
                    .fg(theme.selection_fg)
                    .bg(theme.selection_bg),
            };
            if *role == DayRole::Selected && *level <= 0.0 {
                style = Style::default().fg(theme.faint).bg(theme.selection_bg);
            }
            Span::styled(format!("{glyph}{pad}"), style)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;

    fn t() -> Theme {
        theme::by_name("tolkin-dark", true).unwrap()
    }

    fn line_text(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn hbar_fills_and_keeps_total_width() {
        let theme = t();
        let line = hbar_line("always", 14, 0.5, 20, "12,000 tok", 12, &theme);
        let text = line_text(&line);
        assert_eq!(text.chars().count(), 14 + 20 + 12);
        assert_eq!(text.chars().filter(|c| *c == '█').count(), 10);
        assert!(text.ends_with("  12,000 tok"));
    }

    #[test]
    fn hbar_nonzero_value_never_invisible() {
        let theme = t();
        let line = hbar_line("docs", 14, 0.001, 20, "3 tok", 8, &theme);
        let text = line_text(&line);
        assert_eq!(text.chars().filter(|c| *c == '█').count(), 1);
    }

    #[test]
    fn spark_string_scales_and_keeps_the_newest_tail() {
        assert_eq!(spark_string(&[], 10), "");
        assert_eq!(spark_string(&[5, 5], 0), "");
        assert_eq!(spark_string(&[0, 0, 0], 8), "▁▁▁");
        let s = spark_string(&[0, 4, 8], 8);
        assert_eq!(s.chars().count(), 3);
        assert!(s.starts_with('▁') && s.ends_with('█'), "scaled: {s}");
        // Longer than width: only the newest values render.
        let s = spark_string(&[9, 9, 9, 1, 2], 2);
        assert_eq!(s.chars().count(), 2);
        assert!(s.ends_with('█'), "tail max normalizes: {s}");
    }

    #[test]
    fn strip_marks_zero_days_flat_and_scales_levels() {
        let theme = t();
        let levels = [0.0, 0.5, 1.0];
        let roles = [DayRole::Plain, DayRole::Today, DayRole::Selected];
        let spans = strip_spans(&levels, &roles, 1.0, 2, &theme);
        assert_eq!(spans.len(), 3);
        assert!(spans[0].content.starts_with('▁'));
        assert!(spans[2].content.starts_with('█'));
        // Each cell is glyph + 1 pad space.
        assert_eq!(spans[1].content.chars().count(), 2);
    }
}
