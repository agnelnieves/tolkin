//! Keymap: every dashboard action declared exactly once.
//!
//! The footer hints, the help overlay, and the command palette all render
//! from this table, so a binding can never drift between surfaces. Wave 1
//! wires navigation, refresh, rescan, and theme cycling; later waves attach
//! behavior to the remaining actions (the bindings already exist so help
//! and palette generation stay complete).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Dashboard tab identity and ordering. Overview is the default tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabId {
    Overview,
    Project,
    Machine,
    Spend,
}

impl TabId {
    pub const ALL: [TabId; 4] = [
        TabId::Overview,
        TabId::Project,
        TabId::Machine,
        TabId::Spend,
    ];

    pub fn title(self) -> &'static str {
        match self {
            TabId::Overview => "Overview",
            TabId::Project => "Project",
            TabId::Machine => "Machine",
            TabId::Spend => "Spend",
        }
    }

    pub fn index(self) -> usize {
        match self {
            TabId::Overview => 0,
            TabId::Project => 1,
            TabId::Machine => 2,
            TabId::Spend => 3,
        }
    }

    pub fn next(self) -> TabId {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> TabId {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// Every action the dashboard can perform. Some are palette-reachable only
/// in later waves; all are declared here so the table stays the single
/// source of truth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    Back,
    GoTab(TabId),
    NextTab,
    PrevTab,
    Down,
    Up,
    Top,
    Bottom,
    HalfPageDown,
    HalfPageUp,
    DayLeft,
    DayRight,
    PanelNext,
    PanelPrev,
    OpenDetail,
    Refresh,
    Rescan,
    AuditSelected,
    GenerateReport,
    CopySelection,
    FilterList,
    CycleSort,
    CycleTheme,
    Help,
    Palette,
}

/// Input context the binding applies in. `Global` bindings resolve in every
/// non-input context as a fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Global,
    List,
    DayStrip,
    Modal,
    /// Constructed when the palette ships (next wave); resolve already
    /// implements its typing rules.
    #[allow(dead_code)]
    PaletteInput,
    /// Constructed when the list filter ships (next wave).
    #[allow(dead_code)]
    FilterInput,
}

/// Help overlay grouping (rendered in later waves; declared now so the
/// table never needs a schema change).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    Navigate,
    Act,
    View,
}

pub struct Binding {
    pub keys: &'static [(KeyCode, KeyModifiers)],
    pub action: Action,
    pub context: Context,
    /// Rendered key label, e.g. "j/k".
    pub keys_label: &'static str,
    /// Short hint verb, e.g. "navigate".
    pub hint: &'static str,
    /// Help overlay grouping; read when the help overlay ships.
    #[allow(dead_code)]
    pub group: Group,
}

const NONE: KeyModifiers = KeyModifiers::NONE;
const CTRL: KeyModifiers = KeyModifiers::CONTROL;
/// Crossterm reports shifted printable keys with the SHIFT modifier set.
const SHIFT: KeyModifiers = KeyModifiers::SHIFT;

/// The binding table. Order within a context is the footer-hint priority
/// order (first bindings are the most discoverable).
pub static BINDINGS: &[Binding] = &[
    // Global.
    Binding {
        keys: &[(KeyCode::Char('q'), NONE), (KeyCode::Char('c'), CTRL)],
        action: Action::Quit,
        context: Context::Global,
        keys_label: "q",
        hint: "quit",
        group: Group::Act,
    },
    Binding {
        keys: &[(KeyCode::Esc, NONE)],
        action: Action::Back,
        context: Context::Global,
        keys_label: "esc",
        hint: "back",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('1'), NONE)],
        action: Action::GoTab(TabId::Overview),
        context: Context::Global,
        keys_label: "1",
        hint: "overview",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('2'), NONE)],
        action: Action::GoTab(TabId::Project),
        context: Context::Global,
        keys_label: "2",
        hint: "project",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('3'), NONE)],
        action: Action::GoTab(TabId::Machine),
        context: Context::Global,
        keys_label: "3",
        hint: "machine",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('4'), NONE)],
        action: Action::GoTab(TabId::Spend),
        context: Context::Global,
        keys_label: "4",
        hint: "spend",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Tab, NONE)],
        action: Action::NextTab,
        context: Context::Global,
        keys_label: "tab",
        hint: "next tab",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::BackTab, NONE), (KeyCode::BackTab, SHIFT)],
        action: Action::PrevTab,
        context: Context::Global,
        keys_label: "shift+tab",
        hint: "prev tab",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char(']'), NONE)],
        action: Action::PanelNext,
        context: Context::Global,
        keys_label: "]",
        hint: "next panel",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('['), NONE)],
        action: Action::PanelPrev,
        context: Context::Global,
        keys_label: "[",
        hint: "prev panel",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('r'), NONE)],
        action: Action::Refresh,
        context: Context::Global,
        keys_label: "r",
        hint: "refresh",
        group: Group::Act,
    },
    Binding {
        keys: &[(KeyCode::Char('s'), NONE)],
        action: Action::Rescan,
        context: Context::Global,
        keys_label: "s",
        hint: "scan",
        group: Group::Act,
    },
    Binding {
        keys: &[(KeyCode::Char('o'), NONE)],
        action: Action::GenerateReport,
        context: Context::Global,
        keys_label: "o",
        hint: "report",
        group: Group::Act,
    },
    Binding {
        keys: &[(KeyCode::Char('t'), NONE)],
        action: Action::CycleTheme,
        context: Context::Global,
        keys_label: "t",
        hint: "theme",
        group: Group::View,
    },
    Binding {
        keys: &[(KeyCode::Char('?'), NONE), (KeyCode::Char('?'), SHIFT)],
        action: Action::Help,
        context: Context::Global,
        keys_label: "?",
        hint: "help",
        group: Group::View,
    },
    Binding {
        keys: &[
            (KeyCode::Char('k'), CTRL),
            (KeyCode::Char(':'), NONE),
            (KeyCode::Char(':'), SHIFT),
        ],
        action: Action::Palette,
        context: Context::Global,
        keys_label: "ctrl+k",
        hint: "commands",
        group: Group::View,
    },
    // List.
    Binding {
        keys: &[(KeyCode::Char('j'), NONE), (KeyCode::Down, NONE)],
        action: Action::Down,
        context: Context::List,
        keys_label: "j/k",
        hint: "navigate",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('k'), NONE), (KeyCode::Up, NONE)],
        action: Action::Up,
        context: Context::List,
        keys_label: "k",
        hint: "up",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('g'), NONE)],
        action: Action::Top,
        context: Context::List,
        keys_label: "g",
        hint: "top",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('G'), NONE), (KeyCode::Char('G'), SHIFT)],
        action: Action::Bottom,
        context: Context::List,
        keys_label: "G",
        hint: "bottom",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('d'), CTRL)],
        action: Action::HalfPageDown,
        context: Context::List,
        keys_label: "ctrl+d",
        hint: "half page down",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('u'), CTRL)],
        action: Action::HalfPageUp,
        context: Context::List,
        keys_label: "ctrl+u",
        hint: "half page up",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Enter, NONE)],
        action: Action::OpenDetail,
        context: Context::List,
        keys_label: "enter",
        hint: "detail",
        group: Group::Act,
    },
    Binding {
        keys: &[(KeyCode::Char('a'), NONE)],
        action: Action::AuditSelected,
        context: Context::List,
        keys_label: "a",
        hint: "audit",
        group: Group::Act,
    },
    Binding {
        keys: &[(KeyCode::Char('y'), NONE)],
        action: Action::CopySelection,
        context: Context::List,
        keys_label: "y",
        hint: "copy",
        group: Group::Act,
    },
    Binding {
        keys: &[(KeyCode::Char('/'), NONE)],
        action: Action::FilterList,
        context: Context::List,
        keys_label: "/",
        hint: "filter",
        group: Group::Act,
    },
    Binding {
        keys: &[(KeyCode::Char(','), NONE)],
        action: Action::CycleSort,
        context: Context::List,
        keys_label: ",",
        hint: "sort",
        group: Group::View,
    },
    // DayStrip (Spend 30-day chart).
    Binding {
        keys: &[(KeyCode::Char('h'), NONE), (KeyCode::Left, NONE)],
        action: Action::DayLeft,
        context: Context::DayStrip,
        keys_label: "h/l",
        hint: "day",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Char('l'), NONE), (KeyCode::Right, NONE)],
        action: Action::DayRight,
        context: Context::DayStrip,
        keys_label: "l",
        hint: "day right",
        group: Group::Navigate,
    },
    Binding {
        keys: &[(KeyCode::Enter, NONE)],
        action: Action::OpenDetail,
        context: Context::Modal,
        keys_label: "enter",
        hint: "confirm",
        group: Group::Act,
    },
    // Inside the file-detail modal, `a` (re)runs the audit worker.
    Binding {
        keys: &[(KeyCode::Char('a'), NONE)],
        action: Action::AuditSelected,
        context: Context::Modal,
        keys_label: "a",
        hint: "audit",
        group: Group::Act,
    },
];

/// True for contexts where the user is typing free text: printable keys
/// must never resolve to global single-char actions there (`q` types a q,
/// it does not quit).
fn is_input_context(context: Context) -> bool {
    matches!(context, Context::PaletteInput | Context::FilterInput)
}

fn key_matches(binding: &Binding, key: &KeyEvent) -> bool {
    binding
        .keys
        .iter()
        .any(|(code, mods)| *code == key.code && *mods == key.modifiers)
}

/// Resolve a key event in a context: context-local bindings first, then the
/// global fallback. Input contexts only resolve esc and ctrl+c.
pub fn resolve(key: &KeyEvent, context: Context) -> Option<Action> {
    if is_input_context(context) {
        if key.code == KeyCode::Esc {
            return Some(Action::Back);
        }
        if key.code == KeyCode::Char('c') && key.modifiers == KeyModifiers::CONTROL {
            return Some(Action::Quit);
        }
        return None;
    }
    if let Some(b) = BINDINGS
        .iter()
        .filter(|b| b.context == context)
        .find(|b| key_matches(b, key))
    {
        return Some(b.action);
    }
    BINDINGS
        .iter()
        .filter(|b| b.context == Context::Global)
        .find(|b| key_matches(b, key))
        .map(|b| b.action)
}

/// Lookup the rendered (keys_label, hint) pair for an action, preferring
/// the context-local binding.
pub fn label_for(action: Action, context: Context) -> Option<(&'static str, &'static str)> {
    BINDINGS
        .iter()
        .filter(|b| b.action == action)
        .min_by_key(|b| if b.context == context { 0 } else { 1 })
        .map(|b| (b.keys_label, b.hint))
}

/// The top footer hints for a context: the actions Wave 1 actually performs,
/// most discoverable first. Rendered by the chrome from the table so labels
/// never drift.
pub fn footer_actions(context: Context) -> Vec<Action> {
    match context {
        Context::List => vec![
            Action::Down,
            Action::NextTab,
            Action::Refresh,
            Action::Rescan,
        ],
        Context::DayStrip => vec![
            Action::DayLeft,
            Action::PanelNext,
            Action::NextTab,
            Action::Refresh,
        ],
        Context::Modal => vec![Action::Back],
        _ => vec![
            Action::NextTab,
            Action::Refresh,
            Action::Rescan,
            Action::CycleTheme,
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn no_duplicate_binding_within_a_context() {
        let mut seen: HashSet<(u8, String)> = HashSet::new();
        for b in BINDINGS {
            let ctx_id = match b.context {
                Context::Global => 0u8,
                Context::List => 1,
                Context::DayStrip => 2,
                Context::Modal => 3,
                Context::PaletteInput => 4,
                Context::FilterInput => 5,
            };
            for (code, mods) in b.keys {
                let key = (ctx_id, format!("{code:?}+{mods:?}"));
                assert!(
                    seen.insert(key.clone()),
                    "duplicate binding {key:?} for {:?}",
                    b.action
                );
            }
        }
    }

    #[test]
    fn every_action_is_bound() {
        let bound: Vec<Action> = BINDINGS.iter().map(|b| b.action).collect();
        let all = [
            Action::Quit,
            Action::Back,
            Action::GoTab(TabId::Overview),
            Action::GoTab(TabId::Project),
            Action::GoTab(TabId::Machine),
            Action::GoTab(TabId::Spend),
            Action::NextTab,
            Action::PrevTab,
            Action::Down,
            Action::Up,
            Action::Top,
            Action::Bottom,
            Action::HalfPageDown,
            Action::HalfPageUp,
            Action::DayLeft,
            Action::DayRight,
            Action::PanelNext,
            Action::PanelPrev,
            Action::OpenDetail,
            Action::Refresh,
            Action::Rescan,
            Action::AuditSelected,
            Action::GenerateReport,
            Action::CopySelection,
            Action::FilterList,
            Action::CycleSort,
            Action::CycleTheme,
            Action::Help,
            Action::Palette,
        ];
        for action in all {
            assert!(bound.contains(&action), "{action:?} has no binding");
        }
    }

    #[test]
    fn list_context_resolves_local_then_global() {
        let j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(resolve(&j, Context::List), Some(Action::Down));
        // Arrows navigate, they no longer switch tabs.
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(resolve(&down, Context::List), Some(Action::Down));
        let left = KeyEvent::new(KeyCode::Left, KeyModifiers::NONE);
        assert_eq!(
            resolve(&left, Context::List),
            None,
            "left must not switch tabs"
        );
        // Global fallback still applies inside lists.
        let r = KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE);
        assert_eq!(resolve(&r, Context::List), Some(Action::Refresh));
        let two = KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE);
        assert_eq!(
            resolve(&two, Context::List),
            Some(Action::GoTab(TabId::Project))
        );
    }

    #[test]
    fn day_strip_owns_h_and_l() {
        let h = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
        assert_eq!(resolve(&h, Context::DayStrip), Some(Action::DayLeft));
        let l = KeyEvent::new(KeyCode::Right, KeyModifiers::NONE);
        assert_eq!(resolve(&l, Context::DayStrip), Some(Action::DayRight));
    }

    #[test]
    fn input_contexts_suppress_q_but_keep_escape_hatches() {
        let q = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(resolve(&q, Context::PaletteInput), None);
        assert_eq!(resolve(&q, Context::FilterInput), None);
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(resolve(&esc, Context::PaletteInput), Some(Action::Back));
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(resolve(&ctrl_c, Context::FilterInput), Some(Action::Quit));
    }

    #[test]
    fn tab_id_order_and_titles() {
        assert_eq!(TabId::Overview.next(), TabId::Project);
        assert_eq!(TabId::Spend.next(), TabId::Overview);
        assert_eq!(TabId::Overview.prev(), TabId::Spend);
        let titles: Vec<&str> = TabId::ALL.iter().map(|t| t.title()).collect();
        assert_eq!(titles, vec!["Overview", "Project", "Machine", "Spend"]);
    }

    #[test]
    fn footer_actions_have_labels_in_table() {
        for ctx in [
            Context::Global,
            Context::List,
            Context::DayStrip,
            Context::Modal,
        ] {
            for action in footer_actions(ctx) {
                assert!(
                    label_for(action, ctx).is_some(),
                    "footer action {action:?} missing from table"
                );
            }
        }
    }
}
