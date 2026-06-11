//! Integration tests for tools/list ingestion in `tolkin mcp`. Each test
//! drives the real binary against the hand-made fixture manifest at
//! fixtures/mcp/sample-tools-list.json (four tools, deliberate description
//! smells) and asserts the rendered table, the JSON shape, and the basis
//! labeling discipline: exact tokenized-manifest counts supersede catalog
//! estimates and the two are never blended unlabeled.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_tolkin");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/mcp/sample-tools-list.json")
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tolkin-mcp-tools-test-{name}-{}-{}",
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

/// Command with an isolated data dir so no test touches real state.
fn cmd(data_dir: &Path) -> Command {
    let mut c = Command::new(BIN);
    c.env_remove("CI")
        .env_remove("TOLKIN_NO_LEDGER")
        .env("TOLKIN_DATA_DIR", data_dir);
    c
}

fn run_json(data_dir: &Path, args: &[&str]) -> serde_json::Value {
    let out = cmd(data_dir).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "tolkin {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is JSON")
}

#[test]
fn positional_manifest_auto_detects_and_renders_table() {
    let dir = tmp("auto-detect");
    let out = cmd(&dir).arg("mcp").arg(fixture()).output().unwrap();
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("tools/list manifest (single server)"),
        "{text}"
    );
    assert!(text.contains("Per-tool breakdown"), "{text}");
    assert!(text.contains("tokenized manifest (o200k_base)"), "{text}");
    // All four fixture tools appear in the table.
    for tool in [
        "create_ticket",
        "helpful_search",
        "list_tickets_open",
        "list_tickets_closed",
    ] {
        assert!(text.contains(tool), "missing {tool} in:\n{text}");
    }
    // The deliberate smells fire.
    assert!(text.contains("no-leading-purpose-verb"), "{text}");
    assert!(text.contains("near-duplicate-descriptions"), "{text}");
    // The compact-clarity stance is rendered, never a lengthen suggestion.
    assert!(text.contains("never lengthen"), "{text}");
}

#[test]
fn manifest_json_shape_is_additive_and_exact() {
    let dir = tmp("manifest-json");
    let a = run_json(&dir, &["mcp", fixture().to_str().unwrap(), "--json"]);

    // Documented mcp --json keys all still present (additive contract).
    let s = &a["servers"][0];
    for key in [
        "name",
        "matched_id",
        "display",
        "transport",
        "tools",
        "cold_tokens",
        "scenarios",
        "cli_alternative",
        "recommendation",
        "note",
        "savings_tokens",
        "savings_usd",
        "slim",
    ] {
        assert!(!s[key].is_null() || s.get(key).is_some(), "missing {key}");
    }
    for key in ["cold", "warm", "tool_search", "capacity"] {
        assert!(s["scenarios"][key].is_u64(), "missing scenarios.{key}");
    }

    // New keys: basis and tools_detail.
    assert_eq!(
        s["basis"].as_str().unwrap(),
        "tokenized manifest (o200k_base)"
    );
    let detail = &s["tools_detail"];
    assert_eq!(detail["tool_count"].as_u64().unwrap(), 4);
    let total = detail["total_tokens"].as_u64().unwrap();
    assert!(total > 0);
    // The per-tool counts sum exactly to the total: no blended figures.
    let sum: u64 = detail["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["tokens"].as_u64().unwrap())
        .sum();
    assert_eq!(sum, total);
    // capacity carries the exact base total; cold applies the provider
    // multiplier on top of the same exact base.
    assert_eq!(s["scenarios"]["capacity"].as_u64().unwrap(), total);
    assert_eq!(a["totals"]["measured"].as_u64().unwrap(), 1);

    // Smells: the fixture plants one no-verb description and one
    // near-duplicate pair.
    let rules: Vec<&str> = detail["smells"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["rule"].as_str().unwrap())
        .collect();
    assert!(rules.contains(&"no-leading-purpose-verb"), "{rules:?}");
    assert!(rules.contains(&"near-duplicate-descriptions"), "{rules:?}");
}

#[test]
fn tools_list_flag_supersedes_catalog_in_config() {
    let dir = tmp("flag-config");
    let config = dir.join("config.json");
    fs::write(
        &config,
        r#"{ "mcpServers": { "tracker": { "command": "node", "args": ["./tracker.js"] } } }"#,
    )
    .unwrap();

    // Single-server config: no --server needed.
    let a = run_json(
        &dir,
        &[
            "mcp",
            config.to_str().unwrap(),
            "--tools-list",
            fixture().to_str().unwrap(),
            "--json",
        ],
    );
    let s = &a["servers"][0];
    assert_eq!(s["name"].as_str().unwrap(), "tracker");
    assert_eq!(
        s["basis"].as_str().unwrap(),
        "tokenized manifest (o200k_base)"
    );
    assert!(s["cold_tokens"].as_u64().unwrap() > 0);
    // Unknown-but-measured: counted in the totals, not excluded.
    assert_eq!(a["totals"]["measured"].as_u64().unwrap(), 1);
    assert!(a["totals"]["capacity"].as_u64().unwrap() > 0);
    let notes: Vec<&str> = a["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert!(
        !notes.iter().any(|n| n.contains("excluded from the totals")),
        "{notes:?}"
    );
}

#[test]
fn tools_list_flag_requires_server_choice_in_multi_server_config() {
    let dir = tmp("flag-multi");
    let config = dir.join("config.json");
    fs::write(
        &config,
        r#"{ "mcpServers": {
            "github": { "command": "npx", "args": ["@modelcontextprotocol/server-github"] },
            "tracker": { "command": "node", "args": ["./tracker.js"] }
        } }"#,
    )
    .unwrap();

    // Without --server: a hard error naming the candidates.
    let out = cmd(&dir)
        .args([
            "mcp",
            config.to_str().unwrap(),
            "--tools-list",
            fixture().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--server"), "{err}");
    assert!(err.contains("github") && err.contains("tracker"), "{err}");

    // With --server: the manifest attaches to the named server only and the
    // other keeps its catalog basis. Mixed analyses carry both basis notes.
    let a = run_json(
        &dir,
        &[
            "mcp",
            config.to_str().unwrap(),
            "--tools-list",
            fixture().to_str().unwrap(),
            "--server",
            "tracker",
            "--json",
        ],
    );
    let servers = a["servers"].as_array().unwrap();
    let tracker = servers
        .iter()
        .find(|s| s["name"] == "tracker")
        .expect("tracker present");
    let github = servers
        .iter()
        .find(|s| s["name"] == "github")
        .expect("github present");
    assert!(tracker["basis"]
        .as_str()
        .unwrap()
        .starts_with("tokenized manifest"));
    assert_eq!(
        github["basis"].as_str().unwrap(),
        "catalog estimate (representative)"
    );
    assert!(github.get("tools_detail").is_none());
    let notes: Vec<&str> = a["notes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_str().unwrap())
        .collect();
    assert!(notes.iter().any(|n| n.contains("representative estimates")));
    assert!(notes.iter().any(|n| n.contains("never blended")));

    // Misnamed --server: hard error.
    let out = cmd(&dir)
        .args([
            "mcp",
            config.to_str().unwrap(),
            "--tools-list",
            fixture().to_str().unwrap(),
            "--server",
            "nope",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[test]
fn manifest_via_stdin_works() {
    use std::io::Write;
    use std::process::Stdio;

    let dir = tmp("stdin");
    let manifest = fs::read_to_string(fixture()).unwrap();
    let mut child = cmd(&dir)
        .args(["mcp", "--tools-list", "-", "--server", "tracker", "--json"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(manifest.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let a: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(a["servers"][0]["name"].as_str().unwrap(), "tracker");
    assert_eq!(a["totals"]["measured"].as_u64().unwrap(), 1);
}

#[test]
fn unknown_server_note_points_at_the_flag() {
    let dir = tmp("unknown-note");
    let config = dir.join("config.json");
    fs::write(
        &config,
        r#"{ "mcpServers": { "homegrown": { "command": "node", "args": ["./srv.js"] } } }"#,
    )
    .unwrap();
    let a = run_json(&dir, &["mcp", config.to_str().unwrap(), "--json"]);
    let note = a["servers"][0]["note"].as_str().unwrap();
    assert!(note.contains("--tools-list"), "{note}");
}

#[test]
fn anthropic_provider_labels_manifest_counts_an_estimate() {
    let dir = tmp("anthropic-label");
    let a = run_json(
        &dir,
        &[
            "mcp",
            fixture().to_str().unwrap(),
            "--provider",
            "anthropic",
            "--json",
        ],
    );
    let basis = a["servers"][0]["basis"].as_str().unwrap();
    assert!(basis.contains("cl100k_base proxy"), "{basis}");
    assert!(basis.contains("estimate"), "{basis}");
}
