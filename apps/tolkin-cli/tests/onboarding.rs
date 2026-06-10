//! Integration tests for the onboarding, ledger, and stats surface. Each test
//! drives the real `tolkin` binary in its own tempdir and sets the env vars
//! per-Command so they never bleed across tests (`std::env::set_var` is
//! deliberately avoided).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-onboarding-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Build a Command with CI and TOLKIN_NO_LEDGER stripped from the inherited
/// env. The harness machine may set CI=true; tests that need ledger writes
/// must not inherit that.
fn cmd(data_dir: &Path) -> Command {
    let mut c = Command::new(BIN);
    c.env_remove("CI")
        .env_remove("TOLKIN_NO_LEDGER")
        .env("TOLKIN_DATA_DIR", data_dir);
    c
}

fn ledger_path(data_dir: &Path) -> PathBuf {
    data_dir.join("ledger.jsonl")
}

fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config.toml")
}

#[test]
fn init_yes_writes_config_with_defaults() {
    let dir = tmp("init-yes");
    let out = cmd(&dir).arg("init").arg("--yes").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let config = config_path(&dir);
    assert!(
        config.is_file(),
        "config.toml missing at {}",
        config.display()
    );
    let text = fs::read_to_string(&config).unwrap();
    // consent_ledger=true, consent_log_ingestion=false by default.
    assert!(text.contains("consent_ledger = true"), "{text}");
    assert!(text.contains("consent_log_ingestion = false"), "{text}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn audit_after_init_appends_one_ledger_record() {
    let dir = tmp("audit");
    let out = cmd(&dir).arg("init").arg("--yes").output().unwrap();
    assert!(out.status.success());

    let fixture = dir.join("fixture.txt");
    fs::write(&fixture, "Hello tolkin.\nThis is a small fixture file.\n").unwrap();

    let out = cmd(&dir).arg("audit").arg(&fixture).output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let ledger = ledger_path(&dir);
    assert!(ledger.is_file(), "ledger.jsonl missing");
    let text = fs::read_to_string(&ledger).unwrap();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "expected 1 record, got: {text}");
    let v: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(v["v"], 2);
    assert_eq!(v["command"], "audit");
    assert_eq!(v["tolkin_version"], env!("CARGO_PKG_VERSION"));
    assert!(
        v["prices_observed"].as_str().unwrap_or("").len() >= 4,
        "prices_observed missing: {v}"
    );
    assert!(
        v["ts"].as_u64().unwrap_or(0) > 1_700_000_000,
        "ts not plausible: {v}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_prints_count_and_reset_clears_ledger() {
    let dir = tmp("stats");
    assert!(cmd(&dir)
        .arg("init")
        .arg("--yes")
        .output()
        .unwrap()
        .status
        .success());

    let fixture = dir.join("fixture.txt");
    fs::write(&fixture, "Some content here.\n").unwrap();
    assert!(cmd(&dir)
        .arg("audit")
        .arg(&fixture)
        .output()
        .unwrap()
        .status
        .success());

    let out = cmd(&dir).arg("stats").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Ledger: 1 records"), "stdout: {stdout}");
    assert!(stdout.contains("Identified"), "stdout: {stdout}");
    assert!(stdout.contains("audit findings"), "stdout: {stdout}");
    assert!(stdout.contains("input-token bounded"), "stdout: {stdout}");

    let out = cmd(&dir).arg("stats").arg("--json").output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stats --json parses");
    assert_eq!(v["scope"], "project");
    assert_eq!(v["ledger"]["records"], 1);
    assert!(v["tiers"]["identified"].is_object());
    assert!(v["tiers"]["realized"].is_null());
    assert!(v["tiers"]["measured"].is_null());

    let out = cmd(&dir).arg("stats").arg("--reset").output().unwrap();
    assert!(out.status.success());
    assert!(!ledger_path(&dir).is_file(), "ledger should be gone");
    assert!(config_path(&dir).is_file(), "config should survive reset");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ci_env_suppresses_all_writes() {
    let dir = tmp("ci");
    let fixture = dir.join("fixture.txt");
    fs::write(&fixture, "Some content.\n").unwrap();

    // Build a Command from scratch so we can leave CI=true in place and
    // skip the init step entirely.
    let mut c = Command::new(BIN);
    c.env_remove("TOLKIN_NO_LEDGER")
        .env("TOLKIN_DATA_DIR", &dir)
        .env("CI", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("audit")
        .arg(&fixture);
    let out = c.output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !config_path(&dir).is_file(),
        "config should not exist under CI"
    );
    assert!(
        !ledger_path(&dir).is_file(),
        "ledger should not exist under CI"
    );
    let _ = fs::remove_dir_all(&dir);
}
