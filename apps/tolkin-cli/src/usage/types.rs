//! Shared types for local usage-log ingestion.
//!
//! Privacy posture: ingestion reads local agent session logs (Claude Code,
//! Codex) strictly read-only, keeps only token counts and timestamps, and
//! never touches message content. Nothing leaves the machine.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    ClaudeCode,
    Codex,
}

impl UsageSource {
    #[allow(dead_code)] // consumed by the I3 dashboard source labels
    pub fn label(&self) -> &'static str {
        match self {
            UsageSource::ClaudeCode => "Claude Code",
            UsageSource::Codex => "Codex",
        }
    }
}

/// Token totals split the way providers price them. Cache writes keep the
/// 5m/1h split because Anthropic prices the TTLs differently.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct UsageTotals {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_5m_tokens: u64,
    pub cache_write_1h_tokens: u64,
}

impl UsageTotals {
    pub fn add(&mut self, other: &UsageTotals) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_5m_tokens += other.cache_write_5m_tokens;
        self.cache_write_1h_tokens += other.cache_write_1h_tokens;
    }

    /// Everything the provider counts on the input side of a request.
    pub fn input_side(&self) -> u64 {
        self.input_tokens
            + self.cache_read_tokens
            + self.cache_write_5m_tokens
            + self.cache_write_1h_tokens
    }

    pub fn is_empty(&self) -> bool {
        self.input_side() == 0 && self.output_tokens == 0
    }
}

/// One agent session, aggregated from its log file.
#[derive(Clone, Debug, Serialize)]
pub struct SessionUsage {
    pub source: UsageSource,
    /// Session log identity (file stem for Claude Code, rollout id for Codex).
    pub session_id: String,
    /// Raw cwd string recorded in the log. Matched against ledger
    /// project_key values by string comparison; log cwds may reference
    /// directories that no longer exist, so no canonicalization here.
    pub project_key: String,
    /// Unix epoch seconds of the first and last usage-bearing records.
    pub first_ts: u64,
    pub last_ts: u64,
    pub totals: UsageTotals,
    /// Keyed by the RAW model id as it appears in the log (normalization for
    /// pricing happens at cost time, so unpriced ids stay visible).
    pub by_model: BTreeMap<String, UsageTotals>,
    /// Per-UTC-day ("YYYY-MM-DD") split, for spend trends.
    pub by_day: BTreeMap<String, UsageTotals>,
}

/// Everything ingestion produces for one machine scan.
#[derive(Clone, Debug, Default, Serialize)]
pub struct UsageData {
    pub sessions: Vec<SessionUsage>,
    /// Lines that failed to parse across all files (lenient parsing).
    pub skipped_lines: u64,
    /// Log files that could not be read at all.
    pub skipped_files: u64,
}
