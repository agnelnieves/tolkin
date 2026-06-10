use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use tolkin_core::redact::{self, RedactOptions};

use crate::input;

#[derive(Args, Debug)]
pub struct RedactArgs {
    /// File to redact, or "-" for stdin. Defaults to stdin when omitted.
    pub file: Option<PathBuf>,

    /// Only redact high-confidence matches (fewer, higher-precision redactions).
    #[arg(long)]
    pub strict: bool,

    /// Catalog kind to leave untouched (repeatable), e.g. --allow high-entropy.
    #[arg(long = "allow", value_name = "KIND")]
    pub allow: Vec<String>,

    /// Emit JSON (redacted text + full ledger) instead of plain redacted text.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: RedactArgs) -> Result<()> {
    let text = input::read(args.file.as_deref())?;
    let opts = RedactOptions {
        strict: args.strict,
        allow: args.allow,
    };
    let result = redact::redact(&text, &opts);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    // Redacted text on stdout so it stays pipeable; the ledger goes to stderr.
    println!("{}", result.redacted_text);
    eprintln!(
        "\nredacted {} secret(s), {} flagged for review",
        result.redacted_count, result.review_count
    );
    for f in &result.findings {
        let tag = if f.redacted { "redacted" } else { "review  " };
        eprintln!(
            "  [{tag}] {:<16} conf {:.2}  bytes {}..{}",
            f.kind, f.confidence, f.start, f.end
        );
    }
    Ok(())
}
