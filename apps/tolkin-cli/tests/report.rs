//! Integration tests for `tolkin report --html`. Each test drives the real
//! `tolkin` binary in its own tempdir so state never bleeds across runs. The
//! report command READS the ledger; writes (and log ingestion) are governed by
//! `CI=true` / `TOLKIN_NO_LEDGER`, so the `report` surface must keep working
//! under those env flags.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-report-it-{name}-{}-{}",
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

fn seed_consented_config(dir: &Path) {
    fs::write(
        dir.join("config.toml"),
        "v = 1\nconsent_ledger = true\nconsent_log_ingestion = false\nonboarded_at = 1781100000\n",
    )
    .unwrap();
}

#[test]
fn report_writes_html_file_and_prints_path() {
    let dir = tmp("html-file");
    let out_dir = tmp("html-out");
    seed_consented_config(&dir);

    let out_path = out_dir.join("report.html");
    let out = cmd(&dir)
        .arg("report")
        .arg("--global")
        .arg("--output")
        .arg(&out_path)
        .output()
        .expect("binary runs");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("report.html"),
        "expected output path on stdout: {stdout}"
    );

    assert!(
        out_path.is_file(),
        "expected file at {}",
        out_path.display()
    );
    let html = fs::read_to_string(&out_path).unwrap();
    assert!(html.contains("Tolkin savings report"), "title missing");
    assert!(
        html.contains("All savings are input-token bounded; output may vary."),
        "honesty line missing"
    );
    assert!(html.starts_with("<!doctype html>"));

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&out_dir);
}

#[test]
fn report_under_ci_env_still_renders() {
    // The report surface READS the ledger; the env kill governs writes and log
    // ingestion, not reading. Stats behaves the same way: under CI=true the
    // report command must still produce a file.
    let dir = tmp("ci");
    let out_dir = tmp("ci-out");
    seed_consented_config(&dir);

    let out_path = out_dir.join("report.html");
    let mut c = Command::new(BIN);
    c.env_remove("TOLKIN_NO_LEDGER")
        .env("TOLKIN_DATA_DIR", &dir)
        .env("CI", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("report")
        .arg("--output")
        .arg(&out_path);
    let out = c.output().expect("binary runs");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out_path.is_file(), "report missing under CI");
    let html = fs::read_to_string(&out_path).unwrap();
    assert!(html.contains("Tolkin savings report"));

    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&out_dir);
}
