//! Shared snapshot builder used by both `tolkin stats` (plain/json) and the
//! TUI dashboard. The job is one thing: read the ledger and config, optionally
//! read usage logs (consent + env permitting), and assemble the `TierInputs`
//! that `tiers::compute` consumes.
//!
//! Keeping this in one place ensures the dashboard sees exactly what the JSON
//! report sees, so the honesty design stays consistent across surfaces.

use std::env;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tolkin_core::pricing;
use tolkin_core::Provider;

use crate::advisories;
use crate::cache_analysis;
use crate::ledger::{self, Config, LedgerRecord};
use crate::tiers::{self, TierReport};
use crate::usage::{self, cost, types::UsageData};

/// Result of one snapshot read: everything a stats surface needs without
/// touching the env, the clock, or the filesystem again.
pub struct StatsSnapshot {
    /// The data dir the snapshot was loaded from. Stored for callers that need
    /// to write back (e.g. cache, ledger reset).
    #[allow(dead_code)]
    pub dir: PathBuf,
    pub config: Option<Config>,
    pub records: Vec<LedgerRecord>,
    pub skipped: u64,
    pub project_key: String,
    pub ingestion_on: bool,
    pub usage_data: Option<UsageData>,
    pub rate_model_id: &'static str,
    pub rate_model_display: &'static str,
    pub rate_usd_per_mtok_input: f64,
    pub assumed_rate: f64,
    pub now: u64,
}

impl StatsSnapshot {
    pub fn load() -> Option<StatsSnapshot> {
        let dir = ledger::data_dir()?;
        Some(Self::load_in(&dir))
    }

    /// Inner load. Targets an explicit data dir so the dashboard and tests can
    /// share the same code path under a tempdir.
    pub fn load_in(dir: &Path) -> StatsSnapshot {
        let config = ledger::load_config_from(dir);
        let (records, skipped) = ledger::read_records_in(dir);
        let project_key = canonical_cwd();

        let home = usage::home_root();
        let ingestion_on = config
            .as_ref()
            .is_some_and(|c| c.consent_log_ingestion && !ledger::disabled_by_env())
            && home.is_some();
        let usage_data: Option<UsageData> = if ingestion_on {
            home.map(|home| {
                let (claude, codex) = usage::default_dirs(&home);
                usage::cache::read_all_cached(Some(&claude), Some(&codex), dir)
            })
        } else {
            None
        };

        let assumed_rate = config
            .as_ref()
            .and_then(|c| c.session_rate_per_day)
            .unwrap_or(10.0);
        let rate_model = pricing::default_for(Provider::Anthropic);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        StatsSnapshot {
            dir: dir.to_path_buf(),
            config,
            records,
            skipped,
            project_key,
            ingestion_on,
            usage_data,
            rate_model_id: rate_model.id,
            rate_model_display: rate_model.display,
            rate_usd_per_mtok_input: rate_model.input,
            assumed_rate,
            now,
        }
    }

    /// Tier report scoped to the current project.
    pub fn compute_project(&self) -> TierReport {
        self.compute(Some(self.project_key.as_str()))
    }

    /// Tier report scoped globally (all projects in the ledger).
    pub fn compute_global(&self) -> TierReport {
        self.compute(None)
    }

    fn compute(&self, scope_project: Option<&str>) -> TierReport {
        tiers::compute(&tiers::TierInputs {
            scope_project,
            ledger: &self.records,
            sessions: self.usage_data.as_ref().map(|d| d.sessions.as_slice()),
            assumed_rate_per_day: self.assumed_rate,
            realized_usd_per_mtok: self.rate_usd_per_mtok_input,
            cost_fn: &cost::cost_usd,
            now: self.now,
        })
    }

    /// Cache health report for the given scope (`None` = global). `None` is
    /// returned exactly when ingestion is off (no usage data was read), so
    /// every surface gates the cache section on the same condition the
    /// measured tier already uses.
    pub fn compute_cache(
        &self,
        scope_project: Option<&str>,
    ) -> Option<cache_analysis::CacheReport> {
        let usage = self.usage_data.as_ref()?;
        Some(cache_analysis::analyze(&cache_analysis::CacheInputs {
            scope_project,
            sessions: &usage.sessions,
            rate_fn: &cost::cache_rates,
        }))
    }

    /// Advisory block for the given scope. `None` is returned when the
    /// measured tier is absent (ingestion off), mirroring the cache gate.
    /// Computed in the shared snapshot path so stats/TUI/report all agree.
    pub fn compute_advisories(&self, report: &TierReport) -> Option<advisories::AdvisoryBlock> {
        let measured = report.measured.as_ref()?;
        let sessions = self
            .usage_data
            .as_ref()
            .map(|d| d.sessions.as_slice())
            .unwrap_or(&[]);
        let monthly_cap_usd = self.config.as_ref().and_then(|c| c.monthly_cap_usd);
        Some(advisories::compute(&advisories::AdvisoryInputs {
            measured,
            sessions,
            cost_fn: &cost::cost_usd,
            input_rate_fn: &cost::input_rate_usd_per_mtok,
            monthly_cap_usd,
            now: self.now,
        }))
    }
}

/// Canonicalized cwd, matching how `ledger::append` records project keys.
pub fn canonical_cwd() -> String {
    let cwd = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    cwd.canonicalize()
        .unwrap_or(cwd)
        .to_string_lossy()
        .into_owned()
}
