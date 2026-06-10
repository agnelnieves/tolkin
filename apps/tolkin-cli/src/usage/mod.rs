//! Local usage-log ingestion: pull-based readers over agent session logs.
//! See types.rs for the privacy posture.

use std::path::{Path, PathBuf};

pub mod cache;
pub mod claude_code;
pub mod codex;
pub mod cost;
pub mod types;

/// Default per-OS log directories for the two ingested sources.
///
/// Both providers happen to place their logs under the user's home on every
/// platform we ship to, so we resolve from the supplied home root rather than
/// from environment lookups. Callers that need to override either path (CI,
/// tests) build their own PathBufs.
pub fn default_dirs(home: &Path) -> (PathBuf, PathBuf) {
    let claude = home.join(".claude").join("projects");
    let codex = home.join(".codex").join("sessions");
    (claude, codex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dirs_joins_under_home() {
        let home = PathBuf::from("/tmp/h");
        let (claude, codex) = default_dirs(&home);
        assert_eq!(claude, PathBuf::from("/tmp/h/.claude/projects"));
        assert_eq!(codex, PathBuf::from("/tmp/h/.codex/sessions"));
    }
}
