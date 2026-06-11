//! Theme system for the dashboard.
//!
//! Radix-style scale anchored tokens, four built-ins, environment driven
//! selection. The layering trick: surfaces differ by background, not by
//! heavy borders. Style rule for the whole TUI: no `Color::` literal lives
//! outside this file; every screen and component takes a `&Theme`.
//!
//! Detection is parameterized through [`ThemeEnv`] so unit tests inject
//! values instead of mutating process globals.

use ratatui::style::Color;

/// One color token: the truecolor value plus a hand-picked 256-color
/// fallback for terminals without `COLORTERM=truecolor`.
#[derive(Clone, Copy, Debug)]
struct Tone {
    rgb: (u8, u8, u8),
    indexed: u8,
}

const fn tone(r: u8, g: u8, b: u8, indexed: u8) -> Tone {
    Tone {
        rgb: (r, g, b),
        indexed,
    }
}

impl Tone {
    fn color(self, truecolor: bool) -> Color {
        if truecolor {
            Color::Rgb(self.rgb.0, self.rgb.1, self.rgb.2)
        } else {
            Color::Indexed(self.indexed)
        }
    }
}

/// Semantic tokens consumed by every screen and component.
#[derive(Clone, Debug)]
pub struct Theme {
    pub name: &'static str,
    // Surfaces (panels layer by background).
    pub bg: Color,
    pub surface: Color,
    pub element: Color,
    pub overlay: Color,
    // Lines.
    pub border_subtle: Color,
    pub border: Color,
    pub border_active: Color,
    // Text.
    pub text: Color,
    pub muted: Color,
    pub faint: Color,
    // Brand and semantics.
    pub accent: Color,
    pub accent_bright: Color,
    pub ok: Color,
    pub warn: Color,
    pub err: Color,
    /// Toast rails and the header update chip.
    pub info: Color,
    // Derived.
    pub selection_bg: Color,
    pub selection_fg: Color,
    pub bar_fill: Color,
    pub bar_empty: Color,
    /// Multiplier applied to RGB channels under a modal scrim.
    pub scrim_factor: f32,
}

/// Built-in theme names in cycle order.
pub const THEME_NAMES: [&str; 4] = ["tolkin-dark", "tolkin-light", "terminal", "mono"];

/// Default theme name when nothing selects otherwise.
pub const DEFAULT_THEME: &str = "tolkin-dark";

fn tolkin_dark(tc: bool) -> Theme {
    Theme {
        name: "tolkin-dark",
        bg: tone(0x0A, 0x0E, 0x12, 233).color(tc),
        surface: tone(0x11, 0x16, 0x1C, 234).color(tc),
        element: tone(0x18, 0x1F, 0x27, 235).color(tc),
        overlay: tone(0x1F, 0x29, 0x33, 236).color(tc),
        border_subtle: tone(0x2A, 0x35, 0x40, 237).color(tc),
        border: tone(0x36, 0x43, 0x52, 239).color(tc),
        border_active: tone(0x46, 0x56, 0x6A, 60).color(tc),
        text: tone(0xE6, 0xED, 0xF3, 255).color(tc),
        muted: tone(0x9D, 0xB0, 0xC0, 248).color(tc),
        faint: tone(0x5A, 0x6B, 0x7A, 242).color(tc),
        accent: tone(0x22, 0xD3, 0xEE, 45).color(tc),
        accent_bright: tone(0x67, 0xE8, 0xF9, 123).color(tc),
        ok: tone(0x4A, 0xDE, 0x80, 78).color(tc),
        warn: tone(0xFB, 0xBF, 0x24, 220).color(tc),
        err: tone(0xF8, 0x71, 0x71, 210).color(tc),
        info: tone(0x60, 0xA5, 0xFA, 75).color(tc),
        selection_bg: tone(0x22, 0xD3, 0xEE, 45).color(tc),
        selection_fg: tone(0x0A, 0x0E, 0x12, 233).color(tc),
        bar_fill: tone(0x22, 0xD3, 0xEE, 45).color(tc),
        bar_empty: tone(0x23, 0x2D, 0x38, 236).color(tc),
        scrim_factor: 0.45,
    }
}

fn tolkin_light(tc: bool) -> Theme {
    Theme {
        name: "tolkin-light",
        bg: tone(0xF7, 0xFA, 0xFC, 255).color(tc),
        surface: tone(0xEE, 0xF2, 0xF6, 254).color(tc),
        element: tone(0xE2, 0xE8, 0xF0, 253).color(tc),
        overlay: tone(0xD8, 0xE0, 0xE9, 252).color(tc),
        border_subtle: tone(0xC8, 0xD2, 0xDC, 251).color(tc),
        border: tone(0xB0, 0xBE, 0xCB, 249).color(tc),
        border_active: tone(0x90, 0xA2, 0xB3, 246).color(tc),
        text: tone(0x0B, 0x15, 0x20, 233).color(tc),
        muted: tone(0x3F, 0x4F, 0x5E, 240).color(tc),
        faint: tone(0x88, 0x95, 0xA2, 245).color(tc),
        accent: tone(0x08, 0x91, 0xB2, 31).color(tc),
        accent_bright: tone(0x06, 0xB6, 0xD4, 38).color(tc),
        ok: tone(0x16, 0xA3, 0x4A, 28).color(tc),
        warn: tone(0xB4, 0x53, 0x09, 130).color(tc),
        err: tone(0xDC, 0x26, 0x26, 160).color(tc),
        info: tone(0x25, 0x63, 0xEB, 27).color(tc),
        selection_bg: tone(0xA5, 0xF3, 0xFC, 159).color(tc),
        selection_fg: tone(0x0B, 0x15, 0x20, 233).color(tc),
        bar_fill: tone(0x08, 0x91, 0xB2, 31).color(tc),
        bar_empty: tone(0xD5, 0xDE, 0xE7, 252).color(tc),
        scrim_factor: 0.55,
    }
}

/// Respects the user's terminal background (OpenCode's `system` idea):
/// every surface is `Color::Reset` and text colors come from the 16-color
/// set so the terminal palette decides the look.
fn terminal_theme() -> Theme {
    Theme {
        name: "terminal",
        bg: Color::Reset,
        surface: Color::Reset,
        element: Color::Reset,
        overlay: Color::Reset,
        border_subtle: Color::DarkGray,
        border: Color::DarkGray,
        border_active: Color::Gray,
        text: Color::Reset,
        muted: Color::Gray,
        faint: Color::DarkGray,
        accent: Color::Cyan,
        accent_bright: Color::LightCyan,
        ok: Color::Green,
        warn: Color::Yellow,
        err: Color::Red,
        info: Color::Blue,
        selection_bg: Color::Cyan,
        selection_fg: Color::Black,
        bar_fill: Color::Cyan,
        bar_empty: Color::DarkGray,
        scrim_factor: 0.45,
    }
}

/// No color beyond Reset, White, Gray, DarkGray. Forced under `NO_COLOR`
/// or a dumb terminal.
fn mono() -> Theme {
    Theme {
        name: "mono",
        bg: Color::Reset,
        surface: Color::Reset,
        element: Color::Reset,
        overlay: Color::Reset,
        border_subtle: Color::DarkGray,
        border: Color::DarkGray,
        border_active: Color::Gray,
        text: Color::Reset,
        muted: Color::Gray,
        faint: Color::DarkGray,
        accent: Color::White,
        accent_bright: Color::White,
        ok: Color::White,
        warn: Color::White,
        err: Color::White,
        info: Color::White,
        selection_bg: Color::DarkGray,
        selection_fg: Color::White,
        bar_fill: Color::White,
        bar_empty: Color::DarkGray,
        scrim_factor: 0.45,
    }
}

/// Environment facts that drive selection and degradation. Constructed
/// from the process env exactly once (in the run() seam); tests build it
/// by hand.
#[derive(Clone, Debug, Default)]
pub struct ThemeEnv {
    /// `TOLKIN_THEME` if set and non-empty.
    pub tolkin_theme: Option<String>,
    /// `NO_COLOR` present with any value.
    pub no_color: bool,
    /// `COLORTERM` contains "truecolor" or "24bit".
    pub truecolor: bool,
    /// `TERM` is "dumb" or unset-and-empty.
    pub dumb_terminal: bool,
}

impl ThemeEnv {
    pub fn from_env() -> ThemeEnv {
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        let term = std::env::var("TERM").unwrap_or_default();
        ThemeEnv {
            tolkin_theme: std::env::var("TOLKIN_THEME").ok().filter(|v| !v.is_empty()),
            no_color: std::env::var_os("NO_COLOR").is_some(),
            truecolor: colorterm.contains("truecolor") || colorterm.contains("24bit"),
            dumb_terminal: term == "dumb",
        }
    }
}

/// Theme by name at the given color depth. Unknown names yield `None`.
pub fn by_name(name: &str, truecolor: bool) -> Option<Theme> {
    match name {
        "tolkin-dark" => Some(tolkin_dark(truecolor)),
        "tolkin-light" => Some(tolkin_light(truecolor)),
        "terminal" => Some(terminal_theme()),
        "mono" => Some(mono()),
        _ => None,
    }
}

/// Selection precedence: NO_COLOR or a dumb terminal force mono; then the
/// `TOLKIN_THEME` env; then the config's `ui_theme` (threaded by the
/// caller); then the default. Unknown names fall through to the next rung.
pub fn select(env: &ThemeEnv, config_theme: Option<&str>) -> Theme {
    if env.no_color || env.dumb_terminal {
        return mono();
    }
    if let Some(theme) = env
        .tolkin_theme
        .as_deref()
        .and_then(|n| by_name(n, env.truecolor))
    {
        return theme;
    }
    if let Some(theme) = config_theme.and_then(|n| by_name(n, env.truecolor)) {
        return theme;
    }
    by_name(DEFAULT_THEME, env.truecolor).expect("default theme exists")
}

/// Next theme in cycle order for the `t` key. When mono is forced by the
/// environment the cycle is a no-op (the forced theme is returned).
pub fn cycle(current: &str, env: &ThemeEnv) -> Theme {
    if env.no_color || env.dumb_terminal {
        return mono();
    }
    let idx = THEME_NAMES.iter().position(|n| *n == current).unwrap_or(0);
    let next = THEME_NAMES[(idx + 1) % THEME_NAMES.len()];
    by_name(next, env.truecolor).expect("cycle names are built-ins")
}

/// Dim one color toward the modal scrim. RGB multiplies channels by the
/// theme's scrim factor; indexed and named colors step to a darker ramp
/// neighbor; Reset stays Reset. Lives here so the no-Color-literals rule
/// holds everywhere else.
pub fn scrim_dim(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        Color::Indexed(n) => Color::Indexed(scrim_dim_indexed(n)),
        Color::Reset => Color::Reset,
        named => scrim_dim_named(named),
    }
}

/// 256-color dim: grayscale ramp entries step down; cube entries halve
/// each channel; bright system colors fall to their dim twins.
fn scrim_dim_indexed(n: u8) -> u8 {
    match n {
        232..=255 => n.saturating_sub(8).max(232),
        16..=231 => {
            let off = n - 16;
            let r = off / 36;
            let g = (off % 36) / 6;
            let b = off % 6;
            16 + (r / 2) * 36 + (g / 2) * 6 + (b / 2)
        }
        8..=15 => n - 8,
        _ => 0,
    }
}

/// Named (16-color) dim neighbors: a small const mapping, no arithmetic.
fn scrim_dim_named(color: Color) -> Color {
    match color {
        Color::White | Color::Gray => Color::DarkGray,
        Color::DarkGray | Color::Black => Color::Black,
        Color::LightRed => Color::Red,
        Color::LightGreen => Color::Green,
        Color::LightYellow => Color::Yellow,
        Color::LightBlue => Color::Blue,
        Color::LightMagenta => Color::Magenta,
        Color::LightCyan => Color::Cyan,
        Color::Red | Color::Green | Color::Yellow | Color::Blue | Color::Magenta | Color::Cyan => {
            Color::DarkGray
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(theme: Option<&str>, no_color: bool, truecolor: bool, dumb: bool) -> ThemeEnv {
        ThemeEnv {
            tolkin_theme: theme.map(str::to_string),
            no_color,
            truecolor,
            dumb_terminal: dumb,
        }
    }

    #[test]
    fn scrim_dim_multiplies_rgb_and_stays_in_palette_ranges() {
        assert_eq!(
            scrim_dim(Color::Rgb(200, 100, 50), 0.5),
            Color::Rgb(100, 50, 25)
        );
        assert_eq!(scrim_dim(Color::Reset, 0.5), Color::Reset);
        assert_eq!(scrim_dim(Color::White, 0.5), Color::DarkGray);
        assert_eq!(scrim_dim(Color::LightCyan, 0.5), Color::Cyan);
        for n in 0..=255u8 {
            let Color::Indexed(d) = scrim_dim(Color::Indexed(n), 0.5) else {
                panic!("indexed must stay indexed");
            };
            match n {
                232..=255 => assert!((232..=255).contains(&d)),
                16..=231 => assert!((16..=231).contains(&d)),
                _ => assert!(d <= 7),
            }
        }
    }

    #[test]
    fn no_color_forces_mono_over_everything() {
        let t = select(
            &env(Some("tolkin-dark"), true, true, false),
            Some("terminal"),
        );
        assert_eq!(t.name, "mono");
        let t = select(&env(None, false, false, true), None);
        assert_eq!(t.name, "mono", "dumb terminal forces mono");
    }

    #[test]
    fn env_var_beats_config_beats_default() {
        let t = select(
            &env(Some("terminal"), false, true, false),
            Some("tolkin-light"),
        );
        assert_eq!(t.name, "terminal");
        let t = select(&env(None, false, true, false), Some("tolkin-light"));
        assert_eq!(t.name, "tolkin-light");
        let t = select(&env(None, false, true, false), None);
        assert_eq!(t.name, DEFAULT_THEME);
    }

    #[test]
    fn unknown_names_fall_through() {
        let t = select(&env(Some("nope"), false, true, false), Some("also-nope"));
        assert_eq!(t.name, DEFAULT_THEME);
    }

    #[test]
    fn truecolor_selects_rgb_and_otherwise_indexed() {
        let rgb = select(&env(None, false, true, false), None);
        assert!(matches!(rgb.accent, Color::Rgb(0x22, 0xD3, 0xEE)));
        let indexed = select(&env(None, false, false, false), None);
        assert!(matches!(indexed.accent, Color::Indexed(45)));
    }

    #[test]
    fn terminal_theme_respects_reset_surfaces() {
        let t = by_name("terminal", true).unwrap();
        assert!(matches!(t.bg, Color::Reset));
        assert!(matches!(t.surface, Color::Reset));
    }

    #[test]
    fn cycle_walks_all_four_and_respects_forced_mono() {
        let e = env(None, false, true, false);
        let mut name = DEFAULT_THEME.to_string();
        let mut seen = vec![name.clone()];
        for _ in 0..3 {
            name = cycle(&name, &e).name.to_string();
            seen.push(name.clone());
        }
        assert_eq!(seen, THEME_NAMES.to_vec());
        assert_eq!(
            cycle("tolkin-dark", &env(None, true, true, false)).name,
            "mono"
        );
    }
}
