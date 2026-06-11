//! Determinism contract tests: verify that every deterministic command produces
//! byte-identical output across multiple runs, regardless of sidecar configuration.
//! This is the load-bearing proof that the sidecar layer (when added) does not
//! affect deterministic surfaces.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-determinism-{name}-{}-{}",
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

/// Normalizes volatile timestamps in JSON: replaces all ISO-8601 and unix-timestamp
/// values at "generated_at" keys with a fixed placeholder. Keeps output JSON-valid
/// by preserving the value structure (string or number).
fn normalize_timestamps(json_text: &str) -> String {
    let mut result = String::new();
    let mut chars = json_text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '"' && chars.peek() == Some(&'g') {
            // Tentatively found opening quote; check for "generated_at"
            let peek_ahead: String = json_text.chars().skip(result.len() + 1).take(13).collect();
            if peek_ahead.starts_with("generated_at") {
                // Copy through the key and colon
                result.push('"');
                for _ in 0..13 {
                    result.push(chars.next().unwrap());
                }
                result.push('"');

                // Skip whitespace and colon
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == ':' {
                        result.push(chars.next().unwrap());
                        break;
                    }
                    result.push(chars.next().unwrap());
                }

                // Skip whitespace after colon
                while chars
                    .peek()
                    .is_some_and(|c| c.is_whitespace() && *c != '\n')
                {
                    result.push(chars.next().unwrap());
                }

                // Consume the timestamp value (string or number)
                if chars.peek() == Some(&'"') {
                    result.push(chars.next().unwrap());
                    for ts_ch in chars.by_ref() {
                        if ts_ch == '"' {
                            break;
                        }
                    }
                    result.push_str("\"TIMESTAMP_NORMALIZED\"");
                } else if chars
                    .peek()
                    .is_some_and(|c| c.is_ascii_digit() || *c == '-')
                {
                    while chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_digit() || *c == '.')
                    {
                        chars.next();
                    }
                    result.push_str("1234567890");
                }

                continue;
            }
        }
        result.push(ch);
    }

    result
}

/// Run a command with a given data_dir and set of env vars. Returns normalized stdout.
fn run_cmd_normalized(
    data_dir: &Path,
    home_dir: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> (String, bool) {
    let mut c = Command::new(BIN);
    c.env_remove("CI")
        .env_remove("TOLKIN_NO_LEDGER")
        .env("TOLKIN_DATA_DIR", data_dir)
        .env("HOME", home_dir)
        .env("USERPROFILE", home_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    for (k, v) in extra_env {
        c.env(*k, *v);
    }

    for arg in args {
        c.arg(arg);
    }

    let out = c.output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let normalized = normalize_timestamps(&stdout);
    (normalized, out.status.success())
}

// Create a minimal fixture directory with a few small files
fn create_fixture_dir(dir: &Path) {
    let fixture_file = dir.join("test.md");
    fs::write(&fixture_file, "# Test\n\nSome markdown content.\n").unwrap();

    let claude_md = dir.join("CLAUDE.md");
    fs::write(&claude_md, "# Guidelines\n\nTest guidelines.\n").unwrap();

    let code_file = dir.join("main.rs");
    fs::write(&code_file, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
}

#[test]
fn deterministic_audit_with_json_flag() {
    let data_dir = tmp("audit-data");
    let home_dir = tmp("audit-home");
    let fixture_dir = tmp("audit-fixture");
    create_fixture_dir(&fixture_dir);

    let fixture_file = fixture_dir.join("test.md");

    let (out1, success1) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["audit", &fixture_file.to_string_lossy(), "--json"],
        &[],
    );
    let (out2, success2) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["audit", &fixture_file.to_string_lossy(), "--json"],
        &[],
    );

    assert!(success1, "audit run 1 should succeed");
    assert!(success2, "audit run 2 should succeed");
    assert_eq!(
        out1, out2,
        "audit --json output should be identical across runs"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture_dir);
}

#[test]
fn deterministic_scan_with_json_flag() {
    let data_dir = tmp("scan-data");
    let home_dir = tmp("scan-home");

    // scan uses HOME to find .config, so we point it at our temp home
    let (out1, success1) = run_cmd_normalized(&data_dir, &home_dir, &["scan", "--json"], &[]);
    let (out2, success2) = run_cmd_normalized(&data_dir, &home_dir, &["scan", "--json"], &[]);

    assert!(success1, "scan run 1 should succeed");
    assert!(success2, "scan run 2 should succeed");
    assert_eq!(
        out1, out2,
        "scan --json output should be identical across runs"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
}

#[test]
fn deterministic_project_with_json_flag() {
    let data_dir = tmp("project-data");
    let home_dir = tmp("project-home");
    let fixture_dir = tmp("project-fixture");
    create_fixture_dir(&fixture_dir);

    let (out1, success1) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["project", &fixture_dir.to_string_lossy(), "--json"],
        &[],
    );
    let (out2, success2) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["project", &fixture_dir.to_string_lossy(), "--json"],
        &[],
    );

    assert!(success1, "project run 1 should succeed");
    assert!(success2, "project run 2 should succeed");
    assert_eq!(
        out1, out2,
        "project --json output should be identical across runs"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture_dir);
}

#[test]
fn deterministic_stats_with_json_flag() {
    let data_dir = tmp("stats-data");
    let home_dir = tmp("stats-home");

    let (out1, success1) = run_cmd_normalized(&data_dir, &home_dir, &["stats", "--json"], &[]);
    let (out2, success2) = run_cmd_normalized(&data_dir, &home_dir, &["stats", "--json"], &[]);

    assert!(success1, "stats run 1 should succeed");
    assert!(success2, "stats run 2 should succeed");
    assert_eq!(
        out1, out2,
        "stats --json output should be identical across runs"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
}

#[test]
fn deterministic_cache_with_json_flag() {
    let data_dir = tmp("cache-data");
    let home_dir = tmp("cache-home");

    let (out1, success1) = run_cmd_normalized(&data_dir, &home_dir, &["cache", "--json"], &[]);
    let (out2, success2) = run_cmd_normalized(&data_dir, &home_dir, &["cache", "--json"], &[]);

    assert!(success1, "cache run 1 should succeed");
    assert!(success2, "cache run 2 should succeed");
    assert_eq!(
        out1, out2,
        "cache --json output should be identical across runs"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
}

// Sidecar-ignorance tests: runs the same command with and without hostile
// sidecar env vars set. Output must be identical (post-normalization).

#[test]
fn deterministic_commands_ignore_sidecar_env_audit() {
    let data_dir = tmp("sidecar-audit-data");
    let home_dir = tmp("sidecar-audit-home");
    let fixture_dir = tmp("sidecar-audit-fixture");
    create_fixture_dir(&fixture_dir);

    let fixture_file = fixture_dir.join("test.md");

    let (out_without, success_without) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["audit", &fixture_file.to_string_lossy(), "--json"],
        &[],
    );

    let (out_with, success_with) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["audit", &fixture_file.to_string_lossy(), "--json"],
        &[
            ("TOLKIN_NO_SIDECAR", "0"),
            ("TOLKIN_SIDECAR_BASE_URL", "http://127.0.0.1:1"),
            ("TOLKIN_SIDECAR_ALLOW_REMOTE", "1"),
        ],
    );

    assert!(success_without, "audit without sidecar env should succeed");
    assert!(success_with, "audit with sidecar env should succeed");
    assert_eq!(
        out_without, out_with,
        "audit --json should ignore sidecar env vars"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture_dir);
}

#[test]
fn deterministic_commands_ignore_sidecar_env_scan() {
    let data_dir = tmp("sidecar-scan-data");
    let home_dir = tmp("sidecar-scan-home");

    let (out_without, success_without) =
        run_cmd_normalized(&data_dir, &home_dir, &["scan", "--json"], &[]);

    let (out_with, success_with) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["scan", "--json"],
        &[
            ("TOLKIN_NO_SIDECAR", "0"),
            ("TOLKIN_SIDECAR_BASE_URL", "http://127.0.0.1:1"),
            ("TOLKIN_SIDECAR_ALLOW_REMOTE", "1"),
        ],
    );

    assert!(success_without, "scan without sidecar env should succeed");
    assert!(success_with, "scan with sidecar env should succeed");
    assert_eq!(
        out_without, out_with,
        "scan --json should ignore sidecar env vars"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
}

#[test]
fn deterministic_commands_ignore_sidecar_env_project() {
    let data_dir = tmp("sidecar-project-data");
    let home_dir = tmp("sidecar-project-home");
    let fixture_dir = tmp("sidecar-project-fixture");
    create_fixture_dir(&fixture_dir);

    let (out_without, success_without) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["project", &fixture_dir.to_string_lossy(), "--json"],
        &[],
    );

    let (out_with, success_with) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["project", &fixture_dir.to_string_lossy(), "--json"],
        &[
            ("TOLKIN_NO_SIDECAR", "0"),
            ("TOLKIN_SIDECAR_BASE_URL", "http://127.0.0.1:1"),
            ("TOLKIN_SIDECAR_ALLOW_REMOTE", "1"),
        ],
    );

    assert!(
        success_without,
        "project without sidecar env should succeed"
    );
    assert!(success_with, "project with sidecar env should succeed");
    assert_eq!(
        out_without, out_with,
        "project --json should ignore sidecar env vars"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture_dir);
}

#[test]
fn deterministic_commands_ignore_sidecar_env_stats() {
    let data_dir = tmp("sidecar-stats-data");
    let home_dir = tmp("sidecar-stats-home");

    let (out_without, success_without) =
        run_cmd_normalized(&data_dir, &home_dir, &["stats", "--json"], &[]);

    let (out_with, success_with) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["stats", "--json"],
        &[
            ("TOLKIN_NO_SIDECAR", "0"),
            ("TOLKIN_SIDECAR_BASE_URL", "http://127.0.0.1:1"),
            ("TOLKIN_SIDECAR_ALLOW_REMOTE", "1"),
        ],
    );

    assert!(success_without, "stats without sidecar env should succeed");
    assert!(success_with, "stats with sidecar env should succeed");
    assert_eq!(
        out_without, out_with,
        "stats --json should ignore sidecar env vars"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
}

#[test]
fn deterministic_commands_ignore_sidecar_env_cache() {
    let data_dir = tmp("sidecar-cache-data");
    let home_dir = tmp("sidecar-cache-home");

    let (out_without, success_without) =
        run_cmd_normalized(&data_dir, &home_dir, &["cache", "--json"], &[]);

    let (out_with, success_with) = run_cmd_normalized(
        &data_dir,
        &home_dir,
        &["cache", "--json"],
        &[
            ("TOLKIN_NO_SIDECAR", "0"),
            ("TOLKIN_SIDECAR_BASE_URL", "http://127.0.0.1:1"),
            ("TOLKIN_SIDECAR_ALLOW_REMOTE", "1"),
        ],
    );

    assert!(success_without, "cache without sidecar env should succeed");
    assert!(success_with, "cache with sidecar env should succeed");
    assert_eq!(
        out_without, out_with,
        "cache --json should ignore sidecar env vars"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
}

#[test]
fn deterministic_version_ignores_sidecar_env() {
    let home_dir = tmp("version-home");

    let mut c1 = Command::new(BIN);
    c1.env("HOME", &home_dir)
        .env("USERPROFILE", &home_dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("--version");

    let out1 = c1.output().unwrap();
    let stdout1 = String::from_utf8_lossy(&out1.stdout).to_string();

    let mut c2 = Command::new(BIN);
    c2.env("HOME", &home_dir)
        .env("USERPROFILE", &home_dir)
        .env("TOLKIN_NO_SIDECAR", "0")
        .env("TOLKIN_SIDECAR_BASE_URL", "http://127.0.0.1:1")
        .env("TOLKIN_SIDECAR_ALLOW_REMOTE", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .arg("--version");

    let out2 = c2.output().unwrap();
    let stdout2 = String::from_utf8_lossy(&out2.stdout).to_string();

    assert_eq!(stdout1, stdout2, "--version should ignore sidecar env vars");

    let _ = fs::remove_dir_all(&home_dir);
}
