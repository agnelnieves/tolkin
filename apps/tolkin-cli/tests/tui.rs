//! TUI surface integration tests.
//!
//! Each test drives the real `tolkin` binary under a tempdir to keep state
//! isolated. None of these tests open a real terminal: bare `tolkin` with
//! piped stdio must exit 2 with usage on stderr (the script-facing contract),
//! `stats --compact` writes a single static frame to stdout, and
//! `stats --tui` must fail gracefully under non-TTY stdio rather than hang or
//! flip the terminal into raw mode.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-tui-test-{name}-{}-{}",
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

fn cmd(data_dir: &Path) -> Command {
    let mut c = Command::new(BIN);
    c.env_remove("CI")
        .env_remove("TOLKIN_NO_LEDGER")
        .env("TOLKIN_DATA_DIR", data_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c
}

fn seed_config(dir: &Path, ingestion: bool) {
    let mut f = fs::File::create(dir.join("config.toml")).unwrap();
    let ingest = if ingestion { "true" } else { "false" };
    writeln!(f, "v = 1").unwrap();
    writeln!(f, "consent_ledger = true").unwrap();
    writeln!(f, "consent_log_ingestion = {ingest}").unwrap();
    writeln!(f, "onboarded_at = 1781100000").unwrap();
}

#[test]
fn bare_tolkin_piped_exits_2_with_usage_on_stderr() {
    // Non-TTY contract: no subcommand = usage error, fast exit, never blocks.
    let dir = tmp("bare-piped");
    let out = cmd(&dir).output().expect("binary runs");
    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("no subcommand provided"),
        "stderr should contain usage error: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_compact_renders_frame_and_exits_zero() {
    let dir = tmp("compact");
    seed_config(&dir, false);
    let out = cmd(&dir).arg("stats").arg("--compact").output().unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("tolkin"), "header missing: {stdout}");
    assert!(stdout.contains("Overview"), "Overview tab title missing");
    assert!(stdout.contains("Project"), "Project tab title missing");
    assert!(stdout.contains("Machine"), "Machine tab title missing");
    assert!(stdout.contains("Spend"), "Spend tab title missing");
    assert!(
        stdout.contains("input savings, output may vary"),
        "honesty line missing: {stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_tui_under_non_tty_errors_without_hanging() {
    let dir = tmp("tui-nontty");
    seed_config(&dir, false);
    // Pipe both stdio handles; the binary must NOT enter raw mode and must
    // return promptly with an error rather than wait on event::poll.
    let out = cmd(&dir).arg("stats").arg("--tui").output().unwrap();
    assert!(
        !out.status.success(),
        "stats --tui under non-TTY should fail"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.to_lowercase().contains("terminal"),
        "stderr should mention the terminal requirement: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn stats_compact_without_config_shows_setup_card() {
    // Fresh tempdir, no config and no ledger: every tab should fall back to the
    // setup card so a brand-new user is never staring at a blank dashboard.
    let dir = tmp("compact-fresh");
    let out = cmd(&dir).arg("stats").arg("--compact").output().unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Setup") || stdout.contains("tolkin init"));
    let _ = fs::remove_dir_all(&dir);
}

fn seed_ledger_record(dir: &Path) {
    // One project snapshot in the ledger's on-disk format (LedgerRecord):
    // the compact frame then renders real numbers instead of empty states.
    let mut f = fs::File::create(dir.join("ledger.jsonl")).unwrap();
    writeln!(
        f,
        "{}",
        concat!(
            r#"{"v":1,"ts":1781100000,"command":"project","project_key":"/tmp/repo","#,
            r#""headline":{"always_tokens":9000,"reclaimable_min":200,"reclaimable_max":800,"#,
            r#""files_scanned":120},"tolkin_version":"0.14.0","prices_observed":"2026-06-10"}"#
        )
    )
    .unwrap();
}

#[test]
fn stats_compact_is_byte_deterministic_for_a_fixed_data_dir() {
    // The compact frame is a shareable artifact: two runs over the same
    // seeded dir must produce identical bytes (animator disabled, no
    // selection chrome, ages pinned to the snapshot clock).
    let dir = tmp("compact-deterministic");
    seed_config(&dir, false);
    seed_ledger_record(&dir);
    let run = || {
        let out = cmd(&dir).arg("stats").arg("--compact").output().unwrap();
        assert!(
            out.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        out.stdout
    };
    let first = run();
    let second = run();
    assert_eq!(
        first, second,
        "compact frames diverged across identical runs"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn reduced_motion_env_does_not_change_the_compact_contract() {
    // TOLKIN_REDUCED_MOTION must be a no-op for the (already static)
    // compact frame: same chrome, same honesty line, byte-equal output.
    let dir = tmp("compact-reduced-motion");
    seed_config(&dir, false);
    seed_ledger_record(&dir);
    let baseline = cmd(&dir).arg("stats").arg("--compact").output().unwrap();
    let reduced = cmd(&dir)
        .env("TOLKIN_REDUCED_MOTION", "1")
        .arg("stats")
        .arg("--compact")
        .output()
        .unwrap();
    assert!(baseline.status.success() && reduced.status.success());
    let text = String::from_utf8_lossy(&reduced.stdout);
    for needle in [
        "tolkin",
        "Overview",
        "Project",
        "Machine",
        "Spend",
        "input savings, output may vary",
    ] {
        assert!(
            text.contains(needle),
            "{needle} missing under reduced motion"
        );
    }
    assert_eq!(
        baseline.stdout, reduced.stdout,
        "reduced motion altered the static frame"
    );
    let _ = fs::remove_dir_all(&dir);
}
