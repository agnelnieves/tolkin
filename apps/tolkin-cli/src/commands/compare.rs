use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::input;
use crate::tokenize::{self, Provider};

#[derive(Args, Debug)]
pub struct CompareArgs {
    /// File to tokenize, or "-" for stdin. Defaults to stdin when omitted.
    pub file: Option<PathBuf>,

    /// Emit JSON instead of a human-readable table.
    #[arg(long)]
    pub json: bool,
}

struct Row {
    provider: Provider,
    tokens: usize,
}

pub fn run(args: CompareArgs) -> Result<()> {
    let text = input::read(args.file.as_deref())?;
    let chars = text.chars().count();
    let bytes = text.len();

    let rows = Provider::ALL
        .iter()
        .map(|p| {
            Ok::<_, anyhow::Error>(Row {
                provider: *p,
                tokens: tokenize::count(*p, &text)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    if args.json {
        print_json(&rows, chars, bytes);
    } else {
        print_table(&rows, chars, bytes);
    }
    Ok(())
}

fn print_table(rows: &[Row], chars: usize, bytes: usize) {
    println!("{:<12} {:<8} {:<8} Bytes", "Provider", "Tokens", "Chars");
    for row in rows {
        let tokens = if row.provider.is_estimate() {
            format!("~{}", row.tokens)
        } else {
            row.tokens.to_string()
        };
        println!(
            "{:<12} {:<8} {:<8} {:<8}",
            row.provider.to_string(),
            tokens,
            chars,
            bytes
        );
    }
}

fn print_json(rows: &[Row], chars: usize, bytes: usize) {
    let mut out = String::from("{\"chars\":");
    out.push_str(&chars.to_string());
    out.push_str(",\"bytes\":");
    out.push_str(&bytes.to_string());
    out.push_str(",\"providers\":{");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "\"{}\":{{\"tokens\":{},\"estimate\":{}}}",
            row.provider.slug(),
            row.tokens,
            row.provider.is_estimate()
        ));
    }
    out.push_str("},\"note\":\"anthropic is approximation, ~10% error\"}");
    println!("{out}");
}
