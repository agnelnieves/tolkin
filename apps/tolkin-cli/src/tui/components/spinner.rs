//! Braille spinner frames and the scanner label sweep for busy states.
//! The frame index derives from the model's accumulated busy milliseconds
//! and is advanced on ticks; this module only maps an index to glyphs or
//! spans, so rendering stays pure and deterministic.

use ratatui::style::Style;
use ratatui::text::Span;

use crate::tui::theme::Theme;

pub const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Static fallback when animations are disabled (reduced motion, compact
/// frame): a quiet ellipsis instead of a frozen spinner frame.
pub const STATIC_FALLBACK: &str = "⋯";

/// Spinner advances one frame per 80 ms of accumulated busy time.
pub const FRAME_MS: u64 = 80;

/// The scanner sweep advances one step per 30 fps frame.
pub const SCAN_FRAME_MS: u64 = 33;
/// Cells in the muted trail behind the sweep head.
const TRAIL: usize = 4;
/// Frames the head stays pinned at each end before reversing.
const HOLD: u64 = 3;

/// Map accumulated busy milliseconds to a frame. Disabled animation always
/// yields the static fallback.
pub fn frame(busy_ms: u64, animations_enabled: bool) -> &'static str {
    if !animations_enabled {
        return STATIC_FALLBACK;
    }
    FRAMES[((busy_ms / FRAME_MS) % FRAMES.len() as u64) as usize]
}

/// Head position and travel direction for sweep frame `frame` over `len`
/// characters: bidirectional, [`HOLD`] frames pinned at each end (the
/// arrival frame plus two), one cell per frame between. Direction is the
/// arrival direction during holds, so the trail never flips while parked.
fn head_state(frame: u64, len: usize) -> (usize, i8) {
    if len <= 1 {
        return (0, 1);
    }
    let travel = (len - 1) as u64;
    // Cycle: hold left (3), sweep right (travel), hold right (3, counting
    // the arrival), sweep left (travel - 1 interior cells).
    let period = 2 * travel + 2 * HOLD - 2;
    let phase = frame % period;
    if phase < HOLD {
        return (0, -1); // parked at the left end after a leftward arrival
    }
    let phase = phase - HOLD;
    if phase < travel {
        return ((phase + 1) as usize, 1); // sweeping right, cells 1..=travel
    }
    let phase = phase - travel;
    if phase < HOLD - 1 {
        return (travel as usize, 1); // parked at the right end
    }
    let phase = phase - (HOLD - 1);
    ((travel - 1 - phase) as usize, -1) // sweeping left, travel-1 down to 1
}

/// The animated scanner label: a per-char color sweep with an accent head
/// and a 4-char muted trail behind it, over a faint base. Reduced motion
/// renders the plain muted label instead.
pub fn scanner_spans(
    label: &str,
    busy_ms: u64,
    enabled: bool,
    theme: &Theme,
) -> Vec<Span<'static>> {
    if !enabled {
        return vec![Span::styled(
            label.to_string(),
            Style::default().fg(theme.muted),
        )];
    }
    let chars: Vec<char> = label.chars().collect();
    let (head, dir) = head_state(busy_ms / SCAN_FRAME_MS, chars.len());
    chars
        .iter()
        .enumerate()
        .map(|(i, c)| {
            // Distance behind the head, measured against travel direction.
            let behind = if dir >= 0 {
                head as i64 - i as i64
            } else {
                i as i64 - head as i64
            };
            let color = if i == head {
                theme.accent
            } else if (1..=TRAIL as i64).contains(&behind) {
                theme.muted
            } else {
                theme.faint
            };
            Span::styled(c.to_string(), Style::default().fg(color))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;

    #[test]
    fn frames_advance_every_80ms_and_wrap() {
        assert_eq!(frame(0, true), FRAMES[0]);
        assert_eq!(frame(79, true), FRAMES[0]);
        assert_eq!(frame(80, true), FRAMES[1]);
        assert_eq!(frame(800, true), FRAMES[0]);
    }

    #[test]
    fn disabled_yields_static_fallback() {
        assert_eq!(frame(0, false), STATIC_FALLBACK);
        assert_eq!(frame(400, false), STATIC_FALLBACK);
    }

    #[test]
    fn head_holds_three_frames_at_each_end_and_bounces() {
        let len = 5; // travel 4, period 2*4 + 2*3 - 2 = 12
        let positions: Vec<usize> = (0..12).map(|f| head_state(f, len).0).collect();
        assert_eq!(positions, vec![0, 0, 0, 1, 2, 3, 4, 4, 4, 3, 2, 1]);
        // The cycle wraps cleanly back to the left hold.
        assert_eq!(head_state(12, len).0, 0);
        // Directions: rightward while sweeping right, leftward back.
        assert_eq!(head_state(4, len).1, 1);
        assert_eq!(head_state(10, len).1, -1);
        // Holds keep the arrival direction.
        assert_eq!(head_state(7, len).1, 1, "right hold keeps rightward");
        assert_eq!(head_state(1, len).1, -1, "left hold keeps leftward");
        // Degenerate labels never panic.
        assert_eq!(head_state(9, 1), (0, 1));
        assert_eq!(head_state(9, 0), (0, 1));
    }

    #[test]
    fn scanner_paints_accent_head_with_muted_trail_over_faint_base() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        // Frame 10 over "scanning repo" (len 13, travel 12): phase 10 in
        // the rightward sweep puts the head at cell 8.
        let spans = scanner_spans("scanning repo", 10 * SCAN_FRAME_MS, true, &t);
        assert_eq!(spans.len(), 13);
        let fg = |i: usize| spans[i].style.fg.unwrap();
        assert_eq!(fg(8), t.accent, "head");
        for i in 4..8 {
            assert_eq!(fg(i), t.muted, "trail cell {i}");
        }
        assert_eq!(fg(3), t.faint, "past the trail");
        assert_eq!(fg(9), t.faint, "ahead of the head");
        let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(text, "scanning repo");
    }

    #[test]
    fn scanner_reduced_motion_is_a_plain_muted_label() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let spans = scanner_spans("scanning repo", 999, false, &t);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].style.fg.unwrap(), t.muted);
        assert_eq!(spans[0].content.as_ref(), "scanning repo");
    }
}
