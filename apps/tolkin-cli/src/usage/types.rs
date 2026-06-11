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

/// One request's cache-relevant token counts, retained per session for the
/// prompt-cache analysis (`tolkin cache`): timestamp, fresh input, cache
/// read, and the 5m/1h cache-write split. Deliberately compact and exactly
/// this tuple; the privacy class is unchanged (timestamps and token counts
/// only, never content, never paths).
///
/// Field order is load-bearing: the derived `Ord` sorts by `ts` first, then
/// by the token fields, which is the deterministic within-session request
/// order the cache analysis depends on (two requests can share a timestamp,
/// and map-iteration order must never decide which one counts as "first").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct RequestUsage {
    /// Unix epoch seconds of the request's log record.
    pub ts: u64,
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_5m_tokens: u64,
    pub cache_write_1h_tokens: u64,
}

impl RequestUsage {
    /// Everything the provider counts on the input side of this request.
    pub fn input_side(&self) -> u64 {
        self.input_tokens
            + self.cache_read_tokens
            + self.cache_write_5m_tokens
            + self.cache_write_1h_tokens
    }

    /// Cache-write tokens (both TTLs) for this request.
    pub fn write_tokens(&self) -> u64 {
        self.cache_write_5m_tokens + self.cache_write_1h_tokens
    }
}

/// One agent session, aggregated from its log file.
///
/// # Subagent streams
///
/// Claude Code writes Task-tool subagent transcripts to a dedicated path:
/// `<projects_dir>/<slug>/<parent_session_id>/subagents/agent-<id>.jsonl`.
/// Each subagent file is its OWN prompt stream (its own system prompt and
/// growing prefix) and a single parent task can fan out many of them. The
/// reader emits one `SessionUsage` per subagent file so cache analysis sees
/// each as its own timeline; folding them into the parent's request vector
/// would interleave unrelated prefixes and corrupt churn / TTL math.
///
/// The distinction between parent sessions and subagent streams is carried
/// OUTSIDE this struct so the struct stays additive-free for downstream
/// modules that already construct it through struct literals. See
/// [`UsageData::subagent_session_keys`] for the side-table of subagent
/// stream identifiers and [`UsageData::is_subagent`] for the seam helper.
#[derive(Clone, Debug, Serialize)]
pub struct SessionUsage {
    pub source: UsageSource,
    /// Session log identity. For Claude Code parent sessions this is the
    /// jsonl file stem (a session uuid). For Claude Code subagent streams
    /// this is the subagent jsonl file stem (e.g. `agent-abc0120eb1e3db5f8`).
    /// For Codex it is the rollout id.
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
    /// Per-request cache tuples, sorted ascending by [`RequestUsage`] order,
    /// populated AFTER global cross-file dedup so resumed sessions never
    /// double count. Retained for Claude Code sessions and subagent streams
    /// (each subagent is its own prefix timeline); Codex rollouts carry no
    /// cache-write fields (OpenAI exposes neither write events nor a TTL
    /// choice), so cache analysis is Claude-Code-sourced for now and this
    /// vector stays empty for Codex.
    pub requests: Vec<RequestUsage>,
}

/// Everything ingestion produces for one machine scan.
///
/// The `sessions` vector is the unified working set every downstream surface
/// consumes (stats totals, cache analysis, the dashboard). For Claude Code
/// it carries BOTH parent session rows and subagent-stream rows because
/// every token entered must be measured.
///
/// The side-table [`Self::subagent_session_keys`] identifies which entries
/// are subagent streams (keyed by the `(session_id, project_key)` tuple
/// that uniquely identifies a SessionUsage). It exists so a surface that
/// wants the "spirit of a working session" headline can filter parents
/// only without forcing additive fields onto downstream consumers of
/// SessionUsage. The fields stay where they are; the discrimination lives
/// next to the collection it discriminates.
#[derive(Clone, Debug, Default, Serialize)]
pub struct UsageData {
    pub sessions: Vec<SessionUsage>,
    /// Lines that failed to parse across all files (lenient parsing).
    pub skipped_lines: u64,
    /// Log files that could not be read at all.
    pub skipped_files: u64,
    /// Set of `(session_id, project_key)` pairs that identify subagent
    /// stream entries in `sessions`. Empty for Codex-only data and for
    /// Claude Code data without any subagent transcripts. The pair is the
    /// same key the Claude Code reader uses to bucket records into
    /// SessionUsage rows; using it here keeps the side-table addressable
    /// directly without scanning the full sessions vector.
    pub subagent_session_keys: std::collections::BTreeSet<(String, String)>,
    /// Parent-session-id lookup keyed by the same `(session_id, project_key)`
    /// pair as `subagent_session_keys`. Populated only for subagent entries;
    /// the entry's value is the file stem of the parent jsonl session that
    /// fanned out the subagent. Surfaces can group fan-out spend back to
    /// the originating parent through this map.
    pub subagent_parent_ids: BTreeMap<(String, String), String>,
}

impl UsageData {
    /// Count of Claude Code parent session rows. The "working session"
    /// headline lives here: a task that fanned out twelve subagents stays
    /// one parent session in this count, while its subagent transcripts are
    /// reachable via [`Self::subagent_stream_count`]. Internal helper used by
    /// surfaces that want the disambiguated headline; not part of any
    /// documented JSON key by itself.
    #[allow(dead_code)] // public seam for surfaces, exercised by tests
    pub fn parent_session_count(&self) -> usize {
        self.sessions
            .iter()
            .filter(|s| !self.is_subagent(s))
            .count()
    }

    /// Count of Claude Code subagent-stream rows.
    #[allow(dead_code)] // public seam for surfaces, exercised by tests
    pub fn subagent_stream_count(&self) -> usize {
        self.subagent_session_keys.len()
    }

    /// True when this `SessionUsage` is a subagent stream entry. Looks up
    /// the side-table by `(session_id, project_key)`.
    #[allow(dead_code)] // public seam for surfaces, exercised by tests
    pub fn is_subagent(&self, session: &SessionUsage) -> bool {
        self.subagent_session_keys
            .contains(&(session.session_id.clone(), session.project_key.clone()))
    }
}
