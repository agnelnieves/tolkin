//! Command palette: a fuzzy-filtered list of every action with its
//! keybinding rendered muted on the right. Suggested actions (the focused
//! context's footer hints) float first; executing an action closes the
//! palette and performs it.
//!
//! The fuzzy filter is a subsequence match with simple scoring: word-start
//! hits and consecutive runs weigh more. No dependency, by contract.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph};
use ratatui::Frame;

use crate::tui::keymap::{self, Action};
use crate::tui::theme::Theme;

use super::list::visible_window;
use super::modal;

/// Match rows visible at once; the window follows the selection.
pub const MAX_ROWS: usize = 10;

/// Palette state owned by the app model. `matches` is recomputed on every
/// edit (in update, never in view).
pub struct PaletteState {
    pub query: String,
    pub sel: usize,
    pub matches: Vec<Action>,
    suggested: Vec<Action>,
}

impl PaletteState {
    pub fn new(suggested: Vec<Action>) -> PaletteState {
        let matches = filter_actions("", &suggested);
        PaletteState {
            query: String::new(),
            sel: 0,
            matches,
            suggested,
        }
    }

    /// Recompute the match list after an edit; the selection resets to the
    /// best match.
    pub fn refilter(&mut self) {
        self.matches = filter_actions(&self.query, &self.suggested);
        self.sel = 0;
    }
}

/// Subsequence fuzzy score. `None` when `query` is not a subsequence of
/// `candidate` (case-insensitive). Hits score 1, word starts +3, runs
/// consecutive with the previous hit +2, so "rs" prefers "rescan project"
/// over scattered matches.
pub fn fuzzy_score(query: &str, candidate: &str) -> Option<i32> {
    let q: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    if q.is_empty() {
        return Some(0);
    }
    let cand: Vec<char> = candidate.chars().flat_map(|c| c.to_lowercase()).collect();
    let mut score = 0i32;
    let mut qi = 0usize;
    let mut last_hit: Option<usize> = None;
    for (i, &c) in cand.iter().enumerate() {
        if qi >= q.len() {
            break;
        }
        if c != q[qi] {
            continue;
        }
        score += 1;
        let word_start = i == 0 || matches!(cand[i - 1], ' ' | '-' | '/');
        if word_start {
            score += 3;
        }
        if i > 0 && last_hit == Some(i - 1) {
            score += 2;
        }
        last_hit = Some(i);
        qi += 1;
    }
    (qi == q.len()).then_some(score)
}

/// Filter the full action list. Empty query: suggested actions first, then
/// the rest in table order. Non-empty: score-ranked subsequence matches,
/// suggested actions carrying a small bonus, table order as the tiebreak.
pub fn filter_actions(query: &str, suggested: &[Action]) -> Vec<Action> {
    let all = keymap::palette_actions();
    if query.is_empty() {
        let mut out: Vec<Action> = suggested
            .iter()
            .copied()
            .filter(|a| all.contains(a))
            .collect();
        for action in all {
            if !out.contains(&action) {
                out.push(action);
            }
        }
        return out;
    }
    let mut scored: Vec<(i32, usize, Action)> = Vec::new();
    for (i, action) in all.into_iter().enumerate() {
        if let Some(mut score) = fuzzy_score(query, keymap::action_name(action)) {
            if suggested.contains(&action) {
                score += 2;
            }
            scored.push((score, i, action));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, action)| action).collect()
}

/// Dialog rectangle for the palette: medium width, top at height/4, height
/// following the visible match rows. Shared with mouse hit-testing.
pub fn palette_rect(area: Rect, matches: usize) -> Rect {
    let w = 60.min(area.width.saturating_sub(2));
    let rows = matches.clamp(1, MAX_ROWS) as u16;
    // Two border rows plus the input row plus the match rows.
    let h = (rows + 3).min(area.height.saturating_sub(2));
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = (area.height / 4).min(area.height.saturating_sub(h));
    Rect {
        x,
        y,
        width: w,
        height: h,
    }
}

pub fn render_palette(frame: &mut Frame, state: &PaletteState, theme: &Theme) {
    modal::apply_scrim(frame, theme);
    let area = frame.area();
    let dialog = palette_rect(area, state.matches.len());

    frame.render_widget(Clear, dialog);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border_active))
        .style(Style::default().bg(theme.overlay))
        .padding(Padding::horizontal(1))
        .title(Span::styled(" commands ", Style::default().fg(theme.text)))
        .title_bottom(
            Line::from(Span::styled(
                " enter run  esc close ",
                Style::default().fg(theme.faint),
            ))
            .right_aligned(),
        );
    let inner = block.inner(dialog);
    frame.render_widget(block, dialog);
    if inner.height == 0 {
        return;
    }

    let mut lines: Vec<Line> = Vec::with_capacity(inner.height as usize);
    lines.push(Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent)),
        Span::styled(state.query.clone(), Style::default().fg(theme.text)),
        Span::styled("▌", Style::default().fg(theme.accent)),
    ]));

    let list_height = inner.height.saturating_sub(1) as usize;
    if state.matches.is_empty() {
        lines.push(Line::from(Span::styled(
            "no matching commands",
            Style::default().fg(theme.faint),
        )));
    } else {
        let (start, end) = visible_window(state.sel, state.matches.len(), list_height);
        let width = inner.width as usize;
        for (row, action) in state.matches[start..end].iter().enumerate() {
            let idx = start + row;
            let name = keymap::action_name(*action);
            let keys = keymap::primary_label(*action);
            let pad = width
                .saturating_sub(name.chars().count())
                .saturating_sub(keys.chars().count())
                .saturating_sub(1);
            let selected = idx == state.sel;
            let (name_style, keys_style) = if selected {
                let sel = Style::default()
                    .fg(theme.selection_fg)
                    .bg(theme.selection_bg);
                (sel.add_modifier(Modifier::BOLD), sel)
            } else {
                (
                    Style::default().fg(theme.text),
                    Style::default().fg(theme.faint),
                )
            };
            let mut spans = vec![Span::styled(name.to_string(), name_style)];
            spans.push(Span::styled(" ".repeat(pad), keys_style));
            spans.push(Span::styled(format!("{keys} "), keys_style));
            lines.push(Line::from(spans));
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
    fn fuzzy_rejects_non_subsequences_and_scores_structure() {
        assert_eq!(fuzzy_score("xyz", "rescan project"), None);
        assert_eq!(fuzzy_score("", "anything"), Some(0));
        // Word starts and consecutive runs outweigh scattered hits.
        let tight = fuzzy_score("re", "rescan project").unwrap();
        let scattered = fuzzy_score("re", "generate html report").unwrap();
        assert!(tight > scattered, "tight {tight} vs scattered {scattered}");
        let word_start = fuzzy_score("p", "go to project").unwrap();
        let mid_word = fuzzy_score("p", "help").unwrap();
        assert!(word_start > mid_word);
        // Case-insensitive.
        assert!(fuzzy_score("RE", "rescan project").is_some());
    }

    #[test]
    fn empty_query_floats_suggested_first_then_table_order() {
        let suggested = vec![Action::Rescan, Action::Help];
        let out = filter_actions("", &suggested);
        assert_eq!(out[0], Action::Rescan);
        assert_eq!(out[1], Action::Help);
        assert_eq!(
            out.len(),
            keymap::palette_actions().len(),
            "every action listed"
        );
        // No duplicates after the suggested float.
        let mut seen = Vec::new();
        for a in &out {
            assert!(!seen.contains(a));
            seen.push(*a);
        }
    }

    #[test]
    fn query_ranks_by_score_and_drops_non_matches() {
        let out = filter_actions("rescan", &[]);
        assert_eq!(out[0], Action::Rescan);
        assert!(!out.contains(&Action::Quit));
        let out = filter_actions("theme", &[]);
        assert_eq!(out[0], Action::CycleTheme);
    }

    #[test]
    fn renders_query_matches_and_keybindings() {
        let t = theme::by_name("tolkin-dark", true).unwrap();
        let mut state = PaletteState::new(vec![Action::Refresh]);
        state.query = "re".to_string();
        state.refilter();
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render_palette(f, &state, &t)).unwrap();
        let buf = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..24 {
            for x in 0..80 {
                text.push_str(buf[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(text.contains("commands"));
        assert!(text.contains("> re"));
        assert!(text.contains("refresh snapshot"), "match missing: {text}");
        assert!(text.contains(" r "), "keybinding column missing");
        assert!(text.contains("esc close"));
    }
}
