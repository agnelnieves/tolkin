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
