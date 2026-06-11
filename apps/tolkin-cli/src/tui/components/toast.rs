//! ToastStack: transient notices stacked top-right with variant-colored
//! side rails. Lifetimes run on the animator's clock (ManualClock in
//! tests), pruned on the tick path, never inside view.

use std::time::Instant;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

/// Toast lifetime before it auto-dismisses.
pub const TOAST_TTL_MS: u128 = 5_000;
/// Visible toasts; pushing past the cap drops the oldest.
pub const MAX_TOASTS: usize = 3;
/// Widest a toast renders, including the rail.
pub const MAX_WIDTH: u16 = 60;
/// First stacked row, under the header per the chrome spec.
const TOP_Y: u16 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Ok,
    Warn,
    Err,
}

pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    born: Instant,
}

/// The model-owned stack. Pushes record the animator clock's now; prune
/// runs on ticks with the same clock.
#[derive(Default)]
pub struct ToastStack {
    toasts: Vec<Toast>,
}

impl ToastStack {
    pub fn push(&mut self, kind: ToastKind, text: String, now: Instant) {
        self.toasts.push(Toast {
            kind,
            text,
            born: now,
        });
        if self.toasts.len() > MAX_TOASTS {
            self.toasts.remove(0);
        }
    }

    /// Drop expired toasts. `Instant::duration_since` saturates, so a
    /// just-pushed toast is always kept.
    pub fn prune(&mut self, now: Instant) {
        self.toasts
            .retain(|t| now.duration_since(t.born).as_millis() < TOAST_TTL_MS);
    }

    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }

    /// Oldest first; the renderer reverses so the newest sits on top.
    pub fn iter(&self) -> std::slice::Iter<'_, Toast> {
        self.toasts.iter()
    }
}

fn rail_color(kind: ToastKind, theme: &Theme) -> Color {
    match kind {
        ToastKind::Info => theme.info,
        ToastKind::Ok => theme.ok,
        ToastKind::Warn => theme.warn,
        ToastKind::Err => theme.err,
    }
}

/// Render the stack top-right at y 2, newest on top, each toast one row on
/// the overlay surface with a `┃` rail and a bold first word.
pub fn render_toasts(frame: &mut Frame, stack: &ToastStack, theme: &Theme) {
    let area = frame.area();
    for (i, toast) in stack.iter().rev().take(MAX_TOASTS).enumerate() {
        let y = area.y + TOP_Y + i as u16;
        if y >= area.y + area.height {
            break;
        }
        // Rail, space, text, one trailing pad cell.
        let body_width = (MAX_WIDTH as usize).saturating_sub(3);
        let shown: String = toast.text.chars().take(body_width).collect();
        let w = (shown.chars().count() as u16 + 3).min(area.width);
        let x = area.x + area.width.saturating_sub(w + 1);
        let rect = Rect {
            x,
            y,
            width: w,
            height: 1,
        };
        frame.render_widget(Clear, rect);
        let surface = Style::default().bg(theme.overlay);
        let (first, rest) = match shown.split_once(' ') {
            Some((first, rest)) => (first.to_string(), format!(" {rest}")),
            None => (shown.clone(), String::new()),
        };
        let line = Line::from(vec![
            Span::styled("┃ ", surface.fg(rail_color(toast.kind, theme))),
            Span::styled(first, surface.fg(theme.text).add_modifier(Modifier::BOLD)),
            Span::styled(rest, surface.fg(theme.muted)),
            Span::styled(" ", surface),
        ]);
        frame.render_widget(Paragraph::new(line).style(surface), rect);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::anim::{Animator, Clock, ManualClock};
    use crate::tui::theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::time::Duration;

    #[test]
    fn toasts_expire_after_the_ttl_on_a_manual_clock() {
        let clock = ManualClock::new();
        let animator = Animator::new(Box::new(clock.clone()), true);
        let mut stack = ToastStack::default();
        stack.push(ToastKind::Ok, "copied path".to_string(), animator.now());
        clock.advance(Duration::from_millis(4_999));
        stack.prune(animator.now());
        assert_eq!(stack.iter().count(), 1, "alive under the ttl");
        clock.advance(Duration::from_millis(2));
        stack.prune(animator.now());
        assert!(stack.is_empty(), "pruned past 5000 ms");
    }

    #[test]
    fn stack_caps_at_three_dropping_the_oldest() {
        let clock = ManualClock::new();
        let mut stack = ToastStack::default();
        for i in 0..4 {
            stack.push(ToastKind::Info, format!("toast {i}"), clock.now());
        }
        assert_eq!(stack.iter().count(), MAX_TOASTS);
        let texts: Vec<&str> = stack.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(texts, vec!["toast 1", "toast 2", "toast 3"]);
    }

    #[test]
    fn renders_top_right_with_rail_and_bold_first_word() {
        let clock = ManualClock::new();
        let mut stack = ToastStack::default();
        stack.push(
            ToastKind::Ok,
            "scanned 312 files in 1.2s".to_string(),
            clock.now(),
        );
        stack.push(ToastKind::Err, "error scan failed".to_string(), clock.now());
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let backend = TestBackend::new(100, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_toasts(f, &stack, &t)).unwrap();
        let buf = terminal.backend().buffer();
        let row =
            |y: u16| -> String { (0..100).map(|x| buf[(x, y)].symbol().to_string()).collect() };
        // Newest on top at y 2 with the err rail; older below.
        assert!(row(2).contains("┃ error scan failed"), "{}", row(2));
        assert!(row(3).contains("┃ scanned 312 files"), "{}", row(3));
        // Right-aligned: nothing renders in the left half.
        assert!(row(2)[..40].trim().is_empty());
        // The rail carries the variant color and the first word is bold.
        let rail_x = row(2).find('┃').unwrap() as u16;
        assert_eq!(buf[(rail_x, 2)].fg, t.err);
        let first_x = rail_x + 2;
        assert!(buf[(first_x, 2)]
            .modifier
            .contains(ratatui::style::Modifier::BOLD));
    }
}
