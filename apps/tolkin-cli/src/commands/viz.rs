use std::io::IsTerminal;
use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::input;
use crate::tokenize::{self, Provider, Segment};

#[derive(Args, Debug)]
pub struct VizArgs {
    /// File to tokenize, or "-" for stdin. Defaults to stdin when omitted.
    pub file: Option<PathBuf>,

    /// Which provider's tokenizer to use.
    #[arg(long, default_value = "openai")]
    pub model: Provider,

    /// Cap how many token chips are rendered. Excess is summarized in a footer.
    #[arg(long, default_value_t = 2000)]
    pub max_tokens: usize,

    /// Emit JSON instead of a rendered view.
    #[arg(long)]
    pub json: bool,
}

/// ANSI 256-color background codes cycled across adjacent tokens so boundaries
/// read clearly. Foreground is forced to black for contrast on these pastels.
const PALETTE: [u8; 6] = [153, 151, 223, 218, 159, 195];

pub fn run(args: VizArgs) -> Result<()> {
    let text = input::read(args.file.as_deref())?;
    let encoding = encoding_label(args.model);

    if args.json {
        return run_json(args.model, &text, encoding);
    }

    if args.model == Provider::Anthropic {
        let count = tokenize::count(args.model, &text)?;
        println!(
            "~{} tokens (estimate, cl100k_base proxy, +/- 10%). Exact counts arrive with the Phase 2 hybrid count_tokens API.",
            group_thousands(count)
        );
        return Ok(());
    }

    let segments = tokenize::segments(args.model, &text)?;
    let total = segments.len();
    let color = use_color();
    let shown = total.min(args.max_tokens);

    let mut rendered = String::new();
    for (i, seg) in segments.iter().take(shown).enumerate() {
        if color {
            rendered.push_str(&color_chip(&seg.text, i));
        } else {
            rendered.push_str(&plain_chip(&seg.text));
        }
    }
    print!("{rendered}");
    if !rendered.ends_with('\n') {
        println!();
    }

    if total > shown {
        println!("... (+{} more tokens)", group_thousands(total - shown));
    }
    println!(
        "{} tokens | {} | {}",
        group_thousands(total),
        args.model,
        encoding
    );
    Ok(())
}

/// Render whitespace visibly so token boundaries around spaces and newlines are
/// legible: a space becomes a middot, a newline becomes a marker plus a real
/// break. Everything inside one token shares a single background color.
fn color_chip(text: &str, idx: usize) -> String {
    let bg = PALETTE[idx % PALETTE.len()];
    let mut body = String::new();
    let mut trailing_break = false;
    for ch in text.chars() {
        match ch {
            ' ' => body.push('\u{00b7}'),
            '\t' => body.push('\u{2192}'),
            '\n' => {
                body.push('\u{21b5}');
                trailing_break = true;
            }
            '\r' => {}
            other => body.push(other),
        }
    }
    // Color the visible glyphs, reset, then emit the real line break (outside
    // the colored span) so the chip color does not bleed across the row.
    let mut out = format!("\u{1b}[48;5;{bg}m\u{1b}[30m{body}\u{1b}[0m");
    if trailing_break {
        out.push('\n');
    }
    out
}

fn plain_chip(text: &str) -> String {
    let mut body = String::new();
    for ch in text.chars() {
        match ch {
            ' ' => body.push('\u{00b7}'),
            '\t' => body.push('\u{2192}'),
            '\n' => body.push('\u{21b5}'),
            '\r' => {}
            other => body.push(other),
        }
    }
    format!("\u{27e6}{body}\u{27e7}")
}

fn run_json(provider: Provider, text: &str, encoding: &str) -> Result<()> {
    let exact = !provider.is_estimate();
    if provider == Provider::Anthropic {
        let count = tokenize::count(provider, text)?;
        println!(
            "{{\"provider\":\"{}\",\"exact\":{},\"tokens\":{},\"encoding\":\"{}\",\"segments\":[],\"note\":\"per-token boundaries are intentionally not shown for Anthropic; cl100k_base is only a proxy for Claude\"}}",
            provider.slug(),
            exact,
            count,
            encoding
        );
        return Ok(());
    }

    let segments = tokenize::segments(provider, text)?;
    let mut out = format!(
        "{{\"provider\":\"{}\",\"exact\":{},\"tokens\":{},\"encoding\":\"{}\",\"segments\":[",
        provider.slug(),
        exact,
        segments.len(),
        encoding
    );
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&segment_json(seg));
    }
    out.push_str("]}");
    println!("{out}");
    Ok(())
}

fn segment_json(seg: &Segment) -> String {
    format!(
        "{{\"id\":{},\"text\":{},\"start\":{},\"end\":{}}}",
        seg.id,
        json_string(&seg.text),
        seg.start,
        seg.end
    )
}

/// Minimal JSON string escaping (no serde dependency in the CLI surface).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn encoding_label(provider: Provider) -> &'static str {
    match provider {
        Provider::OpenAi => "o200k_base",
        Provider::Gemini => "gemma",
        Provider::Anthropic => "cl100k_base (proxy)",
    }
}

/// Color only when stdout is a real terminal and NO_COLOR is unset.
fn use_color() -> bool {
    std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
}

/// Group an integer with comma thousands separators (no locale dependency).
fn group_thousands(n: usize) -> String {
    let digits = n.to_string();
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let first = bytes.len() % 3;
    for (i, b) in bytes.iter().enumerate() {
        if i != 0 && i.checked_sub(first).is_some_and(|d| d % 3 == 0) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thousands_grouping() {
        assert_eq!(group_thousands(0), "0");
        assert_eq!(group_thousands(42), "42");
        assert_eq!(group_thousands(1234), "1,234");
        assert_eq!(group_thousands(1000000), "1,000,000");
    }

    #[test]
    fn json_escapes_control_chars() {
        assert_eq!(json_string("a\"b"), r#""a\"b""#);
        assert_eq!(json_string("a\nb"), r#""a\nb""#);
        assert_eq!(json_string("tab\there"), r#""tab\there""#);
    }

    #[test]
    fn plain_chip_marks_whitespace() {
        let c = plain_chip(" \n");
        assert!(c.contains('\u{00b7}'));
        assert!(c.contains('\u{21b5}'));
        assert!(c.starts_with('\u{27e6}'));
        assert!(c.ends_with('\u{27e7}'));
    }
}
