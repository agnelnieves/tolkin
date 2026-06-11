use anyhow::Result;
use clap::{Parser, Subcommand};

use crate::commands;

#[derive(Parser, Debug)]
#[command(
    name = "tolkin",
    version,
    about = "Tolkin CLI: privacy-first AI token analyzer",
    long_about = None,
)]
pub struct Cli {
    /// When absent in a TTY, tolkin opens the dashboard; non-TTY prints help
    /// to stderr and exits 2 so scripts and agents see unchanged behavior.
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Accept onboarding defaults without prompting.
    #[arg(
        long,
        global = true,
        help = "Accept onboarding defaults without prompting"
    )]
    pub yes: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Count tokens in a file or stdin.
    Count(commands::count::CountArgs),
    /// Visualize token distribution for a file or stdin.
    Viz(commands::viz::VizArgs),
    /// Audit a file for token waste: ranked findings with savings estimates.
    Audit(commands::audit::AuditArgs),
    /// Analyze an MCP config: tool-definition token cost and CLI-swap savings.
    Mcp(commands::mcp::McpArgs),
    /// Estimate provider cost for a file or stdin.
    Cost(commands::cost::CostArgs),
    /// Redact secrets in a file or stdin (runs before anything else).
    Redact(commands::redact::RedactArgs),
    /// Scan local agent configs (MCP, instruction files, shell) for token waste.
    Scan(commands::scan::ScanArgs),
    /// Audit a repository's agent-context token footprint by load profile.
    Project(commands::project::ProjectArgs),
    /// Compare the same input across tokenizer versions (encoding drift).
    Drift(commands::drift::DriftArgs),
    /// Compare token counts across providers.
    Compare(commands::compare::CompareArgs),
    /// Run the first-use preflight and consent flow.
    Init(commands::init::InitArgs),
    /// Show the local savings ledger; full tiered stats arrive with usage ingestion.
    Stats(commands::stats::StatsArgs),
    /// Measured prompt-cache health from local session logs: hit rate, write churn, TTL economics.
    Cache(commands::cache::CacheArgs),
    /// Render a self-contained HTML savings report for stakeholder sharing.
    Report(commands::report::ReportArgs),
    /// Deterministic optimization summary, plus an opt-in local-model advisory (narration, skill lint).
    Optimize(commands::optimize::OptimizeArgs),
    /// Check for a newer tolkin release (one explicit HTTPS request to the npm registry).
    Update(commands::update::UpdateArgs),
    /// Print the underlying tolkin-core version.
    Version,
}

pub fn dispatch(cmd: Commands, yes: bool) -> Result<()> {
    match cmd {
        Commands::Count(args) => commands::count::run(args),
        Commands::Viz(args) => commands::viz::run(args),
        Commands::Audit(args) => commands::audit::run(args),
        Commands::Mcp(args) => commands::mcp::run(args, yes),
        Commands::Cost(args) => commands::cost::run(args),
        Commands::Redact(args) => commands::redact::run(args),
        Commands::Scan(args) => commands::scan::run(args),
        Commands::Project(args) => commands::project::run(args),
        Commands::Drift(args) => commands::drift::run(args),
        Commands::Compare(args) => commands::compare::run(args),
        Commands::Init(args) => commands::init::run(args, yes),
        Commands::Stats(args) => commands::stats::run(args),
        Commands::Cache(args) => commands::cache::run(args),
        Commands::Report(args) => commands::report::run(args),
        Commands::Optimize(args) => commands::optimize::run(args, yes),
        Commands::Update(args) => commands::update::run(args),
        Commands::Version => {
            println!("tolkin-core {}", tolkin_core::version());
            Ok(())
        }
    }
}
