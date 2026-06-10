use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use clap::Args;
use tolkin_core::cost::{self, CacheTtl, CostRequest};
use tolkin_core::pricing::{self, ModelPrice};
use tolkin_core::Provider as CoreProvider;

use crate::input;
use crate::tokenize::{self, Provider as TokProvider};

#[derive(Args, Debug)]
pub struct CostArgs {
    /// File to price, or "-" for stdin. Defaults to stdin when omitted.
    pub file: Option<PathBuf>,

    /// Model id (e.g. claude-sonnet-4.6). Omit to compare each provider default.
    #[arg(long)]
    pub model: Option<String>,

    /// Output tokens. Omit to estimate from the provider output:input ratio.
    #[arg(long)]
    pub output_tokens: Option<u64>,

    /// Number of calls to scale the total by (monthly volume, etc).
    #[arg(long, default_value_t = 1)]
    pub calls: u64,

    /// Fraction of input served from cache, 0..1.
    #[arg(long, default_value_t = 0.0)]
    pub cache_hit_rate: f64,

    /// Cache TTL for cache-write pricing: 5m or 1h.
    #[arg(long, default_value = "5m")]
    pub cache_ttl: String,

    /// Tokens written to cache on a cold turn (Anthropic cache-write pricing).
    #[arg(long, default_value_t = 0)]
    pub cache_write_tokens: u64,

    /// Fraction of input+output eligible for the Batch API 50% discount, 0..1.
    #[arg(long, default_value_t = 0.0)]
    pub batch_fraction: f64,

    /// Emit JSON instead of a table.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: CostArgs) -> Result<()> {
    let text = input::read(args.file.as_deref())?;

    let ttl = match args.cache_ttl.as_str() {
        "5m" => CacheTtl::FiveMin,
        "1h" => CacheTtl::OneHour,
        other => bail!("--cache-ttl must be 5m or 1h (got {other})"),
    };

    let models: Vec<&ModelPrice> = match &args.model {
        Some(id) => vec![pricing::find(id)
            .ok_or_else(|| anyhow!("unknown model: {id}. Run `tolkin cost --help` for ids."))?],
        None => [
            CoreProvider::OpenAi,
            CoreProvider::Anthropic,
            CoreProvider::Gemini,
        ]
        .iter()
        .map(|p| pricing::default_for(*p))
        .collect(),
    };

    let mut breakdowns = Vec::with_capacity(models.len());
    for m in models {
        let input_tokens = tokenize::count(tok_provider(m.provider), &text)? as u64;
        let req = CostRequest {
            model_id: m.id.to_string(),
            input_tokens,
            output_tokens: args.output_tokens,
            calls: args.calls.max(1),
            cache_hit_rate: args.cache_hit_rate,
            cache_write_tokens: args.cache_write_tokens,
            cache_ttl: ttl,
            batch_fraction: args.batch_fraction,
            long_context: false,
        };
        breakdowns.push(cost::estimate_with(m, &req));
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&breakdowns)?);
    } else {
        print_table(&breakdowns);
    }
    Ok(())
}

fn tok_provider(p: CoreProvider) -> TokProvider {
    match p {
        CoreProvider::OpenAi => TokProvider::OpenAi,
        CoreProvider::Anthropic => TokProvider::Anthropic,
        CoreProvider::Gemini => TokProvider::Gemini,
    }
}

fn print_table(rows: &[cost::CostBreakdown]) {
    let calls = rows.first().map_or(1, |r| r.calls);
    println!(
        "{:<22} {:>9} {:>9} {:>12} {:>12}",
        "Model", "Input", "Output", "$/call", "$ total"
    );
    for r in rows {
        let est = matches!(r.provider, CoreProvider::Anthropic);
        let input = decorate(r.input_tokens, est);
        let output = decorate(r.output_tokens, est || r.output_estimated);
        println!(
            "{:<22} {:>9} {:>9} {:>12} {:>12}",
            r.display,
            input,
            output,
            usd(r.per_call.total),
            usd(r.total.total),
        );
    }
    println!();
    if calls > 1 {
        println!("Totals are for {} calls.", commas(calls));
    }
    println!(
        "~ marks an estimate (Anthropic token count is a cl100k_base proxy; output is estimated unless set)."
    );
    println!("Input-token-bounded. Output cost varies by response.");
    println!(
        "Prices observed {}; verify against provider pricing pages before relying on dollar figures.",
        pricing::PRICES_OBSERVED
    );
}

fn decorate(n: u64, estimate: bool) -> String {
    let s = commas(n);
    if estimate {
        format!("~{s}")
    } else {
        s
    }
}

fn usd(v: f64) -> String {
    if v == 0.0 {
        "$0".to_string()
    } else if v < 0.01 {
        format!("${v:.5}")
    } else if v < 1.0 {
        format!("${v:.4}")
    } else {
        format!("${v:.2}")
    }
}

fn commas(n: u64) -> String {
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
