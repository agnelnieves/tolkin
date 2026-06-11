//! Help overlay body: keymap groups generated from keymap.rs (Navigate
//! left, Act and View right) plus the Agents section listing the CLI
//! equivalents, exit codes, and environment switches verbatim.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui::keymap::{self, Group};
use crate::tui::theme::Theme;

/// CLI equivalents listed verbatim for agents; the help overlay is the
/// discoverability surface for the non-TTY contracts.
pub const AGENT_COMMANDS: [(&str, &str); 4] = [
    ("tolkin stats --json --global", "machine-wide stats as JSON"),
    ("tolkin stats --compact", "one static dashboard frame"),
    ("tolkin project --json", "project scan as JSON"),
    ("tolkin mcp --json", "MCP probe as JSON"),
];

/// Column geometry: two keymap columns inside the large (88) dialog.
const COL: usize = 41;
const KEY_W: usize = 11;

fn header(title: &str, theme: &Theme) -> Vec<Span<'static>> {
    vec![Span::styled(
        format!("{title:<COL$}"),
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    )]
}

fn entry(keys: &str, hint: &str, theme: &Theme) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!("{keys:<KEY_W$}"), Style::default().fg(theme.accent)),
        Span::styled(
            format!("{hint:<width$}", width = COL.saturating_sub(KEY_W)),
            Style::default().fg(theme.muted),
        ),
    ]
}

fn blank() -> Vec<Span<'static>> {
    vec![Span::raw(" ".repeat(COL))]
}

/// Build the full help body: a two-column keymap block, then the agents
/// section across the full width.
pub fn lines(theme: &Theme) -> Vec<Line<'static>> {
    // Left column: Navigate. Right column: Act, a blank, then View.
    let mut left: Vec<Vec<Span>> = vec![header("navigate", theme)];
    for (_, keys, hint) in keymap::help_entries(Group::Navigate) {
        left.push(entry(keys, hint, theme));
    }
    let mut right: Vec<Vec<Span>> = vec![header("act", theme)];
    for (_, keys, hint) in keymap::help_entries(Group::Act) {
        right.push(entry(keys, hint, theme));
    }
    right.push(blank());
    right.push(header("view", theme));
    for (_, keys, hint) in keymap::help_entries(Group::View) {
        right.push(entry(keys, hint, theme));
    }

    let rows = left.len().max(right.len());
    let mut out: Vec<Line> = Vec::with_capacity(rows + 9);
    for i in 0..rows {
        let mut spans = left.get(i).cloned().unwrap_or_else(blank);
        spans.push(Span::raw("  "));
        spans.extend(right.get(i).cloned().unwrap_or_default());
        out.push(Line::from(spans));
    }

    out.push(Line::default());
    out.push(Line::from(Span::styled(
        "agents",
        Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
    )));
    for (cmd, what) in AGENT_COMMANDS {
        out.push(Line::from(vec![
            Span::styled(format!("{cmd:<30}"), Style::default().fg(theme.accent)),
            Span::styled(what.to_string(), Style::default().fg(theme.muted)),
        ]));
    }
    out.push(Line::from(Span::styled(
        "bare tolkin without a TTY exits 2 with usage on stderr",
        Style::default().fg(theme.muted),
    )));
    out.push(Line::from(vec![
        Span::styled("env: ", Style::default().fg(theme.muted)),
        Span::styled("NO_COLOR", Style::default().fg(theme.accent)),
        Span::styled(" (mono), ", Style::default().fg(theme.muted)),
        Span::styled("TOLKIN_REDUCED_MOTION", Style::default().fg(theme.accent)),
        Span::styled("=1 (no motion), ", Style::default().fg(theme.muted)),
        Span::styled("TOLKIN_THEME", Style::default().fg(theme.accent)),
        Span::styled(" (theme)", Style::default().fg(theme.muted)),
    ]));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::theme;

    fn flat(lines: &[Line]) -> String {
        lines
            .iter()
            .map(|l| {
                l.spans
                    .iter()
                    .map(|s| s.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn help_body_renders_groups_agents_and_env_contracts() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let text = flat(&lines(&t));
        for needle in [
            "navigate",
            "act",
            "view",
            "agents",
            "tolkin stats --json --global",
            "tolkin stats --compact",
            "tolkin project --json",
            "tolkin mcp --json",
            "exits 2",
            "NO_COLOR",
            "TOLKIN_REDUCED_MOTION",
            "TOLKIN_THEME",
        ] {
            assert!(text.contains(needle), "{needle} missing:\n{text}");
        }
        // Every generated keymap entry's label reaches the body.
        for group in [Group::Navigate, Group::Act, Group::View] {
            for (_, keys, hint) in keymap::help_entries(group) {
                assert!(text.contains(keys), "{keys} missing");
                assert!(text.contains(hint), "{hint} missing");
            }
        }
    }
}
