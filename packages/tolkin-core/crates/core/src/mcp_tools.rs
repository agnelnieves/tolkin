//! tools/list ingestion: per-tool exact analysis for an MCP server inventory.
//!
//! This module is the "inside the server" half of the MCP analyzer. The config
//! analyzer in `mcp.rs` prices servers from a curated catalog of representative
//! estimates; this module accepts a server's real `tools/list` output and
//! produces exact per-tool token data plus description-smell lints.
//!
//! Tokenization is NOT done here (the core carries no tokenizer; see lib.rs).
//! The flow is two-phase by design:
//!
//! 1. [`parse_tools_list`] turns raw tools/list JSON into [`ToolSpec`]s, each
//!    carrying the canonical compact serialization the platform should count.
//! 2. The platform (CLI: tiktoken-rs; browser: gpt-tokenizer) counts each
//!    serialization and each description, then calls
//!    [`analyze_tool_inventory`] with the pre-tokenized [`ToolTokenCount`]s.
//!
//! Counts produced this way are labeled "tokenized manifest (<tokenizer>)" and
//! supersede the catalog's representative estimates wherever both exist.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mcp;
use crate::Provider;

/// Token budget for a single tool description before the over-long lint fires.
///
/// Justification: the analyzer's own Tool Search math budgets roughly 600
/// tokens for an ENTIRE deferred-loaded tool (name + description + schema; see
/// `mcp::scenarios`). A description alone spending a third of that all-in
/// budget on prose, before the schema is even counted, has outgrown the
/// selection job descriptions exist for. The research framing backs the
/// direction: a public study of 856 MCP tools found 97.1 percent had
/// description smells, and the documented failure mode of verbose descriptions
/// is more steps and higher per-request cost, not better task success.
/// Anthropic's tool-use guidance recommends detailed descriptions of a few
/// sentences (roughly 60 to 100 tokens), so 200 leaves generous headroom above
/// the recommended floor; this lint never argues for cutting below clarity.
const DESCRIPTION_TOKEN_BUDGET: u64 = 200;

/// Descriptions shorter than this skip the MinHash near-duplicate pass (too
/// few shingles for a stable estimate). Exact duplicates are still caught at
/// any length.
const MIN_MINHASH_DESC_BYTES: usize = 60;

/// Estimated-Jaccard threshold for "near-duplicate descriptions". Same value
/// the audit engine uses for near-duplicate paragraphs.
const DESC_JACCARD_THRESHOLD: f64 = 0.7;

/// Leading verbs that signal a purpose-first description. Heuristic by
/// construction: a fixed list cannot be complete, and a description can be
/// clear without a leading verb, so the lint says so in its note.
static PURPOSE_VERBS: &[&str] = &[
    "add",
    "aggregate",
    "analyze",
    "append",
    "approve",
    "archive",
    "assign",
    "attach",
    "batch",
    "browse",
    "build",
    "calculate",
    "call",
    "cancel",
    "capture",
    "check",
    "clear",
    "click",
    "clone",
    "close",
    "comment",
    "compare",
    "compute",
    "configure",
    "convert",
    "copy",
    "count",
    "create",
    "delete",
    "deploy",
    "describe",
    "detach",
    "disable",
    "dismiss",
    "download",
    "drop",
    "echo",
    "edit",
    "enable",
    "estimate",
    "execute",
    "extract",
    "fetch",
    "filter",
    "find",
    "follow",
    "fork",
    "format",
    "generate",
    "get",
    "grant",
    "group",
    "insert",
    "install",
    "invoke",
    "kill",
    "label",
    "link",
    "list",
    "load",
    "lock",
    "make",
    "manage",
    "mark",
    "merge",
    "move",
    "navigate",
    "open",
    "parse",
    "pause",
    "ping",
    "play",
    "post",
    "print",
    "publish",
    "pull",
    "push",
    "query",
    "react",
    "read",
    "record",
    "refresh",
    "register",
    "reject",
    "remove",
    "rename",
    "replace",
    "reply",
    "request",
    "reset",
    "resolve",
    "restore",
    "resume",
    "retrieve",
    "return",
    "revoke",
    "run",
    "sample",
    "save",
    "schedule",
    "search",
    "send",
    "set",
    "show",
    "sort",
    "spawn",
    "star",
    "start",
    "stop",
    "stream",
    "submit",
    "summarize",
    "switch",
    "sync",
    "tag",
    "take",
    "test",
    "toggle",
    "translate",
    "trigger",
    "truncate",
    "type",
    "uninstall",
    "unlink",
    "unlock",
    "update",
    "upload",
    "use",
    "validate",
    "verify",
    "view",
    "wait",
    "watch",
    "write",
];

/// Substrings (lowercase) that count as "the description mentions its output
/// shape". Deliberately broad so the lint stays conservative: when in doubt,
/// do not flag.
static OUTPUT_SHAPE_MARKERS: &[&str] = &[
    "return",
    "respond",
    "response",
    "output",
    "result",
    "format",
    "json",
    "markdown",
    "yield",
    "produce",
    "emit",
    "list of",
    "array of",
    "as text",
    "as a string",
    "as html",
    "gives back",
];

/// One tool parsed out of a tools/list payload, with the canonical compact
/// serialization the platform tokenizer should count.
#[derive(Clone, Debug, Serialize)]
pub struct ToolSpec {
    pub name: String,
    /// Empty string when the tool declares no description.
    pub description: String,
    /// Compact JSON of `{name, description, input_schema}` in that field
    /// order. `input_schema` object keys serialize in canonical sorted order
    /// (serde_json default), so the same manifest content always yields the
    /// same bytes regardless of upstream key order. This is the string to
    /// tokenize.
    pub serialized: String,
}

/// One tool with the platform-supplied token counts. Input to
/// [`analyze_tool_inventory`]; the core never counts tokens itself.
#[derive(Clone, Debug, Deserialize)]
pub struct ToolTokenCount {
    pub name: String,
    /// Description text, needed for the smell lints.
    #[serde(default)]
    pub description: String,
    /// Token count of the full canonical serialization (see [`ToolSpec`]).
    pub tokens: u64,
    /// Token count of the description string alone.
    #[serde(default)]
    pub description_tokens: u64,
}

/// Per-tool table row: exact tokens and share of the server total.
#[derive(Clone, Debug, Serialize)]
pub struct ToolRow {
    pub name: String,
    pub tokens: u64,
    pub description_tokens: u64,
    /// Share of the server's total tool-definition tokens, percent, 2 dp.
    pub share_pct: f64,
}

/// One description smell across one or more tools.
#[derive(Clone, Debug, Serialize)]
pub struct ToolSmell {
    /// Kebab-case rule id.
    pub rule: String,
    /// Tools the smell applies to, in manifest order.
    pub tools: Vec<String>,
    pub detail: String,
    /// Compact-clarity stance: clarify or shorten, never lengthen.
    pub recommendation: String,
}

/// Exact per-tool analysis of one server's tool inventory.
#[derive(Clone, Debug, Serialize)]
pub struct ToolInventory {
    /// Tokenizer label supplied by the platform, e.g. "o200k_base".
    pub tokenizer: String,
    /// Basis label for every count here: "tokenized manifest (<tokenizer>)".
    pub basis: String,
    pub tool_count: u32,
    /// Exact sum of the per-tool serialization counts.
    pub total_tokens: u64,
    /// Rows sorted by tokens descending (name ascending on ties).
    pub tools: Vec<ToolRow>,
    pub smells: Vec<ToolSmell>,
    pub notes: Vec<String>,
}

/// Request shape for surfaces that pass the whole inventory as one JSON
/// payload (the WASM binding). `provider` defaults to Anthropic, matching the
/// config analyzer's default cache math.
#[derive(Debug, Deserialize)]
pub struct InventoryRequest {
    pub server: String,
    #[serde(default)]
    pub provider: Option<Provider>,
    pub tokenizer: String,
    pub tools: Vec<ToolTokenCount>,
}

/// True when `text` parses as a tools/list payload: a bare array of tool
/// objects, a `{"tools": [...]}` object (the MCP tools/list result shape), or
/// a full JSON-RPC envelope with `result.tools`. Returns false for anything
/// carrying an MCP client-config key (mcpServers, servers, context_servers,
/// mcp), so config detection always wins.
pub fn is_tools_list(text: &str) -> bool {
    let cleaned = mcp::strip_jsonc(text);
    let Ok(value) = serde_json::from_str::<Value>(&cleaned) else {
        return false;
    };
    if let Some(obj) = value.as_object() {
        if ["mcpServers", "servers", "context_servers", "mcp"]
            .iter()
            .any(|k| obj.contains_key(*k))
        {
            return false;
        }
    }
    locate_tools(&value).is_some()
}

fn locate_tools(value: &Value) -> Option<&Vec<Value>> {
    let arr = if let Some(arr) = value.as_array() {
        arr
    } else if let Some(arr) = value.get("tools").and_then(Value::as_array) {
        arr
    } else if let Some(arr) = value
        .get("result")
        .and_then(|r| r.get("tools"))
        .and_then(Value::as_array)
    {
        arr
    } else {
        return None;
    };
    // Every entry must be an object with a string name; otherwise this is
    // some other JSON that happens to carry a "tools" key.
    let all_named = !arr.is_empty()
        && arr
            .iter()
            .all(|t| t.get("name").and_then(Value::as_str).is_some());
    if all_named {
        Some(arr)
    } else {
        None
    }
}

/// The serialization contract: field order name, description, input_schema.
/// serde serializes struct fields in declaration order, which pins the order
/// without a preserve_order feature. The schema value itself comes from
/// serde_json's default map (BTreeMap), so nested keys are in canonical
/// sorted order: deterministic for identical content regardless of upstream
/// key order, and disclosed in the analysis notes.
#[derive(Serialize)]
struct CanonicalTool<'a> {
    name: &'a str,
    description: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    input_schema: Option<&'a Value>,
}

/// Parse a tools/list payload into [`ToolSpec`]s. Accepts the same three
/// shapes as [`is_tools_list`] plus JSONC comments and trailing commas (paste
/// resilience). Accepts both `inputSchema` (MCP wire form) and `input_schema`
/// (Anthropic API form) per tool; extra fields (title, annotations,
/// outputSchema) are dropped from the canonical serialization because clients
/// do not forward them to the model.
pub fn parse_tools_list(text: &str) -> Result<Vec<ToolSpec>, String> {
    let cleaned = mcp::strip_jsonc(text);
    let value: Value = serde_json::from_str(&cleaned)
        .map_err(|e| format!("could not parse tools/list as JSON: {e}"))?;
    let arr = locate_tools(&value).ok_or_else(|| {
        "no tools found (expected {\"tools\": [...]}, a bare array of tool objects, or a JSON-RPC envelope with result.tools)"
            .to_string()
    })?;

    let mut specs = Vec::with_capacity(arr.len());
    for (i, tool) in arr.iter().enumerate() {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("tool at index {i} has no string name"))?;
        let description = tool
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let schema = tool.get("inputSchema").or_else(|| tool.get("input_schema"));
        let canonical = CanonicalTool {
            name,
            description,
            input_schema: schema,
        };
        let serialized = serde_json::to_string(&canonical)
            .map_err(|e| format!("could not serialize tool {name}: {e}"))?;
        specs.push(ToolSpec {
            name: name.to_string(),
            description: description.to_string(),
            serialized,
        });
    }
    Ok(specs)
}

/// Analyze a pre-tokenized tool inventory: per-tool table, exact totals, and
/// description-smell lints. `tokenizer_label` names the tokenizer the caller
/// used (e.g. "o200k_base") and is embedded in the basis label.
pub fn analyze_tool_inventory(
    tools: &[ToolTokenCount],
    tokenizer_label: &str,
) -> Result<ToolInventory, String> {
    if tools.is_empty() {
        return Err("the tool inventory is empty (no tools to analyze)".to_string());
    }

    let total_tokens: u64 = tools.iter().map(|t| t.tokens).sum();
    let mut rows: Vec<ToolRow> = tools
        .iter()
        .map(|t| {
            let share = if total_tokens == 0 {
                0.0
            } else {
                (t.tokens as f64 / total_tokens as f64 * 100.0 * 100.0).round() / 100.0
            };
            ToolRow {
                name: t.name.clone(),
                tokens: t.tokens,
                description_tokens: t.description_tokens,
                share_pct: share,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.tokens.cmp(&a.tokens).then(a.name.cmp(&b.name)));

    let mut smells = Vec::new();
    lint_leading_verb(tools, &mut smells);
    lint_output_shape(tools, &mut smells);
    lint_over_long(tools, &mut smells);
    lint_near_duplicates(tools, &mut smells);

    let basis = format!("tokenized manifest ({tokenizer_label})");
    let notes = vec![
        format!(
            "Token counts are exact counts of this manifest under {tokenizer_label}: each tool serialized compactly as {{name, description, input_schema}} with schema keys in canonical sorted order. A specific client's wire bytes can differ by a few tokens (field order, dropped extras like title and annotations)."
        ),
        "Description guidance is compact-clarity: better descriptions can help task success, but verbose augmentation increases steps and cost. Rewrites should clarify or shorten, never lengthen.".to_string(),
        "Description smells are heuristics (fixed verb and keyword lists, MinHash near-duplicate estimate); review before acting.".to_string(),
    ];

    Ok(ToolInventory {
        tokenizer: tokenizer_label.to_string(),
        basis,
        tool_count: u32::try_from(tools.len()).unwrap_or(u32::MAX),
        total_tokens,
        tools: rows,
        smells,
        notes,
    })
}

/// First alphabetic word of `text`, lowercase.
fn first_word(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    let word: String = trimmed
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if word.is_empty() {
        None
    } else {
        Some(word.to_ascii_lowercase())
    }
}

/// True when `word` (already lowercase) is a purpose verb, allowing the
/// third-person form ("lists" matches "list", "searches" matches "search").
fn is_purpose_verb(word: &str) -> bool {
    if PURPOSE_VERBS.binary_search(&word).is_ok() {
        return true;
    }
    if let Some(stem) = word.strip_suffix("es") {
        if PURPOSE_VERBS.binary_search(&stem).is_ok() {
            return true;
        }
    }
    if let Some(stem) = word.strip_suffix('s') {
        if PURPOSE_VERBS.binary_search(&stem).is_ok() {
            return true;
        }
    }
    false
}

fn lint_leading_verb(tools: &[ToolTokenCount], smells: &mut Vec<ToolSmell>) {
    let flagged: Vec<String> = tools
        .iter()
        .filter(|t| {
            !t.description.trim().is_empty()
                && !first_word(&t.description).is_some_and(|w| is_purpose_verb(&w))
        })
        .map(|t| t.name.clone())
        .collect();
    // Tools with no description at all are a different problem (the model has
    // only the name and schema to go on); flag them under this rule too, with
    // their own wording, since "lead with the purpose" is exactly what they
    // lack.
    let missing: Vec<String> = tools
        .iter()
        .filter(|t| t.description.trim().is_empty())
        .map(|t| t.name.clone())
        .collect();

    if !flagged.is_empty() {
        smells.push(ToolSmell {
            rule: "no-leading-purpose-verb".to_string(),
            tools: flagged.clone(),
            detail: format!(
                "{} tool description(s) do not lead with a purpose verb (heuristic: first word checked against a fixed verb list; a description can be clear without one).",
                flagged.len()
            ),
            recommendation: "Rewrite the first clause to lead with the action (verb first, object second). Rephrase within the existing length; do not lengthen.".to_string(),
        });
    }
    if !missing.is_empty() {
        smells.push(ToolSmell {
            rule: "missing-description".to_string(),
            tools: missing.clone(),
            detail: format!(
                "{} tool(s) declare no description; the model selects on name and schema alone.",
                missing.len()
            ),
            recommendation: "Add one compact sentence: purpose verb, object, output shape. Compact clarity, not augmentation.".to_string(),
        });
    }
}

fn lint_output_shape(tools: &[ToolTokenCount], smells: &mut Vec<ToolSmell>) {
    let flagged: Vec<String> = tools
        .iter()
        .filter(|t| {
            let d = t.description.to_ascii_lowercase();
            !d.trim().is_empty() && !OUTPUT_SHAPE_MARKERS.iter().any(|m| d.contains(m))
        })
        .map(|t| t.name.clone())
        .collect();
    if flagged.is_empty() {
        return;
    }
    smells.push(ToolSmell {
        rule: "no-output-shape".to_string(),
        tools: flagged.clone(),
        detail: format!(
            "{} tool description(s) never mention what comes back (no return / output / response / format language).",
            flagged.len()
        ),
        recommendation: "Name the output shape in one clause (what the tool returns and in what form). Rephrase within the existing length; do not lengthen.".to_string(),
    });
}

fn lint_over_long(tools: &[ToolTokenCount], smells: &mut Vec<ToolSmell>) {
    let flagged: Vec<(String, u64)> = tools
        .iter()
        .filter(|t| t.description_tokens > DESCRIPTION_TOKEN_BUDGET)
        .map(|t| (t.name.clone(), t.description_tokens))
        .collect();
    if flagged.is_empty() {
        return;
    }
    let names: Vec<String> = flagged.iter().map(|(n, _)| n.clone()).collect();
    let worst = flagged.iter().map(|(_, t)| *t).max().unwrap_or(0);
    smells.push(ToolSmell {
        rule: "over-long-description".to_string(),
        tools: names.clone(),
        detail: format!(
            "{} description(s) exceed the {DESCRIPTION_TOKEN_BUDGET}-token budget (worst: {worst} tokens). The budget is a third of the analyzer's ~600-token all-in cost for a deferred-loaded tool; prose alone past that point has outgrown the selection job.",
            names.len()
        ),
        recommendation: "Cut to the selection-relevant facts: purpose, key constraints, output shape. Verbose descriptions raise every request's cost and can add steps; compact clarity outperforms augmentation.".to_string(),
    });
}

fn lint_near_duplicates(tools: &[ToolTokenCount], smells: &mut Vec<ToolSmell>) {
    let n = tools.len();
    if n < 2 {
        return;
    }
    // Pairwise similarity: exact match (trimmed, case-insensitive) at any
    // length; MinHash estimated Jaccard for descriptions long enough to
    // shingle. Reuses the audit engine's MinHash machinery (same seeds, same
    // estimator) so the two surfaces agree on what "near-duplicate" means.
    let normalized: Vec<String> = tools
        .iter()
        .map(|t| t.description.trim().to_ascii_lowercase())
        .collect();
    let sigs: Vec<Option<crate::audit::MinHashSignature>> = tools
        .iter()
        .map(|t| {
            let d = t.description.trim();
            if d.len() >= MIN_MINHASH_DESC_BYTES {
                Some(crate::audit::minhash_signature(d.as_bytes()))
            } else {
                None
            }
        })
        .collect();

    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for i in 0..n {
        if normalized[i].is_empty() {
            continue;
        }
        for j in (i + 1)..n {
            let exact = normalized[i] == normalized[j] && !normalized[j].is_empty();
            let near = match (&sigs[i], &sigs[j]) {
                (Some(a), Some(b)) => {
                    crate::audit::estimated_jaccard(a, b) >= DESC_JACCARD_THRESHOLD
                }
                _ => false,
            };
            if exact || near {
                let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                if ri != rj {
                    parent[ri] = rj;
                }
            }
        }
    }

    use std::collections::BTreeMap;
    let mut clusters: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..n {
        let root = find(&mut parent, i);
        clusters.entry(root).or_default().push(i);
    }
    // Sort clusters by their first member so output order follows the
    // manifest, not union-find root identity.
    let mut groups: Vec<Vec<usize>> = clusters.into_values().filter(|m| m.len() >= 2).collect();
    groups.sort_by_key(|m| m[0]);

    for members in groups {
        let names: Vec<String> = members.iter().map(|&i| tools[i].name.clone()).collect();
        smells.push(ToolSmell {
            rule: "near-duplicate-descriptions".to_string(),
            tools: names.clone(),
            detail: format!(
                "{} tools share near-duplicate descriptions (estimated Jaccard >= {DESC_JACCARD_THRESHOLD} on 5-byte shingles, or exact match): {}.",
                names.len(),
                names.join(", ")
            ),
            recommendation: "Differentiate each description by the tool's distinct behavior in a few words, or merge overlapping tools. Near-identical descriptions make selection ambiguous; rephrase, do not lengthen.".to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(name: &str, description: &str, tokens: u64, description_tokens: u64) -> ToolTokenCount {
        ToolTokenCount {
            name: name.to_string(),
            description: description.to_string(),
            tokens,
            description_tokens,
        }
    }

    #[test]
    fn purpose_verb_list_is_sorted_for_binary_search() {
        let mut sorted = PURPOSE_VERBS.to_vec();
        sorted.sort_unstable();
        assert_eq!(PURPOSE_VERBS, sorted.as_slice());
    }

    #[test]
    fn parses_tools_object_shape() {
        let json = r#"{ "tools": [
            { "name": "echo", "description": "Echoes back the input. Returns the message.", "inputSchema": { "type": "object", "properties": { "message": { "type": "string" } } } }
        ] }"#;
        let specs = parse_tools_list(json).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "echo");
        // Canonical field order: name, description, input_schema.
        assert!(specs[0]
            .serialized
            .starts_with(r#"{"name":"echo","description":"#));
        assert!(specs[0].serialized.contains(r#""input_schema":{"#));
        // Compact: no pretty-print spaces after separators.
        assert!(!specs[0].serialized.contains(": \""));
    }

    #[test]
    fn parses_bare_array_and_jsonrpc_envelope() {
        let bare = r#"[ { "name": "a", "description": "Get a thing." } ]"#;
        assert_eq!(parse_tools_list(bare).unwrap().len(), 1);

        let envelope = r#"{ "jsonrpc": "2.0", "id": 2, "result": { "tools": [
            { "name": "b", "description": "List things.", "inputSchema": { "type": "object" } }
        ] } }"#;
        let specs = parse_tools_list(envelope).unwrap();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "b");
    }

    #[test]
    fn accepts_snake_case_input_schema() {
        let json = r#"{ "tools": [ { "name": "x", "description": "Run x.", "input_schema": { "type": "object" } } ] }"#;
        let specs = parse_tools_list(json).unwrap();
        assert!(specs[0]
            .serialized
            .contains(r#""input_schema":{"type":"object"}"#));
    }

    #[test]
    fn drops_title_and_annotations_from_serialization() {
        let json = r#"{ "tools": [ { "name": "x", "title": "X Tool", "description": "Run x.", "annotations": { "readOnlyHint": true } } ] }"#;
        let specs = parse_tools_list(json).unwrap();
        assert!(!specs[0].serialized.contains("title"));
        assert!(!specs[0].serialized.contains("annotations"));
        // No schema: input_schema key omitted entirely.
        assert!(!specs[0].serialized.contains("input_schema"));
    }

    #[test]
    fn schema_key_order_is_canonical() {
        let a = r#"{ "tools": [ { "name": "x", "description": "d", "inputSchema": { "b": 1, "a": 2 } } ] }"#;
        let b = r#"{ "tools": [ { "name": "x", "description": "d", "inputSchema": { "a": 2, "b": 1 } } ] }"#;
        assert_eq!(
            parse_tools_list(a).unwrap()[0].serialized,
            parse_tools_list(b).unwrap()[0].serialized
        );
    }

    #[test]
    fn detection_accepts_manifests_and_rejects_configs() {
        assert!(is_tools_list(r#"{ "tools": [ { "name": "a" } ] }"#));
        assert!(is_tools_list(r#"[ { "name": "a" } ]"#));
        assert!(is_tools_list(
            r#"{ "result": { "tools": [ { "name": "a" } ] } }"#
        ));
        // Config shapes are never manifests, even with a tools key elsewhere.
        assert!(!is_tools_list(
            r#"{ "mcpServers": { "x": { "command": "npx" } } }"#
        ));
        assert!(!is_tools_list(r#"{ "servers": { "x": {} } }"#));
        assert!(!is_tools_list(r#"{ "context_servers": { "x": {} } }"#));
        assert!(!is_tools_list(r#"{ "mcp": { "servers": { "x": {} } } }"#));
        // Empty or unnamed tools arrays are not manifests.
        assert!(!is_tools_list(r#"{ "tools": [] }"#));
        assert!(!is_tools_list(r#"{ "tools": [ { "id": 1 } ] }"#));
        assert!(!is_tools_list("not json"));
    }

    #[test]
    fn errors_when_no_tools() {
        assert!(parse_tools_list(r#"{ "foo": 1 }"#).is_err());
        assert!(analyze_tool_inventory(&[], "o200k_base").is_err());
    }

    #[test]
    fn totals_shares_and_sort_order() {
        let inv = analyze_tool_inventory(
            &[
                tool("small", "Get a thing. Returns JSON.", 250, 6),
                tool("big", "List things. Returns JSON.", 750, 6),
            ],
            "o200k_base",
        )
        .unwrap();
        assert_eq!(inv.total_tokens, 1000);
        assert_eq!(inv.tool_count, 2);
        assert_eq!(inv.basis, "tokenized manifest (o200k_base)");
        // Sorted by tokens descending.
        assert_eq!(inv.tools[0].name, "big");
        assert!((inv.tools[0].share_pct - 75.0).abs() < 1e-9);
        assert!((inv.tools[1].share_pct - 25.0).abs() < 1e-9);
        // The compact-clarity stance ships in the notes, verbatim direction.
        assert!(inv.notes.iter().any(|n| n.contains("never lengthen")));
    }

    #[test]
    fn lint_no_leading_purpose_verb() {
        let inv = analyze_tool_inventory(
            &[
                tool("good", "List repository issues. Returns JSON.", 100, 8),
                tool("good_third_person", "Lists branches. Returns JSON.", 100, 8),
                tool(
                    "bad",
                    "This tool is helpful for repository issues. Returns JSON.",
                    100,
                    12,
                ),
            ],
            "o200k_base",
        )
        .unwrap();
        let smell = inv
            .smells
            .iter()
            .find(|s| s.rule == "no-leading-purpose-verb")
            .expect("smell fires");
        assert_eq!(smell.tools, vec!["bad".to_string()]);
        assert!(smell.detail.contains("heuristic"));
        assert!(smell.recommendation.contains("do not lengthen"));
    }

    #[test]
    fn lint_missing_description() {
        let inv = analyze_tool_inventory(
            &[
                tool("named_only", "", 60, 0),
                tool("ok", "Get one item. Returns JSON.", 80, 7),
            ],
            "o200k_base",
        )
        .unwrap();
        let smell = inv
            .smells
            .iter()
            .find(|s| s.rule == "missing-description")
            .expect("smell fires");
        assert_eq!(smell.tools, vec!["named_only".to_string()]);
        // A missing description must not also fire the verb or shape lints.
        assert!(!inv
            .smells
            .iter()
            .any(|s| s.rule == "no-leading-purpose-verb"
                && s.tools.contains(&"named_only".to_string())));
        assert!(!inv
            .smells
            .iter()
            .any(|s| s.rule == "no-output-shape" && s.tools.contains(&"named_only".to_string())));
    }

    #[test]
    fn lint_no_output_shape() {
        let inv = analyze_tool_inventory(
            &[
                tool("shapeless", "Create an issue in the tracker.", 100, 7),
                tool(
                    "shaped",
                    "Create an issue. Returns the new issue id.",
                    100,
                    10,
                ),
                tool(
                    "shaped_format",
                    "Search code with results formatted as JSON.",
                    100,
                    9,
                ),
            ],
            "o200k_base",
        )
        .unwrap();
        let smell = inv
            .smells
            .iter()
            .find(|s| s.rule == "no-output-shape")
            .expect("smell fires");
        assert_eq!(smell.tools, vec!["shapeless".to_string()]);
    }

    #[test]
    fn lint_over_long_description() {
        let long_desc = "Fetch data. ".repeat(80);
        let inv = analyze_tool_inventory(
            &[
                tool("verbose", &long_desc, 400, 240),
                tool("terse", "Fetch data. Returns JSON.", 60, 6),
            ],
            "o200k_base",
        )
        .unwrap();
        let smell = inv
            .smells
            .iter()
            .find(|s| s.rule == "over-long-description")
            .expect("smell fires");
        assert_eq!(smell.tools, vec!["verbose".to_string()]);
        assert!(smell.detail.contains("200-token budget"));
        assert!(smell.detail.contains("240 tokens"));
        // The boundary itself does not flag.
        let at_budget = analyze_tool_inventory(
            &[tool(
                "edge",
                "Get x. Returns y.",
                250,
                DESCRIPTION_TOKEN_BUDGET,
            )],
            "o200k_base",
        )
        .unwrap();
        assert!(!at_budget
            .smells
            .iter()
            .any(|s| s.rule == "over-long-description"));
    }

    #[test]
    fn lint_near_duplicate_descriptions_minhash() {
        // Two long descriptions differing by one word; one distinct control.
        let a = "Read the complete contents of a file from the filesystem and return it as text with line numbers for display.";
        let b = "Read the complete contents of a file from the filesystem and return it as text with byte offsets for display.";
        let c = "Delete a branch from the remote repository after checking it is fully merged. Returns the deleted ref name.";
        let inv = analyze_tool_inventory(
            &[
                tool("read_a", a, 100, 22),
                tool("read_b", b, 100, 22),
                tool("del", c, 100, 20),
            ],
            "o200k_base",
        )
        .unwrap();
        let smell = inv
            .smells
            .iter()
            .find(|s| s.rule == "near-duplicate-descriptions")
            .expect("smell fires");
        assert_eq!(
            smell.tools,
            vec!["read_a".to_string(), "read_b".to_string()]
        );
    }

    #[test]
    fn lint_near_duplicate_exact_short_descriptions() {
        // Too short for MinHash, caught by the exact-match path.
        let inv = analyze_tool_inventory(
            &[
                tool("a", "Get the item. Returns JSON.", 50, 7),
                tool("b", "get the item. returns json.", 50, 7),
                tool("c", "Set the item. Returns JSON.", 50, 7),
            ],
            "o200k_base",
        )
        .unwrap();
        let smell = inv
            .smells
            .iter()
            .find(|s| s.rule == "near-duplicate-descriptions")
            .expect("smell fires");
        assert_eq!(smell.tools, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn clean_inventory_has_no_smells() {
        let inv = analyze_tool_inventory(
            &[
                tool(
                    "create_issue",
                    "Create an issue. Returns the new issue as JSON.",
                    120,
                    10,
                ),
                tool(
                    "list_branches",
                    "List branches in a repository. Returns an array of branch names.",
                    110,
                    12,
                ),
            ],
            "o200k_base",
        )
        .unwrap();
        assert!(inv.smells.is_empty(), "{:?}", inv.smells);
    }

    #[test]
    fn inventory_request_deserializes() {
        let req: InventoryRequest = serde_json::from_str(
            r#"{ "server": "github", "provider": "openai", "tokenizer": "o200k_base",
                "tools": [ { "name": "a", "description": "Get a. Returns JSON.", "tokens": 10, "description_tokens": 4 } ] }"#,
        )
        .unwrap();
        assert_eq!(req.server, "github");
        assert_eq!(req.provider, Some(Provider::OpenAi));
        assert_eq!(req.tools.len(), 1);
    }

    #[test]
    fn inventory_serializes_snake_case() {
        let inv = analyze_tool_inventory(&[tool("a", "Get a. Returns JSON.", 10, 4)], "o200k_base")
            .unwrap();
        let json = serde_json::to_string(&inv).unwrap();
        for key in [
            "\"tokenizer\"",
            "\"basis\"",
            "\"tool_count\"",
            "\"total_tokens\"",
            "\"tools\"",
            "\"share_pct\"",
            "\"description_tokens\"",
            "\"smells\"",
            "\"notes\"",
        ] {
            assert!(json.contains(key), "missing {key}");
        }
    }
}
