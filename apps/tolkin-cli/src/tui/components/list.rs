//! SelectList: vim-navigable selection with a rail, contrast-safe selected
//! row, and a hand-drawn scrollbar.
//!
//! Selection state lives on the model; the visible window is a pure
//! function of (selected, len, height) so view never mutates anything.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::tui::theme::Theme;

/// Cursor for one list. Movement clamps to the list length supplied by the
/// caller (derived data can shrink between frames).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    pub idx: usize,
}

impl Selection {
    pub fn down(&mut self, len: usize) {
        if len > 0 && self.idx + 1 < len {
            self.idx += 1;
        }
    }

    pub fn up(&mut self) {
        self.idx = self.idx.saturating_sub(1);
    }

    pub fn top(&mut self) {
        self.idx = 0;
    }

    pub fn bottom(&mut self, len: usize) {
        self.idx = len.saturating_sub(1);
    }

    pub fn half_down(&mut self, len: usize, page: usize) {
        if len == 0 {
            self.idx = 0;
            return;
        }
        self.idx = (self.idx + (page / 2).max(1)).min(len - 1);
    }

    pub fn half_up(&mut self, page: usize) {
        self.idx = self.idx.saturating_sub((page / 2).max(1));
    }

    /// Re-clamp after the underlying data changed.
    pub fn clamp(&mut self, len: usize) {
        if len == 0 {
            self.idx = 0;
        } else if self.idx >= len {
            self.idx = len - 1;
        }
    }
}

/// Visible half-open row range keeping `selected` in view: scrolls only
/// when the cursor would leave the viewport.
pub fn visible_window(selected: usize, len: usize, height: usize) -> (usize, usize) {
    if height == 0 || len == 0 {
        return (0, 0);
    }
    let start = selected.saturating_sub(height.saturating_sub(1));
    let start = start.min(len.saturating_sub(height.min(len)));
    (start, (start + height).min(len))
}

/// Render rows with a selection rail and scrollbar. `selected` is None for
/// non-interactive lists (no rail column reserved). The selected row is
/// repainted contrast-safe: selection_fg over selection_bg, original
/// modifiers kept. Callers with cheap rows pass them all; long lists with
/// expensive styling should window first and call
/// [`render_select_list_window`] instead.
pub fn render_select_list(
    frame: &mut Frame,
    area: Rect,
    rows: &[Line],
    selected: Option<usize>,
    focused: bool,
    theme: &Theme,
) {
    let len = rows.len();
    let sel = selected.map(|s| s.min(len.saturating_sub(1)));
    let (start, end) = visible_window(sel.unwrap_or(0), len, area.height as usize);
    render_select_list_window(
        frame,
        area,
        &rows[start..end],
        start,
        len,
        selected,
        focused,
        theme,
    );
}

/// Windowed variant: `rows` holds styled lines for ONLY the slice
/// `[start, start + rows.len())` of a list `len` rows long. Callers compute
/// the slice with [`visible_window`] so per-frame styling work stays
/// proportional to the viewport, not the data. `selected` is an absolute
/// row index into the full list.
// One past clippy's argument budget: the windowing pair (start, len) is
// load-bearing and a wrapper struct would only rename the problem.
#[allow(clippy::too_many_arguments)]
pub fn render_select_list_window(
    frame: &mut Frame,
    area: Rect,
    rows: &[Line],
    start: usize,
    len: usize,
    selected: Option<usize>,
    focused: bool,
    theme: &Theme,
) {
    if area.width < 4 || area.height == 0 {
        return;
    }
    let height = area.height as usize;
    let sel = selected.map(|s| s.min(len.saturating_sub(1)));
    let overflow = len > height;
    let buf = frame.buffer_mut();

    for (row_offset, row) in rows.iter().enumerate().take(height) {
        let y = area.y + row_offset as u16;
        let is_selected = sel == Some(start + row_offset);
        let rail_x = area.x;
        let text_x = area.x + 2;
        // The last column is always a gutter (scrollbar when overflowing),
        // so right-aligned values never shift between states.
        let text_width = area.width.saturating_sub(3);

        if is_selected {
            // Repaint the row surface first so the selection reads as one
            // solid bar, then the rail. Mono themes select via REVERSED
            // (visible without color) instead of a painted background.
            let row_style = theme.selection_style();
            for x in area.x..area.x + area.width.saturating_sub(1) {
                buf[(x, y)].set_style(row_style);
                buf[(x, y)].set_symbol(" ");
            }
            let rail_color = if focused {
                theme.accent
            } else {
                theme.border_active
            };
            buf[(rail_x, y)]
                .set_symbol("▌")
                .set_style(row_style.fg(rail_color));
            let line = restyle_line(row, row_style);
            buf.set_line(text_x, y, &line, text_width);
        } else {
            buf.set_line(text_x, y, row, text_width);
        }
    }

    if overflow {
        render_scrollbar(frame, area, start, len, height, theme);
    }
}

/// Force fg and bg while keeping each span's modifiers (bold stays bold on
/// the selection bar).
fn restyle_line(line: &Line, style: Style) -> Line<'static> {
    let spans: Vec<Span<'static>> = line
        .spans
        .iter()
        .map(|s| {
            Span::styled(
                s.content.to_string(),
                style.add_modifier(s.style.add_modifier),
            )
        })
        .collect();
    Line::from(spans)
}

fn render_scrollbar(
    frame: &mut Frame,
    area: Rect,
    start: usize,
    len: usize,
    height: usize,
    theme: &Theme,
) {
    let x = area.x + area.width - 1;
    let thumb_len = ((height * height) / len).max(1).min(height);
    let denom = len.saturating_sub(height).max(1);
    let thumb_start = (start * (height - thumb_len)) / denom;
    let buf = frame.buffer_mut();
    for row in 0..height {
        let y = area.y + row as u16;
        if row >= thumb_start && row < thumb_start + thumb_len {
            buf[(x, y)]
                .set_symbol("█")
                .set_style(Style::default().fg(theme.border_active));
        } else {
            buf[(x, y)]
                .set_symbol("│")
                .set_style(Style::default().fg(theme.border_subtle));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn selection_moves_and_clamps() {
        let mut s = Selection::default();
        s.down(3);
        s.down(3);
        s.down(3); // clamped at len-1
        assert_eq!(s.idx, 2);
        s.up();
        assert_eq!(s.idx, 1);
        s.top();
        assert_eq!(s.idx, 0);
        s.bottom(3);
        assert_eq!(s.idx, 2);
        s.clamp(1);
        assert_eq!(s.idx, 0);
        s.half_down(20, 10);
        assert_eq!(s.idx, 5);
        s.half_up(10);
        assert_eq!(s.idx, 0);
        // Empty list never panics and pins to zero.
        let mut e = Selection { idx: 5 };
        e.clamp(0);
        assert_eq!(e.idx, 0);
        e.down(0);
        assert_eq!(e.idx, 0);
    }

    #[test]
    fn window_keeps_selection_visible() {
        assert_eq!(visible_window(0, 10, 4), (0, 4));
        assert_eq!(visible_window(3, 10, 4), (0, 4));
        assert_eq!(visible_window(4, 10, 4), (1, 5));
        assert_eq!(visible_window(9, 10, 4), (6, 10));
        assert_eq!(visible_window(0, 2, 4), (0, 2));
        assert_eq!(visible_window(0, 0, 4), (0, 0));
    }

    #[test]
    fn windowed_render_matches_the_full_row_path() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let rows: Vec<Line> = (0..40).map(|i| Line::from(format!("row {i}"))).collect();
        let selected = Some(17);
        let render = |windowed: bool| {
            let backend = TestBackend::new(24, 6);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    if windowed {
                        let (start, end) = visible_window(17, rows.len(), f.area().height as usize);
                        render_select_list_window(
                            f,
                            f.area(),
                            &rows[start..end],
                            start,
                            rows.len(),
                            selected,
                            true,
                            &t,
                        );
                    } else {
                        render_select_list(f, f.area(), &rows, selected, true, &t);
                    }
                })
                .unwrap();
            terminal.backend().buffer().clone()
        };
        assert_eq!(
            render(true),
            render(false),
            "building only the visible slice must blit identically"
        );
    }

    #[test]
    fn renders_rail_on_selected_row_and_scrollbar_on_overflow() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let rows: Vec<Line> = (0..6).map(|i| Line::from(format!("row {i}"))).collect();
        terminal
            .draw(|f| render_select_list(f, f.area(), &rows, Some(4), true, &t))
            .unwrap();
        let buf = terminal.backend().buffer();
        // Selected row 4 sits in the window (2..5), local offset 2.
        assert_eq!(buf[(0, 2)].symbol(), "▌");
        let text: String = (0..20).map(|x| buf[(x, 2)].symbol().to_string()).collect();
        assert!(text.contains("row 4"));
        // Scrollbar occupies the last column.
        let bar: Vec<&str> = (0..3).map(|y| buf[(19, y)].symbol()).collect();
        assert!(bar.contains(&"█"), "scrollbar thumb missing: {bar:?}");
    }
}
