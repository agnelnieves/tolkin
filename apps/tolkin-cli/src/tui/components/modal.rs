//! Modal dialog shell: centered box over a dimmed scrim. The detail bodies
//! are composed by the app layer; this module owns the scrim, the widths,
//! the placement, and the esc affordance, so overlays stack correctly.

use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use ratatui::Frame;

use crate::tui::theme::{self, Theme};

/// The three dialog widths from the design contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModalWidth {
    Medium,
    Large,
    XLarge,
}

impl ModalWidth {
    pub fn cols(self) -> u16 {
        match self {
            ModalWidth::Medium => 60,
            ModalWidth::Large => 88,
            ModalWidth::XLarge => 116,
        }
    }
}

/// The dialog rectangle for a body of `lines` rows: width clamped to the
/// frame minus 2, top at height/4. Mouse hit-testing shares this with the
/// render path so a click can never disagree with what was drawn.
pub fn dialog_rect(area: Rect, width: ModalWidth, lines: u16) -> Rect {
    let w = width.cols().min(area.width.saturating_sub(2));
    let h = (lines + 4).min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = (area.height / 4).min(area.height.saturating_sub(h));
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

/// Dim every cell of the current frame under the modal. The mono theme
/// skips the dim (its palette has no headroom) and relies on the cleared
/// dialog region for separation.
fn apply_scrim(frame: &mut Frame, theme: &Theme) {
    if theme.name == "mono" {
        return;
    }
    let area = frame.area();
    let factor = theme.scrim_factor;
    let buf = frame.buffer_mut();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            let cell = &mut buf[(x, y)];
            let fg = theme::scrim_dim(cell.fg, factor);
            let bg = theme::scrim_dim(cell.bg, factor);
            cell.fg = fg;
            cell.bg = bg;
        }
    }
}

/// Render the dialog shell: scrim, centered box at height/4, title, body,
/// esc hint. Overlay stacking is the caller's job (render each overlay
/// bottom-up; each one dims what sits beneath it).
pub fn render_modal(
    frame: &mut Frame,
    title: &str,
    body: &[Line],
    width: ModalWidth,
    theme: &Theme,
) {
    apply_scrim(frame, theme);
    let area = frame.area();
    let dialog = dialog_rect(area, width, body.len() as u16);

    frame.render_widget(Clear, dialog);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_active))
        .style(Style::default().bg(theme.overlay))
        .padding(Padding::horizontal(1))
        .title(Span::styled(
            format!(" {title} "),
            Style::default().fg(theme.text),
        ))
        .title_bottom(
            Line::from(Span::styled(
                " esc close ",
                Style::default().fg(theme.faint),
            ))
            .right_aligned(),
        );
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    frame.render_widget(Paragraph::new(body.to_vec()), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn modal_renders_title_body_and_esc_hint_over_scrim() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let painted = t.text;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                // Paint something underneath so the scrim has work to do.
                let bg = Paragraph::new("UNDERNEATH").style(Style::default().fg(painted));
                f.render_widget(bg, f.area());
                let body = vec![Line::from("body line one"), Line::from("body line two")];
                render_modal(f, "detail", &body, ModalWidth::Medium, &t);
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..24 {
            for x in 0..80 {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.contains("detail"));
        assert!(text.contains("body line one"));
        assert!(text.contains("esc close"));
        // The underlying cell outside the dialog got dimmed.
        let cell = &buf[(0, 0)];
        assert_eq!(
            cell.fg,
            theme::scrim_dim(painted, t.scrim_factor),
            "scrim must dim fg, got {:?}",
            cell.fg
        );
    }

    #[test]
    fn mono_theme_skips_the_scrim() {
        let t = theme::by_name("mono", false).unwrap();
        let painted = t.muted;
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let bg = Paragraph::new("UNDERNEATH").style(Style::default().fg(painted));
                f.render_widget(bg, f.area());
                render_modal(f, "t", &[Line::from("x")], ModalWidth::Medium, &t);
            })
            .unwrap();
        let buf = terminal.backend().buffer();
        assert_eq!(buf[(0, 0)].fg, painted, "mono must not dim");
    }

    #[test]
    fn modal_clamps_to_small_terminals() {
        let t = theme::by_name("terminal", false).unwrap();
        let backend = TestBackend::new(40, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let body = vec![Line::from("x")];
                render_modal(f, "t", &body, ModalWidth::XLarge, &t);
            })
            .unwrap();
        // No panic = pass; the dialog clamped to 38 cols.
    }
}
