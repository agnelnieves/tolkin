//! Integration tests for `tolkin cache`. Each test drives the real binary
//! with its own temp TOLKIN_DATA_DIR and a temp HOME carrying synthetic
//! Claude Code logs, so the owner's real data dir and real logs are never
//! touched (the privacy rule for every test in this repo).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-cache-it-{name}-{}-{}",
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

fn cmd(data_dir: &Path, home: &Path) -> Command {
    let mut c = Command::new(BIN);
    c.env_remove("CI")
        .env_remove("TOLKIN_NO_LEDGER")
        .env_remove("TOKLER_NO_LEDGER")
        .env("TOLKIN_DATA_DIR", data_dir)
        .env("HOME", home)
        .env("TOLKIN_HOME_DIR", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

fn seed_config(dir: &Path, ingestion: bool) {
    fs::write(
        dir.join("config.toml"),
        format!(
            "v = 1\nconsent_ledger = true\nconsent_log_ingestion = {ingestion}\nonboarded_at = 1781100000\n"
        ),
    )
    .unwrap();
}

fn record(
    message_id: &str,
    request_id: &str,
    ts: &str,
    input: u64,
    read: u64,
    w5: u64,
    w1: u64,
) -> String {
    format!(
        concat!(
            r#"{{"message":{{"model":"claude-sonnet-4-6","id":"{mid}","usage":"#,
            r#"{{"input_tokens":{input},"output_tokens":5,"cache_read_input_tokens":{read},"#,
            r#""cache_creation_input_tokens":{creation},"cache_creation":"#,
            r#"{{"ephemeral_5m_input_tokens":{w5},"ephemeral_1h_input_tokens":{w1}}}}}}},"#,
            r#""requestId":"{rid}","cwd":"/work/proj-cache","timestamp":"{ts}"}}"#,
        ),
        mid = message_id,
        rid = request_id,
        ts = ts,
        input = input,
        read = read,
        creation = w5 + w1,
        w5 = w5,
        w1 = w1,
    )
}

/// Seed one synthetic session whose cache numbers are hand-computed in the
/// assertions below: gaps 120 / 600 / 6480 seconds, a 10K first write, and
/// a 2K mid-session churn write.
fn seed_home(home: &Path) {
    let proj = home.join(".claude").join("projects").join("synthetic");
    fs::create_dir_all(&proj).unwrap();
    let lines = [
        record("m1", "r1", "2026-06-10T12:00:00Z", 100, 0, 0, 10_000),
        record("m2", "r2", "2026-06-10T12:02:00Z", 50, 9_000, 0, 0),
        record("m3", "r3", "2026-06-10T12:12:00Z", 50, 9_000, 0, 2_000),
        record("m4", "r4", "2026-06-10T14:00:00Z", 50, 9_000, 0, 0),
    ];
    fs::write(proj.join("sess-cache.jsonl"), lines.join("\n")).unwrap();
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn cache_global_json_reports_hand_computed_numbers() {
    let data = tmp("json-data");
    let home = tmp("json-home");
    seed_config(&data, true);
    seed_home(&home);

    let out = cmd(&data, &home)
        .args(["cache", "--global", "--json"])
        .output()
        .expect("binary runs");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

    assert_eq!(v["scope"], "global");
    assert!(v["project_key"].is_null());
    let cache = &v["cache"];
    assert_eq!(cache["source"], "claude_code");
    assert_eq!(cache["sessions_analyzed"], 1);
    assert_eq!(cache["requests_analyzed"], 4);

    // Hit rate: read 27,000 over input-side 39,250.
    let hr = &cache["hit_rate"];
    assert_eq!(hr["label"], "ground truth");
    assert_eq!(hr["cache_read_tokens"], 27_000);
    assert_eq!(hr["input_side_tokens"], 39_250);
    assert!(approx(hr["rate"].as_f64().unwrap(), 27_000.0 / 39_250.0));
    assert!(hr["advisory"].is_null(), "0.688 is above the threshold");

    // Churn: 2,000 of 12,000 write tokens after the first write.
    let churn = &cache["write_churn"];
    assert_eq!(churn["label"], "ground truth");
    assert_eq!(churn["total_write_tokens"], 12_000);
    assert_eq!(churn["writes_after_first_tokens"], 2_000);
    assert!(approx(churn["share"].as_f64().unwrap(), 2_000.0 / 12_000.0));
    assert_eq!(churn["worst_sessions"][0]["session_id"], "sess-cache");
    assert_eq!(
        churn["worst_sessions"][0]["writes_after_first_tokens"],
        2_000
    );

    // TTL counterfactual: gaps 120/600/6480 -> W5 = 3 events, W1 = 2;
    // prefix proxy 10,000 -> 30,000 and 20,000 simulated write tokens.
    let ttl = &cache["ttl_counterfactual"];
    assert_eq!(ttl["label"], "advisory estimate");
    assert_eq!(ttl["inputs_label"], "ground truth");
    assert_eq!(ttl["observed_write_tokens_5m"], 0);
    assert_eq!(ttl["observed_write_tokens_1h"], 12_000);
    assert_eq!(ttl["simulated_w5_write_events"], 3);
    assert_eq!(ttl["simulated_w1_write_events"], 2);
    assert_eq!(ttl["simulated_w5_write_tokens"], 30_000);
    assert_eq!(ttl["simulated_w1_write_tokens"], 20_000);
    // Sonnet 4.6 marginals: 5m = 3.45, 1h = 5.70 USD/MTok.
    assert!(approx(
        ttl["usd_5m_strategy"].as_f64().unwrap(),
        30_000.0 * 3.45 / 1e6
    ));
    assert!(approx(
        ttl["usd_1h_strategy"].as_f64().unwrap(),
        20_000.0 * 5.7 / 1e6
    ));
    assert!(approx(
        ttl["priced_share_of_simulated_tokens"].as_f64().unwrap(),
        1.0
    ));
    assert!(ttl["verdict"]
        .as_str()
        .unwrap()
        .starts_with("advisory estimate:"));

    // Cadence: 3 intra gaps, 2 over 5m; no inter-session gaps.
    let cadence = &cache["cadence"];
    assert_eq!(cadence["label"], "ground truth");
    assert_eq!(cadence["intra_session_gaps"], 3);
    assert_eq!(cadence["intra_session_gaps_over_5m"], 2);
    assert_eq!(cadence["inter_session_gaps"], 0);
    assert_eq!(cadence["sessions_zero_cache_read"], 0);

    // The scope line ships in the JSON too, verbatim.
    assert!(cache["scope_line"]
        .as_str()
        .unwrap()
        .starts_with("Claude Code manages its own caching"));

    // No em-dash or en-dash anywhere in the output.
    assert!(!stdout.contains('\u{2014}'));
    assert!(!stdout.contains('\u{2013}'));
}

/// Adversarial pin (A7 review, priority 2): the churn headline number is
/// a measured truth but its named risk ("prefix instability caught in the
/// act") only fits a workload where the prefix is meant to be stable. On
/// Claude Code transcripts the conversation grows on most turns, so the
/// suffix written each turn is what `writes_after_first_tokens` counts.
/// The CHURN_NOTE discloses this; the surface headlines must not assert
/// "the prefix changed mid-session" as a flat fact in the same breath as
/// the percentage, because a hostile reader will read 94 percent as 94
/// percent instability rather than 94 percent growth.
#[test]
fn cache_plain_churn_headline_survives_the_hostile_reader_test() {
    let data = tmp("hostile-data");
    let home = tmp("hostile-home");
    seed_config(&data, true);
    seed_home(&home);

    let out = cmd(&data, &home)
        .args(["cache", "--global"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    // The disambiguating note must appear, verbatim.
    assert!(
        stdout.contains("growth-driven suffix writes dominate this share by construction"),
        "CHURN_NOTE must render (current text): {stdout}"
    );
    // The headline next to the percentage must not assert a flat
    // "prefix changed" claim. The metric measures growth OR
    // change; the headline should not collapse to either case.
    assert!(
        !stdout.contains("means the prefix changed mid-session"),
        "headline must not flatten the metric to a 'prefix changed' claim that the CHURN_NOTE then has to walk back: {stdout}"
    );
    // The headline phrasing must still tie the share to the metric's
    // intent: writes after the first within a session, which is what the
    // metric counts.
    assert!(
        stdout.contains("written after a session's first write"),
        "headline must still describe what the metric measures: {stdout}"
    );
}

#[test]
fn cache_plain_output_carries_tier_labels_and_scope_line() {
    let data = tmp("plain-data");
    let home = tmp("plain-home");
    seed_config(&data, true);
    seed_home(&home);

    let out = cmd(&data, &home)
        .args(["cache", "--global"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Tolkin cache health: all projects (machine-wide)"));
    assert!(stdout.contains("ground truth"), "tier 3 label missing");
    assert!(stdout.contains("advisory estimate"), "tier 1 label missing");
    assert!(
        stdout.contains("Claude Code manages its own caching"),
        "scope line missing"
    );
    assert!(
        stdout.contains("All savings are input-token bounded; output may vary."),
        "input-token-bounded footer missing"
    );
    assert!(
        stdout.contains("1.9 x W1 < 1.15 x W5"),
        "break-even missing"
    );
    assert!(!stdout.contains('\u{2014}'));
    assert!(!stdout.contains('\u{2013}'));
}

#[test]
fn cache_without_ingestion_consent_prints_hint_and_exits_zero() {
    let data = tmp("noconsent-data");
    let home = tmp("noconsent-home");
    seed_config(&data, false);
    seed_home(&home);

    for args in [
        vec!["cache", "--global"],
        vec!["cache", "--global", "--json"],
    ] {
        let out = cmd(&data, &home).args(&args).output().expect("binary runs");
        assert!(out.status.success(), "consent-off must not be an error");
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("Cache analysis needs usage-log ingestion"),
            "hint missing for {args:?}: {stdout}"
        );
        assert!(stdout.contains("tolkin init"));
    }
}

#[test]
fn cache_empty_scope_emits_valid_zero_json() {
    // Consent on, but the project scope (a fresh cwd) has no sessions: a
    // real, empty report with the stable schema, not prose.
    let data = tmp("empty-data");
    let home = tmp("empty-home");
    let cwd = tmp("empty-cwd");
    seed_config(&data, true);
    seed_home(&home);

    let out = cmd(&data, &home)
        .current_dir(&cwd)
        .args(["cache", "--json"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");
    assert_eq!(v["scope"], "project");
    assert_eq!(v["cache"]["sessions_analyzed"], 0);
    assert_eq!(v["cache"]["requests_analyzed"], 0);
    assert_eq!(v["cache"]["hit_rate"]["rate"], 0.0);
    assert_eq!(
        v["cache"]["ttl_counterfactual"]["simulated_w5_write_events"],
        0
    );
    assert!(v["cache"]["scope_line"].as_str().is_some());
}

#[test]
fn stats_json_gains_additive_cache_block_without_moving_keys() {
    let data = tmp("stats-data");
    let home = tmp("stats-home");
    seed_config(&data, true);
    seed_home(&home);
    // One ledger record so stats has something to report.
    fs::write(
        data.join("ledger.jsonl"),
        concat!(
            r#"{"v":2,"ts":1781100000,"command":"project","project_key":"/work/proj-cache","#,
            r#""headline":{"always_tokens":1000,"reclaimable_min":10,"reclaimable_max":20},"#,
            r#""tolkin_version":"test","prices_observed":"test"}"#,
            "\n",
        ),
    )
    .unwrap();

    let out = cmd(&data, &home)
        .args(["stats", "--global", "--json"])
        .output()
        .expect("binary runs");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    // Pre-existing documented keys are intact (the skill schema contract).
    for key in [
        "scope",
        "project_key",
        "generated_at",
        "prices_observed",
        "realized_rate",
        "ledger",
        "ingestion",
        "tiers",
    ] {
        assert!(!v[key].is_null() || key == "project_key", "{key} missing");
    }
    let measured = &v["tiers"]["measured"];
    assert!(!measured["cache_hit_rate"].is_null(), "existing key moved");
    assert!(!measured["totals"].is_null(), "existing key moved");
    assert_eq!(measured["label"], "ground truth");

    // The additive cache block, mirroring `tolkin cache --json`.
    let cache = &measured["cache"];
    assert_eq!(cache["source"], "claude_code");
    assert_eq!(cache["sessions_analyzed"], 1);
    assert_eq!(cache["write_churn"]["total_write_tokens"], 12_000);
    assert_eq!(cache["ttl_counterfactual"]["label"], "advisory estimate");
}
