use std::io::IsTerminal;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

mod advisories;
mod cache_analysis;
mod cli;
mod commands;
mod input;
mod ledger;
mod onboard;
mod parse;
mod project;
mod scan;
mod sidecar;
mod tiers;
mod tokenize;
mod tui;
mod update;
mod usage;
mod verify;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    // Auto-onboarding: triggers only on a TTY when no config exists and the
    // command is not Init/Version. Consent answers do not gate dispatch.
    if should_auto_onboard(cli.command.as_ref()) {
        let _ = onboard::run(false, cli.yes);
    }

    match cli.command {
        Some(cmd) => {
            // The update command already prints version state; no notice there.
            let suppress_notice = matches!(&cmd, cli::Commands::Update(_));
            match cli::dispatch(cmd, cli.yes) {
                Ok(()) => {
                    maybe_print_update_notice(suppress_notice);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::from(1)
                }
            }
        }
        None => run_bare(),
    }
}

/// Consent-gated, once-a-day update notice on stderr after a successful
/// command. Stderr only (JSON consumers read stdout), TTY only (scripts see
/// nothing), and never after the update command itself. All other gating
/// (consent, cadence, CI, env kills) lives in update::passive_refresh_and_notice.
fn maybe_print_update_notice(suppress: bool) {
    if suppress || !std::io::stderr().is_terminal() {
        return;
    }
    if let Some(line) = update::passive_refresh_and_notice() {
        eprintln!("{line}");
    }
}

/// `tolkin` with no subcommand: open the dashboard if both stdio handles are
/// TTYs, otherwise print the usage banner to stderr and exit 2. The non-TTY
/// path preserves the pre-existing script-facing contract (no subcommand =
/// usage error).
fn run_bare() -> ExitCode {
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        return match tui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("Error: {e}");
                ExitCode::from(1)
            }
        };
    }
    let _ = cli::Cli::command().print_help();
    eprintln!();
    eprintln!("Error: no subcommand provided. Run with --help for usage.");
    ExitCode::from(2)
}

fn should_auto_onboard(cmd: Option<&cli::Commands>) -> bool {
    if matches!(
        cmd,
        Some(cli::Commands::Init(_)) | Some(cli::Commands::Version)
    ) {
        return false;
    }
    if ledger::disabled_by_env() {
        return false;
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return false;
    }
    ledger::load_config().is_none()
}
