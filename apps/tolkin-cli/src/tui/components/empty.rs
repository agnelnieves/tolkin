//! Empty and zero states. The setup card is the brand moment for a fresh
//! data dir; the consent panel covers ingestion-off sections; the
//! too-small card names the JSON fallback for tiny terminals.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Padding, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

/// Center a w x h box inside `area`, clamped to fit.
pub fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

/// Breathing period of the setup-card border (section 5: sine, 4600 ms).
pub const SETUP_PULSE_PERIOD_MS: u64 = 4_600;

/// Map the 0..=1 breathing phase onto the 3-step accent ramp. The middle
/// step is the plain accent, which is also the reduced-motion rest state
/// (a disabled animator pins the phase at 0.5).
fn pulse_border(pulse: f32, theme: &Theme) -> ratatui::style::Color {
    if pulse < 1.0 / 3.0 {
        theme.border_active
    } else if pulse < 2.0 / 3.0 {
        theme.accent
    } else {
        theme.accent_bright
    }
}

/// The setup card for a fresh data dir (no config, no ledger). The ONLY
/// screen with an idle animation: the accent border breathes on `pulse`
/// (0..=1 sine phase sampled by the caller through the animator).
pub fn render_setup(frame: &mut Frame, area: Rect, theme: &Theme, pulse: f32) {
    let card = centered(area, 64.min(area.width), 12);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(pulse_border(pulse, theme)))
        .style(Style::default().bg(theme.surface))
        .padding(Padding::new(2, 2, 1, 1))
        .title(Span::styled(" setup ", Style::default().fg(theme.muted)));
    let inner = block.inner(card);
    frame.render_widget(block, card);

    let cmd = |c: &'static str, hint: &'static str| {
        Line::from(vec![
            Span::styled(format!("{c:<18}"), Style::default().fg(theme.accent)),
            Span::styled(hint, Style::default().fg(theme.muted)),
        ])
    };
    let lines = vec![
        Line::from(Span::styled(
            "tolkin",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Runs entirely on your machine. Nothing leaves.",
            Style::default().fg(theme.text),
        )),
        Line::default(),
        cmd("tolkin init", "set up the local savings ledger"),
        cmd("tolkin scan", "discover local agent configs"),
        cmd("tolkin project", "audit this repo's agent-context weight"),
        Line::default(),
        Line::from(Span::styled(
            "Press q to quit.",
            Style::default().fg(theme.faint),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Body lines for any ingestion-off section: name the exact consent
/// command, keep the privacy posture visible.
pub fn consent_lines(theme: &Theme) -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "Usage-log ingestion is off.",
            Style::default().fg(theme.warn),
        )),
        Line::default(),
        Line::from(vec![
            Span::styled("Run ", Style::default().fg(theme.muted)),
            Span::styled("tolkin init", Style::default().fg(theme.accent)),
            Span::styled(
                " and consent to log ingestion to populate this view.",
                Style::default().fg(theme.muted),
            ),
        ]),
        Line::from(Span::styled(
            "Logs read from ~/.claude/projects and ~/.codex/sessions, locally only.",
            Style::default().fg(theme.faint),
        )),
    ]
}

/// Sub-minimum terminal size: a centered card naming the JSON fallback.
pub fn render_too_small(frame: &mut Frame, area: Rect, theme: &Theme) {
    let card = centered(area, 44.min(area.width), 5.min(area.height));
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().bg(theme.surface));
    let inner = block.inner(card);
    frame.render_widget(block, card);
    let lines = vec![
        Line::from(Span::styled(
            "terminal too small (need 80x24)",
            Style::default().fg(theme.warn),
        )),
        Line::from(vec![
            Span::styled("try ", Style::default().fg(theme.muted)),
            Span::styled("tolkin stats --json", Style::default().fg(theme.accent)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn flatten(terminal: &Terminal<TestBackend>) -> String {
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn setup_card_names_the_three_commands() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_setup(f, f.area(), &t, 0.5))
            .unwrap();
        let text = flatten(&terminal);
        assert!(text.contains("tolkin init"));
        assert!(text.contains("tolkin scan"));
        assert!(text.contains("tolkin project"));
        assert!(text.contains("Nothing leaves"));
        assert!(text.contains("q to quit"));
    }

    #[test]
    fn setup_border_breathes_through_the_three_step_ramp() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let border_at = |pulse: f32| {
            let backend = TestBackend::new(80, 24);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| render_setup(f, f.area(), &t, pulse))
                .unwrap();
            let buf = terminal.backend().buffer();
            // The card is centered: 64 wide in 80 -> x 8, 12 tall in 24 -> y 6.
            buf[(8, 6)].fg
        };
        assert_eq!(border_at(0.0), t.border_active, "trough dims");
        assert_eq!(border_at(0.5), t.accent, "mid step is plain accent");
        assert_eq!(border_at(1.0), t.accent_bright, "peak brightens");
    }

    #[test]
    fn too_small_card_names_json_fallback() {
        let t = theme::by_name("mono", false).unwrap();
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render_too_small(f, f.area(), &t))
            .unwrap();
        let text = flatten(&terminal);
        assert!(text.contains("tolkin stats --json"));
    }

    #[test]
    fn centered_clamps_to_area() {
        let r = centered(Rect::new(0, 0, 10, 5), 100, 100);
        assert_eq!(r, Rect::new(0, 0, 10, 5));
    }
}
