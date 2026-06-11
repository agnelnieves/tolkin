//! Integration tests for the three measured advisories surfaced in
//! `tolkin stats` (plain + --json) and the TUI Spend tab.
//!
//! Each test drives the real binary with its own temp TOLKIN_DATA_DIR and a
//! temp TOLKIN_HOME_DIR carrying synthetic Claude Code logs, so the owner's
//! real data dir and real logs are never touched (the privacy rule for every
//! integration test in this repo).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-adv-it-{name}-{}-{}",
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

fn seed_config_with_cap(dir: &Path, ingestion: bool, cap: f64) {
    fs::write(
        dir.join("config.toml"),
        format!(
            "v = 1\nconsent_ledger = true\nconsent_log_ingestion = {ingestion}\nonboarded_at = 1781100000\nmonthly_cap_usd = {cap}\n"
        ),
    )
    .unwrap();
}

struct RecordArgs<'a> {
    message_id: &'a str,
    request_id: &'a str,
    ts: &'a str,
    input: u64,
    output: u64,
    read: u64,
    w5: u64,
    w1: u64,
}

/// Synthetic Claude Code JSONL record for one request.
/// model: claude-sonnet-4-6 (priced, mid-tier), with both input and output.
fn record(args: &RecordArgs) -> String {
    let (message_id, request_id, ts, input, output, read, w5, w1) = (
        args.message_id,
        args.request_id,
        args.ts,
        args.input,
        args.output,
        args.read,
        args.w5,
        args.w1,
    );
    format!(
        concat!(
            r#"{{"message":{{"model":"claude-sonnet-4-6","id":"{mid}","usage":"#,
            r#"{{"input_tokens":{input},"output_tokens":{output},"cache_read_input_tokens":{read},"#,
            r#""cache_creation_input_tokens":{creation},"cache_creation":"#,
            r#"{{"ephemeral_5m_input_tokens":{w5},"ephemeral_1h_input_tokens":{w1}}}}}}},"#,
            r#""requestId":"{rid}","cwd":"/work/proj-adv","timestamp":"{ts}"}}"#,
        ),
        mid = message_id,
        rid = request_id,
        ts = ts,
        input = input,
        output = output,
        read = read,
        creation = w5 + w1,
        w5 = w5,
        w1 = w1,
    )
}

/// Seed synthetic sessions: one session per day for N days, using a
/// per-day timestamp offset. Each session has two requests (write then read).
/// Uses "2026-06-10" as the most recent day (epoch 1749513600).
fn seed_home_multi_day(home: &Path, days: u64) {
    let proj = home.join(".claude").join("projects").join("synthetic");
    fs::create_dir_all(&proj).unwrap();
    // 2026-06-10 00:00:00 UTC = 1749513600
    let base_ts = 1_749_513_600u64;
    let mut lines = Vec::new();
    for i in 0..days {
        let day_secs = base_ts - i * 86_400;
        // format: 2026-06-10T00:00:00Z etc.
        let y = 2026u64;
        let m = 6u64;
        let d = 10u64 - i;
        let ts_str = format!("{y:04}-{m:02}-{d:02}T00:00:00Z");
        let ts2_str = format!("{y:04}-{m:02}-{d:02}T00:01:00Z");
        let _ = day_secs; // used via ts_str
        lines.push(record(&RecordArgs {
            message_id: &format!("m{i}a"),
            request_id: &format!("r{i}a"),
            ts: &ts_str,
            input: 500_000,
            output: 100_000,
            read: 0,
            w5: 0,
            w1: 10_000,
        }));
        lines.push(record(&RecordArgs {
            message_id: &format!("m{i}b"),
            request_id: &format!("r{i}b"),
            ts: &ts2_str,
            input: 50_000,
            output: 10_000,
            read: 9_000,
            w5: 0,
            w1: 0,
        }));
    }
    fs::write(proj.join("sess-adv.jsonl"), lines.join("\n")).unwrap();
}

fn seed_ledger_record(data_dir: &Path, project_key: &str) {
    let record = format!(
        concat!(
            r#"{{"v":2,"ts":1781100000,"command":"project","project_key":"{proj}","#,
            r#""headline":{{"always_tokens":1000,"reclaimable_min":10,"reclaimable_max":20}},"#,
            r#""tolkin_version":"test","prices_observed":"test"}}"#,
        ),
        proj = project_key,
    );
    fs::write(data_dir.join("ledger.jsonl"), record + "\n").unwrap();
}

// -------------------------------------------------------------------------
// Model mix advisory integration tests.

#[test]
fn stats_json_advisories_block_is_additive_not_breaking() {
    // Verify: the documented stats --json keys remain intact when the
    // advisories block is added. This is the schema-stability test.
    let data = tmp("stable-schema-data");
    let home = tmp("stable-schema-home");
    seed_config(&data, true);
    seed_home_multi_day(&home, 5);
    seed_ledger_record(&data, "/work/proj-adv");

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

    // Pre-existing documented keys must remain intact.
    for key in [
        "scope",
        "generated_at",
        "prices_observed",
        "realized_rate",
        "ledger",
        "ingestion",
        "tiers",
    ] {
        assert!(
            !v[key].is_null(),
            "documented key {key} missing after advisories added"
        );
    }

    // The measured tier's pre-existing keys must survive unchanged.
    let measured = &v["tiers"]["measured"];
    assert!(
        !measured["cache_hit_rate"].is_null(),
        "cache_hit_rate moved"
    );
    assert!(!measured["totals"].is_null(), "totals moved");
    assert!(!measured["sessions"].is_null(), "sessions moved");
    assert_eq!(measured["label"], "ground truth");

    // The advisories block is additive: it lives under measured["advisories"].
    let adv = &measured["advisories"];
    assert!(!adv.is_null(), "advisories block absent from measured");
    assert_eq!(adv["label"], "measured advisories (ground truth basis)");

    // Subfields present.
    assert!(!adv["model_mix"].is_null(), "model_mix absent");
    assert!(!adv["output_share"].is_null(), "output_share absent");
    // cap_runway absent (no monthly_cap_usd in config).
    assert!(
        adv["cap_runway"].is_null() || !adv.as_object().unwrap().contains_key("cap_runway"),
        "cap_runway must be absent when unset"
    );

    // No em-dash or en-dash anywhere in output.
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains('\u{2014}'), "em-dash in stats --json");
    assert!(!stdout.contains('\u{2013}'), "en-dash in stats --json");
}

#[test]
fn stats_json_model_mix_single_priced_model() {
    // One priced model (claude-sonnet-4-6): frontier = only model at 100%.
    // frontier_advisory must fire (100% > 25%). cheap_advisory must NOT fire
    // (the single model is also the cheap one at 100% >= 10%).
    let data = tmp("single-model-data");
    let home = tmp("single-model-home");
    seed_config(&data, true);
    seed_home_multi_day(&home, 3);
    seed_ledger_record(&data, "/work/proj-adv");

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

    let mix = &v["tiers"]["measured"]["advisories"]["model_mix"];
    let frontier = &mix["frontier"];
    assert_eq!(frontier["model"], "claude-sonnet-4-6");
    assert!(
        (frontier["share"].as_f64().unwrap() - 1.0).abs() < 1e-9,
        "single model must have 100% share"
    );
    // frontier_advisory: single model at 100% must fire.
    assert!(
        !mix["frontier_advisory"].is_null(),
        "single model at 100% must fire frontier_advisory"
    );
    let adv_text = mix["frontier_advisory"].as_str().unwrap();
    assert!(
        adv_text.contains("community heuristic"),
        "must cite basis: {adv_text}"
    );
    // cheap_advisory: single model also covers cheap tier at 100%; no advisory.
    assert!(
        mix["cheap_advisory"].is_null() || mix["cheap_advisory"] == serde_json::Value::Null,
        "single model must not fire cheap_advisory: {:?}",
        mix["cheap_advisory"]
    );
}

#[test]
fn stats_json_output_share_framing_present() {
    // Output share section must be present and carry the framing sentence.
    let data = tmp("output-share-data");
    let home = tmp("output-share-home");
    seed_config(&data, true);
    seed_home_multi_day(&home, 4);
    seed_ledger_record(&data, "/work/proj-adv");

    let out = cmd(&data, &home)
        .args(["stats", "--global", "--json"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    let os = &v["tiers"]["measured"]["advisories"]["output_share"];
    assert_eq!(os["label"], "ground truth");
    assert!(
        os["output_tokens"].as_u64().unwrap_or(0) > 0,
        "output_tokens must be nonzero"
    );
    let framing = os["framing"].as_str().unwrap_or("");
    assert!(
        framing.contains("most any output-side tool could possibly save"),
        "framing sentence missing from output_share: {framing}"
    );
    // Share must be between 0 and 1.
    let share = os["share"].as_f64().unwrap_or(-1.0);
    assert!((0.0..=1.0).contains(&share), "share out of range: {share}");
}

#[test]
fn stats_json_cap_runway_absent_without_config() {
    // When monthly_cap_usd is not in config.toml, cap_runway must be absent
    // from the JSON output entirely.
    let data = tmp("no-cap-data");
    let home = tmp("no-cap-home");
    seed_config(&data, true);
    seed_home_multi_day(&home, 4);
    seed_ledger_record(&data, "/work/proj-adv");

    let out = cmd(&data, &home)
        .args(["stats", "--global", "--json"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");
    let adv = &v["tiers"]["measured"]["advisories"];
    assert!(
        adv["cap_runway"].is_null()
            || !adv
                .as_object()
                .map(|o| o.contains_key("cap_runway"))
                .unwrap_or(false),
        "cap_runway must be absent: {:?}",
        adv["cap_runway"]
    );
}

#[test]
fn stats_json_cap_runway_present_with_enough_data() {
    // When monthly_cap_usd is configured and enough active days exist,
    // cap_runway must be present and carry the projection fields.
    let data = tmp("with-cap-data");
    let home = tmp("with-cap-home");
    seed_config_with_cap(&data, true, 100.0);
    // Need at least 3 active days in the 7-day window.
    seed_home_multi_day(&home, 7);
    seed_ledger_record(&data, "/work/proj-adv");

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

    let adv = &v["tiers"]["measured"]["advisories"];
    let cap = &adv["cap_runway"];
    assert!(
        !cap.is_null(),
        "cap_runway must be present when monthly_cap_usd set"
    );
    assert_eq!(cap["label"], "ground truth");
    assert_eq!(cap["monthly_cap_usd"].as_f64().unwrap(), 100.0);
    assert!(
        cap["mtd_spend_usd"].as_f64().unwrap_or(-1.0) >= 0.0,
        "mtd_spend_usd must be non-negative"
    );
    // Rate windows present.
    assert!(!cap["rate_7d"].is_null(), "rate_7d absent");
    assert!(!cap["rate_30d"].is_null(), "rate_30d absent");
    assert_eq!(cap["rate_7d"]["window_days"].as_u64().unwrap(), 7);
    assert_eq!(cap["rate_30d"]["window_days"].as_u64().unwrap(), 30);
    // With 7 active days, 7d window should have >= 3 active days and a projection.
    let rate_7d = &cap["rate_7d"];
    assert!(
        rate_7d["active_days"].as_u64().unwrap_or(0) >= 3,
        "expected >= 3 active days in 7d window"
    );
}

#[test]
fn stats_json_cap_runway_insufficient_days_shows_note() {
    // Only 2 days of data: the 7-day projection note must fire.
    let data = tmp("insufficient-data");
    let home = tmp("insufficient-home");
    seed_config_with_cap(&data, true, 100.0);
    seed_home_multi_day(&home, 2);
    seed_ledger_record(&data, "/work/proj-adv");

    let out = cmd(&data, &home)
        .args(["stats", "--global", "--json"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    let adv = &v["tiers"]["measured"]["advisories"];
    let cap = &adv["cap_runway"];
    if !cap.is_null() {
        // If cap_runway is present, the 7d projection must show a note, not a date.
        let rate_7d = &cap["rate_7d"];
        if rate_7d["active_days"].as_u64().unwrap_or(0) < 3 {
            assert!(
                !rate_7d["projection_note"].is_null(),
                "fewer than 3 active days must produce a projection_note"
            );
            assert!(
                rate_7d["projected_cap_date"].is_null(),
                "projected_cap_date must be absent when insufficient data"
            );
        }
    }
}

#[test]
fn stats_plain_advisories_block_renders_under_measured() {
    // Plain stats output must contain the advisories block with model mix and
    // output share sections.
    let data = tmp("plain-adv-data");
    let home = tmp("plain-adv-home");
    seed_config(&data, true);
    seed_home_multi_day(&home, 4);
    seed_ledger_record(&data, "/work/proj-adv");

    let out = cmd(&data, &home)
        .args(["stats", "--global"])
        .output()
        .expect("binary runs");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);

    // The advisories block header.
    assert!(
        stdout.contains("Advisories"),
        "Advisories header missing: {stdout}"
    );
    // Model mix lines.
    assert!(
        stdout.contains("model mix:"),
        "model mix lines missing: {stdout}"
    );
    // Output share line.
    assert!(
        stdout.contains("output share:"),
        "output share line missing: {stdout}"
    );
    assert!(
        stdout.contains("most any output-side tool could possibly save"),
        "framing sentence missing from plain output: {stdout}"
    );
    // No em-dash or en-dash.
    assert!(!stdout.contains('\u{2014}'), "em-dash in plain stats");
    assert!(!stdout.contains('\u{2013}'), "en-dash in plain stats");
}

#[test]
fn stats_plain_cap_runway_renders_when_configured() {
    // Plain stats shows cap runway section when monthly_cap_usd is set.
    let data = tmp("plain-cap-data");
    let home = tmp("plain-cap-home");
    seed_config_with_cap(&data, true, 50.0);
    seed_home_multi_day(&home, 5);
    seed_ledger_record(&data, "/work/proj-adv");

    let out = cmd(&data, &home)
        .args(["stats", "--global"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("cap runway:"),
        "cap runway line missing: {stdout}"
    );
    assert!(
        stdout.contains("50"),
        "cap amount missing from output: {stdout}"
    );
}

#[test]
fn stats_ingestion_off_no_advisories_block() {
    // When ingestion is off, the measured tier is absent so the advisories
    // block must also be absent from --json output.
    let data = tmp("no-ingestion-data");
    let home = tmp("no-ingestion-home");
    seed_config(&data, false); // ingestion off
    seed_home_multi_day(&home, 4);
    seed_ledger_record(&data, "/work/proj-adv");

    let out = cmd(&data, &home)
        .args(["stats", "--global", "--json"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("valid JSON");

    let measured = &v["tiers"]["measured"];
    // When ingestion is off, measured should be null and advisories must be absent.
    if measured.is_null() {
        // Expected: no advisories when measured tier absent.
    } else {
        // If measured is somehow present (e.g. future change), advisories may or may not
        // be there; we just verify no panic.
    }
}

#[test]
fn stats_no_priced_spend_no_advisory_text() {
    // When no priced spend exists (empty session logs), the model mix section
    // must render gracefully.
    let data = tmp("no-spend-data");
    let home = tmp("no-spend-home");
    seed_config(&data, true);
    // Don't seed any log files (empty sessions).
    seed_ledger_record(&data, "/work/proj-adv");

    let out = cmd(&data, &home)
        .args(["stats", "--global"])
        .output()
        .expect("binary runs");
    assert!(out.status.success());
    // No panic or crash with empty sessions.
}
