use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde_json::json;

use crate::input;
use crate::tokenize::{self, Provider};

#[derive(Args, Debug)]
pub struct DriftArgs {
    /// File to analyze, or "-" for stdin. Defaults to stdin when omitted.
    pub file: Option<PathBuf>,

    /// Emit JSON instead of a human-readable report.
    #[arg(long)]
    pub json: bool,
}

/// Published multiplier band for the Opus 4.7/4.8 tokenizer relative to 4.6:
/// roughly 1.0x to 1.35x the tokens for the same input, heaviest on code and
/// non-English text. Anthropic's tokenizer is closed, so this band cannot be
/// measured offline; it is reported as published, never as a measurement.
const OPUS_47_MULTIPLIER: (f64, f64) = (1.0, 1.35);

pub fn run(args: DriftArgs) -> Result<()> {
    let text = input::read(args.file.as_deref())?;

    // cl100k_base is what the Anthropic provider counts with (proxy), and
    // o200k_base is what the OpenAi provider counts with, so both encodings
    // come straight from the existing tokenize plumbing.
    let cl100k = tokenize::count(Provider::Anthropic, &text)?;
    let o200k = tokenize::count(Provider::OpenAi, &text)?;
    let gemini = tokenize::count(Provider::Gemini, &text)?;

    let delta = delta_pct(cl100k, o200k);
    let (band_min, band_max) = opus_band(cl100k);

    if args.json {
        print_json(cl100k, o200k, delta, band_min, band_max, gemini)?;
    } else {
        print_report(cl100k, o200k, delta, band_min, band_max, gemini);
    }
    Ok(())
}

/// Percent change of `new` relative to `base`, rounded to one decimal place.
/// Returns None when `base` is zero (empty input) since the ratio is undefined.
fn delta_pct(base: usize, new: usize) -> Option<f64> {
    if base == 0 {
        return None;
    }
    let raw = (new as f64 - base as f64) / base as f64 * 100.0;
    Some((raw * 10.0).round() / 10.0)
}

/// Apply the published Opus 4.7 multiplier band to the local proxy count.
fn opus_band(proxy: usize) -> (usize, usize) {
    let min = (proxy as f64 * OPUS_47_MULTIPLIER.0).round() as usize;
    let max = (proxy as f64 * OPUS_47_MULTIPLIER.1).round() as usize;
    (min, max)
}

fn print_report(
    cl100k: usize,
    o200k: usize,
    delta: Option<f64>,
    band_min: usize,
    band_max: usize,
    gemini: usize,
) {
    println!("OpenAI encoding drift (exact, offline)");
    println!(
        "  {:<29}{} tokens",
        "cl100k_base (GPT-4 era)",
        commas(cl100k)
    );
    println!(
        "  {:<29}{} tokens",
        "o200k_base  (GPT-4o/5 era)",
        commas(o200k)
    );
    match delta {
        Some(pct) => {
            let direction = if pct < 0.0 {
                " (o200k is denser on this text)"
            } else if pct > 0.0 {
                " (o200k is heavier on this text)"
            } else {
                " (no change on this text)"
            };
            println!("  {:<29}{pct:+.1}%{direction}", "delta");
        }
        None => println!("  {:<29}n/a (empty input)", "delta"),
    }
    println!();
    println!("Claude tokenizer drift (published estimate, not measured)");
    println!(
        "  {:<32}~{} tokens",
        "local estimate (cl100k proxy)",
        commas(cl100k)
    );
    println!(
        "  {:<32}~{} tokens (proxy)",
        "Opus 4.6 baseline",
        commas(band_min)
    );
    println!(
        "  {:<32}~{} to ~{} tokens (published {:.1}x-{:.2}x multiplier)",
        "Opus 4.7/4.8 band",
        commas(band_min),
        commas(band_max),
        OPUS_47_MULTIPLIER.0,
        OPUS_47_MULTIPLIER.1
    );
    println!("  note: exact per-version counts require the Anthropic count_tokens verify");
    println!();
    println!("Gemini");
    println!(
        "  no drift: all current Gemini models share the Gemma vocabulary ({} tokens)",
        commas(gemini)
    );
    println!();
    println!("Notes");
    println!("  - Drift changes cost per character, not per token. Same prices, more tokens.");
    println!("  - Input-token figures only; output may vary.");
}

fn print_json(
    cl100k: usize,
    o200k: usize,
    delta: Option<f64>,
    band_min: usize,
    band_max: usize,
    gemini: usize,
) -> Result<()> {
    let out = json!({
        "openai": {
            "cl100k_base": cl100k,
            "o200k_base": o200k,
            "delta_pct": delta,
        },
        "anthropic": {
            "proxy_tokens": cl100k,
            "opus_47_band_min": band_min,
            "opus_47_band_max": band_max,
            "published_multiplier": [OPUS_47_MULTIPLIER.0, OPUS_47_MULTIPLIER.1],
            "measured": false,
        },
        "gemini": {
            "tokens": gemini,
            "drift": false,
        },
        "notes": [
            "Claude band is the published Opus 4.7 multiplier applied to a cl100k_base proxy, not a measurement.",
            "Exact Claude per-version counts require the Anthropic count_tokens verify.",
            "All current Gemini models share the Gemma vocabulary, so there is no Gemini drift.",
            "Drift changes cost per character, not per token. Same prices, more tokens.",
            "Input-token figures only; output may vary.",
        ],
    });
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}

fn commas(n: usize) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(char::from(*b));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_pct_rounds_to_one_decimal() {
        // (1201 - 1284) / 1284 = -6.464...% which rounds to -6.5.
        assert_eq!(delta_pct(1284, 1201), Some(-6.5));
        assert_eq!(delta_pct(100, 110), Some(10.0));
        assert_eq!(delta_pct(100, 100), Some(0.0));
    }

    #[test]
    fn delta_pct_empty_input_is_none() {
        assert_eq!(delta_pct(0, 0), None);
        assert_eq!(delta_pct(0, 5), None);
    }

    #[test]
    fn opus_band_applies_published_multiplier() {
        assert_eq!(opus_band(1284), (1284, 1733)); // 1284 * 1.35 = 1733.4
        assert_eq!(opus_band(0), (0, 0));
        assert_eq!(opus_band(100), (100, 135));
    }

    #[test]
    fn commas_groups_thousands() {
        assert_eq!(commas(0), "0");
        assert_eq!(commas(999), "999");
        assert_eq!(commas(1284), "1,284");
        assert_eq!(commas(1_000_000), "1,000,000");
    }
}
