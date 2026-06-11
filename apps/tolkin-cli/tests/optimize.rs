//! Integration tests for `tolkin optimize`. Conventions match
//! tests/determinism.rs: temp HOME, TOLKIN_DATA_DIR, CARGO_BIN_EXE_tolkin.
//! Every test pins TOLKIN_NO_SIDECAR=1 and CI=1 so no probe or chat traffic
//! ever leaves the test process.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-optimize-{name}-{}-{}",
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

/// Fixture: a CLAUDE.md plus one skill so both tasks have material.
fn create_fixture_dir(dir: &Path) {
    fs::write(
        dir.join("CLAUDE.md"),
        "# Guidelines\n\nTest guidelines for the optimize fixture.\n",
    )
    .unwrap();
    let skill_dir = dir.join(".claude").join("skills").join("demo");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: demo\ndescription: demo skill for tests\n---\n\nUse this when testing optimize.\n",
    )
    .unwrap();
}

/// Run optimize in `cwd` with the sidecar hard-off. Returns (stdout, success).
fn run_optimize(data_dir: &Path, home_dir: &Path, cwd: &Path, args: &[&str]) -> (String, bool) {
    let mut c = Command::new(BIN);
    c.env("TOLKIN_DATA_DIR", data_dir)
        .env("HOME", home_dir)
        .env("USERPROFILE", home_dir)
        .env("TOLKIN_NO_SIDECAR", "1")
        .env("CI", "1")
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    c.arg("optimize");
    for arg in args {
        c.arg(arg);
    }
    let out = c.output().unwrap();
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        out.status.success(),
    )
}

#[test]
fn dry_run_json_parses_with_null_advisory_and_exits_zero() {
    let data_dir = tmp("json-data");
    let home_dir = tmp("json-home");
    let fixture = tmp("json-fixture");
    create_fixture_dir(&fixture);

    let (stdout, success) = run_optimize(&data_dir, &home_dir, &fixture, &["--dry-run", "--json"]);
    assert!(success, "optimize --dry-run --json should exit 0");

    let doc: serde_json::Value =
        serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert!(doc["model_advisory"].is_null(), "model path is hard-off");
    assert_eq!(doc["dry_run"], serde_json::Value::Bool(true));
    assert_eq!(doc["consent"], "unavailable");
    assert!(
        doc["probes"].as_array().is_some_and(Vec::is_empty),
        "detection must be skipped entirely under CI/TOLKIN_NO_SIDECAR"
    );
    assert!(doc["skeleton"]["totals"]["files_scanned"].is_u64());
    assert!(doc["estimate"]["total_seconds"].is_u64());
    assert!(doc["estimate"]["basis"]
        .as_str()
        .is_some_and(|b| b.contains("estimate")));
    assert_eq!(doc["version"], env!("CARGO_PKG_VERSION"));

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

#[test]
fn dry_run_json_is_byte_identical_across_runs() {
    let data_dir = tmp("ident-data");
    let home_dir = tmp("ident-home");
    let fixture = tmp("ident-fixture");
    create_fixture_dir(&fixture);

    let (out1, ok1) = run_optimize(&data_dir, &home_dir, &fixture, &["--dry-run", "--json"]);
    let (out2, ok2) = run_optimize(&data_dir, &home_dir, &fixture, &["--dry-run", "--json"]);
    assert!(ok1 && ok2, "both runs should exit 0");
    assert_eq!(
        out1, out2,
        "optimize --dry-run --json must be byte-identical across runs"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

#[test]
fn dry_run_skills_task_lists_the_fixture_skill_file() {
    let data_dir = tmp("skills-data");
    let home_dir = tmp("skills-home");
    let fixture = tmp("skills-fixture");
    create_fixture_dir(&fixture);

    let (stdout, success) = run_optimize(
        &data_dir,
        &home_dir,
        &fixture,
        &["--task", "skills", "--dry-run"],
    );
    assert!(success, "optimize --task skills --dry-run should exit 0");
    assert!(
        stdout.contains(".claude/skills/demo/SKILL.md"),
        "dry run should list the fixture SKILL.md, got:\n{stdout}"
    );
    assert!(stdout.contains("task skills: 1 file"));
    assert!(
        !stdout.contains("task narrate"),
        "--task skills must not plan the narrate task"
    );
    assert!(stdout.contains("dry run: no model calls were made"));
    assert!(stdout.contains("estimate"));

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

#[test]
fn human_run_without_sidecar_keeps_the_deterministic_skeleton_and_exits_zero() {
    let data_dir = tmp("human-data");
    let home_dir = tmp("human-home");
    let fixture = tmp("human-fixture");
    create_fixture_dir(&fixture);

    let (stdout, success) = run_optimize(&data_dir, &home_dir, &fixture, &[]);
    assert!(success, "optimize without sidecar should exit 0");
    assert!(stdout.contains("Deterministic summary"));
    assert!(stdout.contains("local model path disabled"));
    assert!(
        !stdout.contains("model advisory (local)"),
        "no advisory sections may render when the model path is off"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

#[test]
fn dry_run_show_prompts_prints_redacted_exact_prompts() {
    let data_dir = tmp("prompts-data");
    let home_dir = tmp("prompts-home");
    let fixture = tmp("prompts-fixture");
    create_fixture_dir(&fixture);
    // Plant a secret in the skill body; it must never reach a prompt.
    let skill = fixture.join(".claude/skills/demo/SKILL.md");
    fs::write(
        &skill,
        "---\nname: demo\ndescription: demo skill for tests\n---\n\nexport OPENAI_API_KEY=sk-proj-abcdefghijklmnopqrstuvwxyz123456T3BlbkFJabcdefghijklmnopqrstuvwxyz123456\n",
    )
    .unwrap();

    let (stdout, success) = run_optimize(
        &data_dir,
        &home_dir,
        &fixture,
        &["--task", "skills", "--dry-run", "--show-prompts"],
    );
    assert!(success);
    assert!(stdout.contains("prompts (exact, after secret redaction)"));
    assert!(stdout.contains("--- skills user: .claude/skills/demo/SKILL.md ---"));
    assert!(
        !stdout.contains("T3BlbkFJ"),
        "secret values must be redacted before entering any prompt"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

/// Write two cached manifest JSON files in the pinned cache shape.
/// The two tools across servers intentionally echo each other (paraphrases)
/// so the mcp task prompt would contain both for the model to compare.
fn create_mcp_manifest_fixtures(dir: &Path) {
    let cache_dir = dir.join(".tolkin").join("mcp-manifests");
    fs::create_dir_all(&cache_dir).unwrap();

    let alpha = serde_json::json!({
        "v": 1,
        "server": "alpha",
        "captured_at": "2026-06-01",
        "transport": "stdio",
        "source": "probe",
        "protocol_version": null,
        "tools": [
            {
                "name": "search_files",
                "description": "Searches files in the workspace by query string and returns matching paths",
                "inputSchema": { "type": "object" }
            }
        ]
    });
    let beta = serde_json::json!({
        "v": 1,
        "server": "beta",
        "captured_at": "2026-06-01",
        "transport": "http",
        "source": "probe",
        "protocol_version": null,
        "tools": [
            {
                "name": "find_files",
                "description": "Locates files in the workspace matching a query and returns their paths",
                "inputSchema": { "type": "object" }
            }
        ]
    });

    let mut alpha_text = serde_json::to_string_pretty(&alpha).unwrap();
    alpha_text.push('\n');
    let mut beta_text = serde_json::to_string_pretty(&beta).unwrap();
    beta_text.push('\n');

    fs::write(cache_dir.join("alpha.json"), alpha_text).unwrap();
    fs::write(cache_dir.join("beta.json"), beta_text).unwrap();
}

#[test]
fn mcp_dry_run_lists_both_manifests_with_token_counts_and_no_network_calls() {
    let data_dir = tmp("mcp-dry-data");
    let home_dir = tmp("mcp-dry-home");
    let fixture = tmp("mcp-dry-fixture");
    create_fixture_dir(&fixture);
    create_mcp_manifest_fixtures(&fixture);

    let (stdout, success) = run_optimize(
        &data_dir,
        &home_dir,
        &fixture,
        &["--task", "mcp", "--dry-run"],
    );
    assert!(
        success,
        "optimize --task mcp --dry-run should exit 0; got:\n{stdout}"
    );
    // Both manifests appear in the dry run listing.
    assert!(
        stdout.contains("alpha.json"),
        "dry run should list alpha.json; got:\n{stdout}"
    );
    assert!(
        stdout.contains("beta.json"),
        "dry run should list beta.json; got:\n{stdout}"
    );
    // Token counts appear (the numbers will vary but the labels must be present).
    assert!(
        stdout.contains("tokens"),
        "dry run should show token counts; got:\n{stdout}"
    );
    // Confirm no model calls.
    assert!(
        stdout.contains("dry run: no model calls were made"),
        "dry run must declare no model calls; got:\n{stdout}"
    );
    // The narrate and skills tasks must not appear.
    assert!(
        !stdout.contains("task narrate"),
        "--task mcp must not plan narrate; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("task skills"),
        "--task mcp must not plan skills; got:\n{stdout}"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

#[test]
fn mcp_no_sidecar_reports_model_path_unavailable_and_exits_zero() {
    let data_dir = tmp("mcp-nosidecar-data");
    let home_dir = tmp("mcp-nosidecar-home");
    let fixture = tmp("mcp-nosidecar-fixture");
    create_fixture_dir(&fixture);
    create_mcp_manifest_fixtures(&fixture);

    // TOLKIN_NO_SIDECAR=1 is already set by run_optimize; confirming exit 0
    // and no advisory output.
    let (stdout, success) = run_optimize(&data_dir, &home_dir, &fixture, &["--task", "mcp"]);
    assert!(
        success,
        "optimize --task mcp with no sidecar should exit 0; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("model advisory (local)"),
        "no advisory when model path is off; got:\n{stdout}"
    );
    // Deterministic skeleton must still render.
    assert!(
        stdout.contains("Deterministic summary"),
        "deterministic skeleton must still render; got:\n{stdout}"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

#[test]
fn mcp_zero_manifests_prints_probe_hint_and_exits_zero() {
    let data_dir = tmp("mcp-zero-data");
    let home_dir = tmp("mcp-zero-home");
    let fixture = tmp("mcp-zero-fixture");
    // Create fixture dir WITHOUT any manifests.
    create_fixture_dir(&fixture);

    let (stdout, success) = run_optimize(
        &data_dir,
        &home_dir,
        &fixture,
        &["--task", "mcp", "--dry-run"],
    );
    assert!(success, "zero manifests should exit 0; got:\n{stdout}");
    assert!(
        stdout.contains("no cached manifests") || stdout.contains("tolkin mcp --probe"),
        "should mention the probe hint; got:\n{stdout}"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

#[test]
fn mcp_dry_run_show_prompts_prints_mcp_system_and_index() {
    let data_dir = tmp("mcp-prompts-data");
    let home_dir = tmp("mcp-prompts-home");
    let fixture = tmp("mcp-prompts-fixture");
    create_fixture_dir(&fixture);
    create_mcp_manifest_fixtures(&fixture);

    let (stdout, success) = run_optimize(
        &data_dir,
        &home_dir,
        &fixture,
        &["--task", "mcp", "--dry-run", "--show-prompts"],
    );
    assert!(
        success,
        "mcp --dry-run --show-prompts should exit 0; got:\n{stdout}"
    );
    assert!(
        stdout.contains("--- mcp system ---"),
        "show-prompts must print the mcp system prompt; got:\n{stdout}"
    );
    assert!(
        stdout.contains("--- mcp user ---"),
        "show-prompts must print the mcp user prompt; got:\n{stdout}"
    );
    // The index must contain lines from both servers.
    assert!(
        stdout.contains("alpha :: search_files"),
        "index must include alpha server tools; got:\n{stdout}"
    );
    assert!(
        stdout.contains("beta :: find_files"),
        "index must include beta server tools; got:\n{stdout}"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

/// Under CI=1 and TOLKIN_NO_SIDECAR=1 the model path is disabled, so
/// `suggested_model` must be null (not populated) in the JSON output.
#[test]
fn dry_run_json_suggested_model_null_when_disabled() {
    let data_dir = tmp("sm-disabled-data");
    let home_dir = tmp("sm-disabled-home");
    let fixture = tmp("sm-disabled-fixture");
    create_fixture_dir(&fixture);

    let (stdout, success) = run_optimize(&data_dir, &home_dir, &fixture, &["--dry-run", "--json"]);
    assert!(success, "optimize --dry-run --json should exit 0");

    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    // CI=1 and TOLKIN_NO_SIDECAR=1 are set by run_optimize; the model path is
    // disabled, so suggested_model must be null, not a populated object.
    assert!(
        doc["suggested_model"].is_null(),
        "suggested_model must be null when the model path is disabled; got: {}",
        doc["suggested_model"]
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

/// The JSON output shape for `suggested_model` is either null (model path
/// disabled) or an object with the required stable keys. Under CI the model
/// path is disabled so this test only exercises the null branch, but it
/// confirms the document always parses and never blocks.
#[test]
fn dry_run_json_suggested_model_is_null_or_well_formed() {
    let data_dir = tmp("sm-shape-data");
    let home_dir = tmp("sm-shape-home");
    let fixture = tmp("sm-shape-fixture");
    create_fixture_dir(&fixture);

    let (stdout, success) = run_optimize(&data_dir, &home_dir, &fixture, &["--dry-run", "--json"]);
    assert!(success, "optimize --dry-run --json should exit 0");

    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");

    let sm = &doc["suggested_model"];
    if !sm.is_null() {
        // If populated, all required keys must be present.
        assert!(
            sm["model"].is_string(),
            "suggested_model.model must be a string"
        );
        assert!(
            sm["download_gb"].is_number(),
            "suggested_model.download_gb must be a number"
        );
        assert!(
            sm["reason"].is_string(),
            "suggested_model.reason must be a string"
        );
        assert!(
            sm["assumed_ram"].is_boolean(),
            "suggested_model.assumed_ram must be a bool"
        );
        assert!(
            sm["setup_hint"].is_string(),
            "suggested_model.setup_hint must be a string"
        );
    }

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

/// Non-TTY JSON run with no sidecar must exit promptly without blocking on
/// stdin. run_optimize sets stdin=Stdio::null() which simulates a pipe.
#[test]
fn non_tty_json_never_blocks_on_stdin() {
    let data_dir = tmp("ntty-json-data");
    let home_dir = tmp("ntty-json-home");
    let fixture = tmp("ntty-json-fixture");
    create_fixture_dir(&fixture);

    let (stdout, success) = run_optimize(&data_dir, &home_dir, &fixture, &["--json"]);
    assert!(success, "optimize --json should exit 0 in non-TTY mode");
    let doc: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must be valid JSON");
    assert!(doc["version"].is_string());
    assert!(doc["skeleton"].is_object());

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}

/// Non-TTY human-mode run with no sidecar must not block on stdin and must
/// not print the interactive guide prompt.
#[test]
fn non_tty_human_never_blocks_on_stdin() {
    let data_dir = tmp("ntty-human-data");
    let home_dir = tmp("ntty-human-home");
    let fixture = tmp("ntty-human-fixture");
    create_fixture_dir(&fixture);

    let (stdout, success) = run_optimize(&data_dir, &home_dir, &fixture, &[]);
    assert!(success, "optimize should exit 0 in non-TTY mode");
    assert!(
        stdout.contains("Deterministic summary"),
        "deterministic skeleton must still render; got:\n{stdout}"
    );
    assert!(
        !stdout.contains("Would you like to see how to install"),
        "interactive prompt must not appear in non-TTY output; got:\n{stdout}"
    );

    let _ = fs::remove_dir_all(&data_dir);
    let _ = fs::remove_dir_all(&home_dir);
    let _ = fs::remove_dir_all(&fixture);
}
