//! Chrome: header (logo block, tab strip, status cluster, animated tab
//! underline) and footer (keymap hints plus the honesty line). Rendered on
//! every screen.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::data::HONESTY_LINE;
use crate::tui::keymap::{self, Context, TabId};
use crate::tui::theme::Theme;

pub struct HeaderProps<'a> {
    pub active: TabId,
    /// Animated tab index (0.0..=3.0); the underline slides through
    /// fractional positions.
    pub underline_pos: f32,
    pub ingestion_on: bool,
    /// "2m" style snapshot age; None before any data loads.
    pub data_age: Option<&'a str>,
    pub version: &'a str,
    /// A cached known-newer version from the passive update check; renders
    /// as an info-colored chip. Zero network behind this.
    pub update: Option<&'a str>,
    /// Busy cluster: (spinner glyph, styled label spans) while a worker
    /// runs. The scan state passes the animated scanner sweep here.
    pub busy: Option<(&'a str, Vec<Span<'static>>)>,
}

/// The logo block text; tab segments anchor after it.
const LOGO: &str = " tolkin ";

/// (x, width) of each tab label in the header row. Deterministic (the logo
/// and titles are fixed), shared by the underline rail and mouse
/// hit-testing so a click can never miss the drawn label.
pub fn tab_segments() -> [(u16, u16); 4] {
    let mut out = [(0u16, 0u16); 4];
    let mut cursor = (LOGO.len() + 2) as u16;
    for (i, tab) in TabId::ALL.iter().enumerate() {
        let w = (tab.title().len() + 2) as u16;
        out[i] = (cursor, w);
        cursor += w + 1;
    }
    out
}

/// The tab under column `x` of the header label row, if any.
pub fn tab_at(x: u16) -> Option<TabId> {
    tab_segments()
        .iter()
        .zip(TabId::ALL)
        .find(|((sx, w), _)| x >= *sx && x < sx + w)
        .map(|(_, tab)| tab)
}

/// Header occupies 2 rows: tabs plus the underline rail.
pub fn render_header(frame: &mut Frame, area: Rect, props: &HeaderProps, theme: &Theme) {
    if area.height < 2 {
        return;
    }
    let row0 = Rect { height: 1, ..area };
    let row1 = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };

    // Left: logo block plus tab strip, on the shared segment geometry.
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        LOGO,
        Style::default()
            .fg(theme.bg)
            .bg(theme.accent)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw("  "));
    let segments = tab_segments();
    for tab in TabId::ALL {
        let label = format!(" {} ", tab.title());
        let style = if tab == props.active {
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.muted)
        };
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), row0);

    // Right: busy spinner, ingestion dot, data age, version.
    let status_cluster = |with_busy: bool| {
        let mut right: Vec<Span> = Vec::new();
        if with_busy {
            if let Some((glyph, label_spans)) = &props.busy {
                right.push(Span::styled(
                    format!("{glyph} "),
                    Style::default().fg(theme.accent),
                ));
                right.extend(label_spans.iter().cloned());
                right.push(Span::raw("  "));
            }
        }
        if props.ingestion_on {
            right.push(Span::styled("● ", Style::default().fg(theme.ok)));
            right.push(Span::styled("ingestion", Style::default().fg(theme.muted)));
        } else {
            right.push(Span::styled("○ ", Style::default().fg(theme.faint)));
            right.push(Span::styled("ingestion", Style::default().fg(theme.faint)));
        }
        if let Some(age) = props.data_age {
            right.push(Span::raw("  "));
            right.push(Span::styled("data ", Style::default().fg(theme.faint)));
            right.push(Span::styled(
                age.to_string(),
                Style::default().fg(theme.muted),
            ));
        }
        right.push(Span::raw("  "));
        right.push(Span::styled(
            format!("v{}", props.version),
            Style::default().fg(theme.faint),
        ));
        if let Some(latest) = props.update {
            right.push(Span::raw("  "));
            right.push(Span::styled(
                format!("update {latest}"),
                Style::default().fg(theme.info),
            ));
        }
        right.push(Span::raw(" "));
        Line::from(right)
    };
    // The tab strip always wins the row: when the cluster would overdraw
    // the drawn labels (80 cols while scanning), the busy label goes
    // first, and whatever still overflows clamps at the strip's edge.
    let left_end = segments.last().map(|(x, w)| x + w + 1).unwrap_or(0);
    let avail = area.width.saturating_sub(left_end);
    let mut right_line = status_cluster(true);
    if right_line.width() as u16 > avail {
        right_line = status_cluster(false);
    }
    let right_width = (right_line.width() as u16).min(avail);
    let right_area = Rect {
        x: area.x + area.width - right_width,
        width: right_width,
        ..row0
    };
    frame.render_widget(Paragraph::new(right_line), right_area);

    // Underline rail: subtle line with an accent segment sliding under the
    // active tab through fractional positions.
    let (seg_x, seg_w) = underline_segment(&segments, props.underline_pos);
    let total = area.width;
    let seg_x = seg_x.min(total);
    let seg_end = (seg_x + seg_w).min(total);
    let rail = Line::from(vec![
        Span::styled(
            "─".repeat(seg_x as usize),
            Style::default().fg(theme.border_subtle),
        ),
        Span::styled(
            "━".repeat((seg_end - seg_x) as usize),
            Style::default().fg(theme.accent),
        ),
        Span::styled(
            "─".repeat((total - seg_end) as usize),
            Style::default().fg(theme.border_subtle),
        ),
    ]);
    frame.render_widget(Paragraph::new(rail), row1);
}

/// Interpolate the underline (x, width) between tab segments for a
/// fractional position.
fn underline_segment(segments: &[(u16, u16)], pos: f32) -> (u16, u16) {
    if segments.is_empty() {
        return (0, 0);
    }
    let max_idx = (segments.len() - 1) as f32;
    let pos = pos.clamp(0.0, max_idx);
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let frac = pos - lo as f32;
    let (x0, w0) = segments[lo];
    let (x1, w1) = segments[hi];
    let x = x0 as f32 + (x1 as f32 - x0 as f32) * frac;
    let w = w0 as f32 + (w1 as f32 - w0 as f32) * frac;
    (x.round() as u16, w.round().max(1.0) as u16)
}

pub struct FooterProps<'a> {
    pub context: Context,
    pub rate_model_display: &'a str,
    pub prices_observed: &'a str,
}

/// Footer occupies 2 rows: contextual hints, then the honesty line with
/// the tier legend (text verbatim from the previous dashboard).
pub fn render_footer(frame: &mut Frame, area: Rect, props: &FooterProps, theme: &Theme) {
    if area.height < 2 {
        return;
    }
    let row0 = Rect { height: 1, ..area };
    let row1 = Rect {
        y: area.y + 1,
        height: 1,
        ..area
    };

    let mut hints: Vec<Span> = vec![Span::raw(" ")];
    for action in keymap::footer_actions(props.context) {
        if let Some((keys, hint)) = keymap::label_for(action, props.context) {
            hints.push(Span::styled(keys, Style::default().fg(theme.accent)));
            hints.push(Span::styled(
                format!(" {hint}   "),
                Style::default().fg(theme.faint),
            ));
        }
    }
    frame.render_widget(Paragraph::new(Line::from(hints)), row0);

    if let Some((keys, hint)) = keymap::label_for(keymap::Action::Palette, props.context) {
        let right = Line::from(vec![
            Span::styled(keys, Style::default().fg(theme.accent)),
            Span::styled(format!(" {hint} "), Style::default().fg(theme.faint)),
        ]);
        let w = (right.width() as u16).min(area.width);
        let right_area = Rect {
            x: area.x + area.width - w,
            width: w,
            ..row0
        };
        frame.render_widget(Paragraph::new(right), right_area);
    }

    // The honesty sentence is sacred and always leads, verbatim at every
    // rung. The legend prefers today's full text; when the terminal cannot
    // carry it, a compressed legend keeps the prices date visible; when
    // even that overflows (the 80-column floor), the tier legend goes and
    // the date stays.
    let lead = format!(" {HONESTY_LINE}. ");
    let fits = |legend: &str| lead.chars().count() + legend.chars().count() <= area.width as usize;
    let full = format!(
        "tiers: identified (advisory), realized (measured structure), measured (ground truth). rate: {} prices {}.",
        props.rate_model_display, props.prices_observed
    );
    let compressed = format!(
        "tiers: identified / realized / measured. prices {}.",
        props.prices_observed
    );
    let legend = if fits(&full) {
        full
    } else if fits(&compressed) {
        compressed
    } else {
        format!("prices {}.", props.prices_observed)
    };
    let honesty = Line::from(vec![
        Span::styled(lead, Style::default().fg(theme.warn)),
        Span::styled(legend, Style::default().fg(theme.faint)),
    ]);
    frame.render_widget(Paragraph::new(honesty), row1);
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
    fn header_renders_logo_tabs_and_status() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let backend = TestBackend::new(100, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let props = HeaderProps {
                    active: TabId::Overview,
                    underline_pos: 0.0,
                    ingestion_on: true,
                    data_age: Some("2m"),
                    version: "0.14.0",
                    update: Some("0.15.0"),
                    busy: None,
                };
                render_header(f, f.area(), &props, &t);
            })
            .unwrap();
        let text = flatten(&terminal);
        assert!(text.contains("tolkin"));
        assert!(text.contains("Overview"));
        assert!(text.contains("Project"));
        assert!(text.contains("Machine"));
        assert!(text.contains("Spend"));
        assert!(text.contains("ingestion"));
        assert!(text.contains("data 2m"));
        assert!(text.contains("v0.14.0"));
        assert!(text.contains("update 0.15.0"), "update chip missing");
        assert!(text.contains("━"), "underline accent segment missing");
    }

    #[test]
    fn busy_label_yields_to_the_tab_strip_at_80_cols() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let render_at = |width: u16| {
            let backend = TestBackend::new(width, 2);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    let props = HeaderProps {
                        active: TabId::Project,
                        underline_pos: 1.0,
                        ingestion_on: true,
                        data_age: Some("2m"),
                        version: "0.14.0",
                        update: None,
                        busy: Some(("⠋", vec![Span::raw("scanning repo")])),
                    };
                    render_header(f, f.area(), &props, &t);
                })
                .unwrap();
            flatten(&terminal)
        };

        // 80 columns while scanning: every tab label renders intact and
        // the busy label is dropped instead of overdrawing the strip.
        let text = render_at(80);
        for label in ["Overview", "Project", "Machine", "Spend"] {
            assert!(text.contains(label), "{label} overdrawn at 80 cols: {text}");
        }
        assert!(
            !text.contains("scanning"),
            "busy label must yield to the tabs: {text}"
        );
        assert!(text.contains("ingestion"), "status dot survives: {text}");
        assert!(text.contains("v0.14.0"), "version survives: {text}");

        // A wide frame keeps the busy label.
        let text = render_at(120);
        assert!(
            text.contains("scanning repo"),
            "wide frames keep the busy label: {text}"
        );
    }

    #[test]
    fn footer_renders_hints_and_honesty_line_verbatim() {
        let t = theme::by_name("mono", false).unwrap();
        let backend = TestBackend::new(170, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                let props = FooterProps {
                    context: Context::List,
                    rate_model_display: "claude-sonnet-4.6",
                    prices_observed: "2026-06-10",
                };
                render_footer(f, f.area(), &props, &t);
            })
            .unwrap();
        let text = flatten(&terminal);
        assert!(text.contains("input savings, output may vary"));
        assert!(text.contains("j/k"));
        assert!(text.contains("enter detail"), "detail hint: {text}");
        assert!(text.contains("ctrl+k commands"), "palette hint: {text}");
        assert!(text.contains("prices 2026-06-10"));
    }

    #[test]
    fn footer_compresses_the_legend_on_narrow_frames_keeping_the_prices_date() {
        let t = theme::by_name("mono", false).unwrap();
        let render_at = |width: u16| {
            let backend = TestBackend::new(width, 2);
            let mut terminal = Terminal::new(backend).unwrap();
            terminal
                .draw(|f| {
                    let props = FooterProps {
                        context: Context::List,
                        rate_model_display: "claude-sonnet-4.6",
                        prices_observed: "2026-06-10",
                    };
                    render_footer(f, f.area(), &props, &t);
                })
                .unwrap();
            flatten(&terminal)
        };

        // 100 columns: the compressed legend rung carries the tier names.
        let text = render_at(100);
        assert!(
            text.contains("input savings, output may vary"),
            "honesty verbatim at every width"
        );
        assert!(
            text.contains("prices 2026-06-10"),
            "prices date survives the narrow footer: {text}"
        );
        assert!(
            text.contains("identified / realized / measured"),
            "compressed legend: {text}"
        );

        // 80 columns (the minimum frame): even the compressed legend
        // overflows, so the tier names go and the honesty sentence plus
        // the prices date stay, uncut.
        let text = render_at(80);
        assert!(
            text.contains("input savings, output may vary"),
            "honesty verbatim at 80 cols: {text}"
        );
        assert!(
            text.contains("prices 2026-06-10."),
            "prices date survives the 80-col footer: {text}"
        );
        assert!(
            !text.contains("identified"),
            "tier legend dropped before the date: {text}"
        );
    }

    #[test]
    fn tab_at_maps_clicks_onto_the_drawn_labels() {
        let segments = tab_segments();
        for (i, tab) in TabId::ALL.iter().enumerate() {
            let (x, w) = segments[i];
            assert_eq!(tab_at(x), Some(*tab), "left edge of {tab:?}");
            assert_eq!(tab_at(x + w - 1), Some(*tab), "right edge of {tab:?}");
        }
        // The logo block and the gaps between labels miss.
        assert_eq!(tab_at(0), None);
        let (x1, _) = segments[1];
        assert_eq!(tab_at(x1 - 1), None, "gap between tabs");
    }

    #[test]
    fn underline_segment_interpolates_between_tabs() {
        let segs = vec![(10, 10), (22, 9), (34, 9), (46, 7)];
        assert_eq!(underline_segment(&segs, 0.0), (10, 10));
        assert_eq!(underline_segment(&segs, 3.0), (46, 7));
        let (x, w) = underline_segment(&segs, 0.5);
        assert!(x > 10 && x < 22, "midpoint x: {x}");
        assert!((9..=10).contains(&w), "midpoint w: {w}");
        // Out-of-range clamps.
        assert_eq!(underline_segment(&segs, 9.0), (46, 7));
    }
}
