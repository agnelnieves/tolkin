//! `tolkin update`: explicit version check against the npm registry.
//!
//! Issues exactly one HTTPS GET to registry.npmjs.org/tolkin-cli/latest and
//! reports whether a newer version is available, with channel-aware upgrade
//! instructions. Explicit invocation is the consent mechanism: the user typed
//! the command, so they agreed to one outbound request.
//!
//! Privacy posture: no user data is sent. The request is a plain GET with no
//! authentication, no cookies, and no query parameters that could carry an
//! identity. Setting TOLKIN_NO_UPDATE_CHECK=1 suppresses even this.

use anyhow::{anyhow, Result};
use clap::Args;
use serde_json::json;

use crate::update;

#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Emit the result as JSON instead of human-readable output.
    #[arg(long)]
    pub json: bool,
}

pub fn run(args: UpdateArgs) -> Result<()> {
    let status = update::check_now().map_err(|e| anyhow!("{e}"))?;

    if args.json {
        let out = json!({
            "current": status.current,
            "latest": status.latest,
            "update_available": status.update_available,
            "channel": status.channel.as_str(),
            "instructions": status.instructions,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("current:  {}", status.current);
    println!("latest:   {}", status.latest);

    if status.update_available {
        println!("status:   update available");
    } else {
        println!("status:   up to date");
    }

    if status.channel == update::InstallChannel::Unknown {
        println!("channel:  unknown install method detected; showing all install channels below");
    }

    if status.update_available || status.channel == update::InstallChannel::Unknown {
        println!();
        println!("to upgrade:");
        for line in &status.instructions {
            println!("  {line}");
        }
    }

    Ok(())
}
