//! Local savings ledger. Append-only JSONL plus a small TOML config.
//!
//! Privacy posture: this is the only module in the CLI that persists state
//! between runs. Records store headline counts only (never file contents and
//! never secret values), writes happen exclusively after recorded consent, and
//! every operation is silently disabled when CI=true or TOLKIN_NO_LEDGER is
//! set (TOKLER_NO_LEDGER, the pre-rename name, still works). Nothing here ever touches the network.
//!
//! Timestamps are unix epoch seconds (u64). Human formatting happens at
//! display time so the ledger stays free of any date crate.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// On-disk version for `config.toml`. Bump when the schema changes.
const CONFIG_VERSION: u32 = 1;

/// On-disk version for each `ledger.jsonl` record. Bump when fields change.
const RECORD_VERSION: u32 = 2;

const LEDGER_FILE: &str = "ledger.jsonl";
const CONFIG_FILE: &str = "config.toml";

/// Resolved data directory for the ledger and config. The TOLKIN_DATA_DIR
/// override exists for tests and CI acceptance runs; everything else falls
/// back to the platform data directory via the `directories` crate.
pub fn data_dir() -> Option<PathBuf> {
    for var in ["TOLKIN_DATA_DIR", "TOKLER_DATA_DIR"] {
        if let Ok(val) = std::env::var(var) {
            if !val.is_empty() {
                return Some(PathBuf::from(val));
            }
        }
    }
    let dir = directories::ProjectDirs::from("", "", "tolkin").map(|p| p.data_dir().to_path_buf());
    if let Some(new_dir) = &dir {
        migrate_pre_rename_dir(new_dir);
    }
    dir
}

/// One-time migration from the pre-rename data dir ("tokler"). Moves the old
/// directory into place when the new one does not exist yet; any failure is
/// silent and simply means a fresh start (the old data stays put).
fn migrate_pre_rename_dir(new_dir: &Path) {
    if new_dir.exists() {
        return;
    }
    let Some(old) = directories::ProjectDirs::from("", "", "tokler") else {
        return;
    };
    let old_dir = old.data_dir();
    if !old_dir.exists() {
        return;
    }
    if let Some(parent) = new_dir.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::rename(old_dir, new_dir);
}

/// CI=true and TOLKIN_NO_LEDGER=1 (or any truthy value) disable all writes.
/// "0" and "false" are explicitly treated as not-set.
pub fn disabled_by_env() -> bool {
    is_truthy_env("CI") || is_truthy_env("TOLKIN_NO_LEDGER") || is_truthy_env("TOKLER_NO_LEDGER")
}

fn is_truthy_env(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let v = v.trim();
            !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
        }
        Err(_) => false,
    }
}

/// Consent and onboarding state. Lives next to the ledger as `config.toml`.
///
/// # Optional fields
///
/// - `session_rate_per_day`: user-supplied sessions-per-day rate for the
///   realized-savings formula when log ingestion is off. Absent by default.
///
/// - `monthly_cap_usd`: optional monthly spend cap in US dollars. When set
///   and measured usage data exists, `tolkin stats` and the report show how
///   much of the cap has been used this calendar month (UTC) and project when
///   the cap will be reached at the trailing 7-day and 30-day average daily
///   rates. Absent by default; omitting it from config.toml disables the cap
///   runway advisory entirely (no prompting when unset). Set this to the
///   Claude monthly limit you are working within, for example
///   `monthly_cap_usd = 50.0`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub v: u32,
    pub consent_ledger: bool,
    pub consent_log_ingestion: bool,
    pub onboarded_at: u64,
    /// Reserved for the next phase: a user-supplied sessions-per-day rate for
    /// the realized-savings formula when log ingestion is off.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub session_rate_per_day: Option<f64>,
    /// Optional monthly spend cap in USD. When set and measured data exists,
    /// the stats and report surfaces show month-to-date spend and project when
    /// the cap will be reached. Absent by default; unset means no advisory.
    /// Forward and backward compatible: older binaries round-trip the value
    /// via the `default` attribute (reads None if absent from the file).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub monthly_cap_usd: Option<f64>,
}

impl Config {
    pub fn new(consent_ledger: bool, consent_log_ingestion: bool) -> Self {
        Self {
            v: CONFIG_VERSION,
            consent_ledger,
            consent_log_ingestion,
            onboarded_at: now_secs(),
            session_rate_per_day: None,
            monthly_cap_usd: None,
        }
    }
}

/// Load the config from the resolved data dir, if present.
pub fn load_config() -> Option<Config> {
    let dir = data_dir()?;
    load_config_from(&dir)
}

/// Save the config to the resolved data dir, creating the dir if needed.
pub fn save_config(cfg: &Config) -> Result<()> {
    let dir = data_dir().ok_or_else(|| anyhow::anyhow!("no data directory available"))?;
    save_config_to(&dir, cfg)
}

/// Append a ledger record. Silent no-op unless: env does not disable writes,
/// a config exists, and `config.consent_ledger` is true. All failures are
/// swallowed so a ledger problem never disrupts a command.
pub fn append(command: &str, project_key: &Path, headline: Value) {
    if disabled_by_env() {
        return;
    }
    let Some(dir) = data_dir() else { return };
    let Some(cfg) = load_config_from(&dir) else {
        return;
    };
    if !cfg.consent_ledger {
        return;
    }
    let _ = append_in(&dir, command, project_key, headline);
}

/// Inner append. Exposed so unit tests can target a tempdir directly.
pub fn append_in(dir: &Path, command: &str, project_key: &Path, headline: Value) -> Result<()> {
    fs::create_dir_all(dir)?;
    let canonical = project_key
        .canonicalize()
        .unwrap_or_else(|_| project_key.to_path_buf());
    let record = json!({
        "v": RECORD_VERSION,
        "ts": now_secs(),
        "command": command,
        "project_key": canonical.to_string_lossy(),
        "headline": headline,
        "tolkin_version": env!("CARGO_PKG_VERSION"),
        "prices_observed": tolkin_core::pricing::PRICES_OBSERVED,
    });
    let mut line = serde_json::to_string(&record)?;
    line.push('\n');
    let path = dir.join(LEDGER_FILE);
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(line.as_bytes())?;
    Ok(())
}

/// Inner config load. Exposed so unit tests can target a tempdir directly.
pub fn load_config_from(dir: &Path) -> Option<Config> {
    let text = fs::read_to_string(dir.join(CONFIG_FILE)).ok()?;
    toml::from_str(&text).ok()
}

/// Inner config save. Exposed so unit tests can target a tempdir directly.
pub fn save_config_to(dir: &Path, cfg: &Config) -> Result<()> {
    fs::create_dir_all(dir)?;
    let text = toml::to_string(cfg)?;
    fs::write(dir.join(CONFIG_FILE), text)?;
    Ok(())
}

/// Resolved ledger file path inside `dir`.
pub fn ledger_path(dir: &Path) -> PathBuf {
    dir.join(LEDGER_FILE)
}

/// One parsed ledger record. `headline` stays schemaless (each command writes
/// its own shape); consumers pull the fields they know.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct LedgerRecord {
    pub v: u32,
    pub ts: u64,
    pub command: String,
    pub project_key: String,
    pub headline: Value,
    #[serde(alias = "tokler_version")]
    pub tolkin_version: String,
    pub prices_observed: String,
}

/// Read all records from the resolved data dir. Missing dir or file is an
/// empty ledger, not an error.
#[allow(dead_code)] // public seam kept for the I3 dashboard; stats reads via read_records_in
pub fn read_records() -> (Vec<LedgerRecord>, u64) {
    match data_dir() {
        Some(dir) => read_records_in(&dir),
        None => (Vec::new(), 0),
    }
}

/// Lenient line-by-line read; the second value counts skipped lines so
/// callers can disclose them.
pub fn read_records_in(dir: &Path) -> (Vec<LedgerRecord>, u64) {
    let Ok(text) = fs::read_to_string(dir.join(LEDGER_FILE)) else {
        return (Vec::new(), 0);
    };
    let mut records = Vec::new();
    let mut skipped = 0u64;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<LedgerRecord>(line) {
            Ok(r) => records.push(r),
            Err(_) => skipped += 1,
        }
    }
    (records, skipped)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tolkin-ledger-test-{name}-{}-{}",
            std::process::id(),
            now_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn append_in_creates_jsonl_with_expected_shape() {
        let dir = tmp("shape");
        let project = dir.join("project-root");
        fs::create_dir_all(&project).unwrap();
        append_in(
            &dir,
            "audit",
            &project,
            json!({ "input_tokens": 42, "findings": 3 }),
        )
        .unwrap();
        let text = fs::read_to_string(dir.join(LEDGER_FILE)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 1);
        let v: Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(v["v"], 2);
        assert_eq!(v["command"], "audit");
        assert_eq!(v["headline"]["input_tokens"], 42);
        assert_eq!(v["tolkin_version"], env!("CARGO_PKG_VERSION"));
        assert!(v["prices_observed"].as_str().unwrap_or("").len() >= 4);
        assert!(v["ts"].as_u64().unwrap_or(0) > 1_700_000_000);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn append_in_appends_subsequent_records() {
        let dir = tmp("append");
        append_in(&dir, "scan", &dir, json!({})).unwrap();
        append_in(&dir, "project", &dir, json!({})).unwrap();
        let text = fs::read_to_string(dir.join(LEDGER_FILE)).unwrap();
        assert_eq!(text.lines().count(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn config_round_trips_through_toml() {
        let dir = tmp("config");
        let cfg = Config::new(true, false);
        save_config_to(&dir, &cfg).unwrap();
        let loaded = load_config_from(&dir).unwrap();
        assert_eq!(loaded.v, 1);
        assert!(loaded.consent_ledger);
        assert!(!loaded.consent_log_ingestion);
        assert!(loaded.onboarded_at > 1_700_000_000);
        assert!(loaded.session_rate_per_day.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_config_from_returns_none_when_missing() {
        let dir = tmp("nocfg");
        assert!(load_config_from(&dir).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn is_truthy_env_treats_zero_and_false_as_disabled() {
        // Local helper, not via std::env to keep the test parallel-safe.
        fn check(v: &str) -> bool {
            !v.is_empty() && v != "0" && !v.eq_ignore_ascii_case("false")
        }
        assert!(!check(""));
        assert!(!check("0"));
        assert!(!check("false"));
        assert!(!check("FALSE"));
        assert!(check("1"));
        assert!(check("true"));
        assert!(check("yes"));
    }
}
