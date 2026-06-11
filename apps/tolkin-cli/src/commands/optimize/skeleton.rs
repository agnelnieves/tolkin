//! Deterministic summary (the "skeleton") for `tolkin optimize`.
//!
//! Built purely from the project library's report. It is identical with or
//! without a sidecar present: the model narrates this surface, never the
//! other way around. Keep every field deterministic; nothing here may depend
//! on probe results, consent, or wall-clock time.

use serde::Serialize;

use crate::project::ProjectReport;

#[derive(Clone, Debug, Serialize)]
pub struct SkeletonTotals {
    pub files_scanned: u64,
    pub context_files: usize,
    pub context_tokens: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SkeletonHeavy {
    pub path: String,
    pub tokens: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct SkeletonRule {
    pub rule: String,
    pub count: usize,
}

/// The deterministic optimize summary. Serialized compactly as the narrate
/// task's entire model input, and rendered for humans at the top of every
/// optimize run.
#[derive(Clone, Debug, Serialize)]
pub struct Skeleton {
    pub totals: SkeletonTotals,
    pub heaviest: Vec<SkeletonHeavy>,
    pub findings_by_rule: Vec<SkeletonRule>,
    pub mcp_servers: usize,
    pub pointer: String,
}

/// Build the skeleton from a project report. Top five heaviest only; the
/// full list stays with `tolkin project`.
pub fn build(report: &ProjectReport) -> Skeleton {
    let heaviest = report
        .heaviest
        .iter()
        .take(5)
        .map(|h| SkeletonHeavy {
            path: h.path.clone(),
            tokens: h.tokens,
        })
        .collect();
    let findings_by_rule = report
        .findings_by_rule
        .iter()
        .map(|r| SkeletonRule {
            rule: r.rule.clone(),
            count: r.count,
        })
        .collect();
    let mcp_servers = report.mcp_configs.iter().map(|m| m.servers).sum();
    Skeleton {
        totals: SkeletonTotals {
            files_scanned: report.totals.files_scanned,
            context_files: report.totals.context_files,
            context_tokens: report.totals.context_tokens,
        },
        heaviest,
        findings_by_rule,
        mcp_servers,
        pointer: "full deterministic detail: tolkin project and tolkin scan".to_string(),
    }
}

impl Skeleton {
    /// Compact JSON used as the narrate task's model input and as the
    /// reference set for the number-faithfulness gate. Field order is fixed
    /// by the struct definition, so the same skeleton always serializes to
    /// the same byte string.
    pub fn to_compact_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Human rendering. Mirrors the project command's plain style.
    pub fn render_human(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();
        let _ = writeln!(out, "Deterministic summary");
        let _ = writeln!(
            out,
            "  {} files scanned, {} in agent context (~{} tokens)",
            commas(self.totals.files_scanned),
            self.totals.context_files,
            commas(self.totals.context_tokens)
        );
        if self.heaviest.is_empty() {
            let _ = writeln!(out, "  heaviest files: none found");
        } else {
            let _ = writeln!(out, "  heaviest files (top {})", self.heaviest.len());
            let width = self
                .heaviest
                .iter()
                .map(|h| h.path.chars().count())
                .max()
                .unwrap_or(0);
            for h in &self.heaviest {
                let _ = writeln!(
                    out,
                    "    {:<width$}  {:>8} tokens",
                    h.path,
                    commas(h.tokens)
                );
            }
        }
        if self.findings_by_rule.is_empty() {
            let _ = writeln!(out, "  findings: none; agent context is lean");
        } else {
            let _ = writeln!(out, "  findings by rule");
            for r in &self.findings_by_rule {
                let noun = if r.count == 1 { "finding" } else { "findings" };
                let _ = writeln!(out, "    {}  {} {noun}", r.rule, r.count);
            }
        }
        let _ = writeln!(out, "  MCP servers: {}", self.mcp_servers);
        let _ = writeln!(out, "  {}", self.pointer);
        out
    }
}

fn commas(n: u64) -> String {
    let s = n.to_string();
    let len = s.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in s.bytes().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(char::from(b));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{HeavyFile, McpConfigEntry, Profiles, ProjectReport, RuleRollup, Totals};

    fn fixture_report() -> ProjectReport {
        ProjectReport {
            root: "/repo".to_string(),
            profiles: Profiles::default(),
            mcp_configs: vec![McpConfigEntry {
                path: ".mcp.json".to_string(),
                servers: 3,
                cold_tokens: 4_910,
            }],
            heaviest: vec![
                HeavyFile {
                    path: "CLAUDE.md".to_string(),
                    tokens: 3_816,
                    findings: 1,
                    savings_min: 480,
                    savings_max: 1_100,
                },
                HeavyFile {
                    path: ".claude/skills/demo/SKILL.md".to_string(),
                    tokens: 1_204,
                    findings: 0,
                    savings_min: 0,
                    savings_max: 0,
                },
            ],
            findings_by_rule: vec![RuleRollup {
                rule: "near-duplicate-paragraphs".to_string(),
                count: 2,
                savings_min: 480,
                savings_max: 1_100,
            }],
            secret_files: Vec::new(),
            totals: Totals {
                files_scanned: 214,
                context_files: 4,
                context_tokens: 14_989,
                savings_min: 480,
                savings_max: 1_100,
            },
            warnings: Vec::new(),
        }
    }

    #[test]
    fn same_fixture_twice_yields_identical_compact_json() {
        let a = build(&fixture_report()).to_compact_json();
        let b = build(&fixture_report()).to_compact_json();
        assert_eq!(a, b, "skeleton JSON must be stable across builds");
        assert!(a.contains("\"files_scanned\":214"));
        assert!(a.contains("\"mcp_servers\":3"));
    }

    #[test]
    fn build_takes_top_five_heaviest_and_sums_servers() {
        let mut report = fixture_report();
        for i in 0..9 {
            report.heaviest.push(HeavyFile {
                path: format!("file-{i}.md"),
                tokens: 10 + i,
                findings: 0,
                savings_min: 0,
                savings_max: 0,
            });
        }
        report.mcp_configs.push(McpConfigEntry {
            path: ".cursor/mcp.json".to_string(),
            servers: 2,
            cold_tokens: 100,
        });
        let s = build(&report);
        assert_eq!(s.heaviest.len(), 5);
        assert_eq!(s.mcp_servers, 5);
    }

    #[test]
    fn render_human_shows_totals_heaviest_rules_and_pointer() {
        let s = build(&fixture_report());
        let out = s.render_human();
        assert!(out.contains("214 files scanned, 4 in agent context (~14,989 tokens)"));
        assert!(out.contains("heaviest files (top 2)"));
        assert!(out.contains("CLAUDE.md"));
        assert!(out.contains("near-duplicate-paragraphs  2 findings"));
        assert!(out.contains("MCP servers: 3"));
        assert!(out.contains("tolkin project and tolkin scan"));
    }
}
