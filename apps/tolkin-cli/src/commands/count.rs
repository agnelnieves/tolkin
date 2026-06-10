use std::path::PathBuf;

use anyhow::{anyhow, Result};
use clap::Args;
use tolkin_core::redact::{self, RedactOptions};

use crate::input;
use crate::tokenize::{self, Provider};
use crate::verify;

#[derive(Args, Debug)]
pub struct CountArgs {
    /// File to tokenize, or "-" for stdin. Defaults to stdin when omitted.
    pub file: Option<PathBuf>,

    /// Which provider's tokenizer to use.
    #[arg(long, default_value = "openai")]
    pub model: Provider,

    /// Show counts for all providers in a table.
    #[arg(long)]
    pub all: bool,

    /// Verify the Anthropic count via the count_tokens API.
    /// Requires ANTHROPIC_API_KEY; sends redacted text to api.anthropic.com.
    #[arg(long)]
    pub verify: bool,

    /// Model id to verify against (used with --verify).
    #[arg(long = "verify-model", default_value = "claude-sonnet-4-6")]
    pub verify_model: String,

    /// Emit JSON instead of human-readable output.
    #[arg(long)]
    pub json: bool,
}

struct Verified {
    tokens: u64,
    model: String,
}

pub fn run(args: CountArgs) -> Result<()> {
    let text = input::read(args.file.as_deref())?;

    let verified = if args.verify {
        Some(run_verify(&text, &args.verify_model)?)
    } else {
        None
    };

    if args.all {
        let counts = Provider::ALL
            .iter()
            .map(|p| Ok::<_, anyhow::Error>((*p, tokenize::count(*p, &text)?)))
            .collect::<Result<Vec<_>>>()?;

        if args.json {
            print_all_json(&counts, verified.as_ref());
        } else {
            print_all_table(&counts, verified.as_ref());
        }
        return Ok(());
    }

    let count = tokenize::count(args.model, &text)?;
    if args.json {
        let mut out = format!(
            "{{\"provider\":\"{}\",\"tokens\":{}",
            args.model.slug(),
            count
        );
        if let Some(v) = &verified {
            out.push_str(&format!(
                ",\"anthropic_verified\":{{\"tokens\":{},\"model\":\"{}\"}}",
                v.tokens, v.model
            ));
        }
        out.push('}');
        println!("{out}");
    } else if let Some(v) = &verified {
        if args.model == Provider::Anthropic {
            // The verified count replaces the local estimate on stdout; the
            // estimate moves to stderr for context.
            println!("{}", v.tokens);
            eprintln!(
                "verified via count_tokens ({}); local estimate was ~{count}",
                v.model
            );
        } else {
            println!("{count}");
            eprintln!("anthropic verified ({}): {} tokens", v.model, v.tokens);
        }
    } else {
        println!("{count}");
    }
    Ok(())
}

/// Run the opt-in count_tokens verification. Privacy rule: the core redactor
/// runs over the text BEFORE anything leaves the machine; only the redacted
/// text is sent.
fn run_verify(text: &str, model: &str) -> Result<Verified> {
    let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
        anyhow!(
            "--verify requires the ANTHROPIC_API_KEY environment variable; \
             set it to your Anthropic API key (sent only to api.anthropic.com)"
        )
    })?;

    let redaction = redact::redact(text, &RedactOptions::default());
    if redaction.redacted_count > 0 {
        eprintln!(
            "note: redacted text sent to api.anthropic.com ({} findings redacted)",
            redaction.redacted_count
        );
    } else {
        eprintln!("note: redacted text sent to api.anthropic.com (no secrets found)");
    }

    let tokens = verify::anthropic_count(&redaction.redacted_text, &api_key, model)?;
    Ok(Verified {
        tokens,
        model: model.to_string(),
    })
}

fn print_all_table(counts: &[(Provider, usize)], verified: Option<&Verified>) {
    println!("{:<12} Tokens", "Provider");
    for (provider, n) in counts {
        let label = match (provider, verified) {
            (Provider::Anthropic, Some(v)) => format!("{}  (verified, {})", v.tokens, v.model),
            _ if provider.is_estimate() => format!("~{n}  (estimate, +/-10%)"),
            _ => n.to_string(),
        };
        println!("{:<12} {}", provider.to_string(), label);
    }
}

fn print_all_json(counts: &[(Provider, usize)], verified: Option<&Verified>) {
    let mut out = String::from("{");
    for (i, (provider, n)) in counts.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("\"{}\":{}", provider.slug(), n));
    }
    if let Some(v) = verified {
        out.push_str(&format!(
            ",\"anthropic_verified\":{{\"tokens\":{},\"model\":\"{}\"}}",
            v.tokens, v.model
        ));
    }
    out.push_str(",\"note\":\"anthropic is approximation, ~10% error\"}");
    println!("{out}");
}
