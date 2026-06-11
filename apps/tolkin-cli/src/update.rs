//! Version update check: explicit `tolkin update` lookup against the npm
//! registry plus a consent-gated passive notice helper. Documented in
//! PRIVACY.md as the second sanctioned egress (a plain GET, no identifiers).
//!
//! Privacy posture: `check_now` issues exactly one outbound request, a plain
//! HTTPS GET to registry.npmjs.org with no cookies, no auth, no user-agent
//! beyond ureq defaults, and no query parameters that could carry an identity.
//! The request fires only when the user explicitly runs `tolkin update`
//! (explicit invocation = consent by action). Setting TOLKIN_NO_UPDATE_CHECK=1
//! suppresses even that.
//!
//! `passive_notice_line` is a pure function with zero IO. Other code wires it
//! to a persistent last-seen cache; this module only computes the string.

use std::cmp::Ordering;

// ---- public types -----------------------------------------------------------

/// All the information the `update` command surfaces.
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub channel: InstallChannel,
    pub instructions: Vec<String>,
}

/// How tolkin was installed, inferred from the executable path.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum InstallChannel {
    Homebrew,
    Npm,
    Source,
    Unknown,
}

impl InstallChannel {
    /// Stable string label used in JSON output and human text.
    pub fn as_str(&self) -> &'static str {
        match self {
            InstallChannel::Homebrew => "homebrew",
            InstallChannel::Npm => "npm",
            InstallChannel::Source => "source",
            InstallChannel::Unknown => "unknown",
        }
    }
}

// ---- channel detection ------------------------------------------------------

/// Classify an install channel from the running executable path string.
///
/// Precedence (first match wins):
///   1. Homebrew: path contains "/Cellar/", "/homebrew/", or "/linuxbrew/"
///   2. Npm: path contains "node_modules", "/.bun/", "/.npm", or "\\npm\\"
///      (Windows npm global path uses backslash separators)
///   3. Source: path contains "target/release", "target/debug",
///      "target\\release", or "target\\debug" (Windows cargo output paths
///      use backslash separators)
///   4. Unknown otherwise
pub fn classify_path(path: &str) -> InstallChannel {
    if path.contains("/Cellar/") || path.contains("/homebrew/") || path.contains("/linuxbrew/") {
        return InstallChannel::Homebrew;
    }
    if path.contains("node_modules")
        || path.contains("/.bun/")
        || path.contains("/.npm")
        || path.contains("\\npm\\")
    {
        return InstallChannel::Npm;
    }
    if path.contains("target/release")
        || path.contains("target/debug")
        || path.contains("target\\release")
        || path.contains("target\\debug")
    {
        return InstallChannel::Source;
    }
    InstallChannel::Unknown
}

fn detect_channel() -> InstallChannel {
    match std::env::current_exe() {
        Ok(p) => classify_path(&p.to_string_lossy()),
        Err(_) => InstallChannel::Unknown,
    }
}

// ---- semver comparison (numeric triple only) --------------------------------

/// Parse the leading "x.y.z" numeric triple from a semver string, ignoring
/// any pre-release suffix (everything after the first '-' or '+') and any
/// extra dot-separated components beyond the third.
///
/// Simplification: only the numeric triple is compared. Pre-release labels
/// such as "-beta.1" are stripped entirely. This is correct for release
/// comparisons where all published versions on the npm registry are stable
/// releases; it would be inaccurate if pre-releases were ever published.
fn parse_triple(v: &str) -> (u64, u64, u64) {
    // Strip pre-release / build metadata suffix.
    let base = match v.find(['-', '+']) {
        Some(pos) => &v[..pos],
        None => v,
    };
    let mut parts = base.splitn(4, '.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u64);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u64);
    let patch = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0u64);
    (major, minor, patch)
}

/// Compare two semver strings numerically by their x.y.z triple.
pub fn cmp_semver(a: &str, b: &str) -> Ordering {
    parse_triple(a).cmp(&parse_triple(b))
}

// ---- update instructions per channel ----------------------------------------

fn instructions_for(channel: &InstallChannel) -> Vec<String> {
    match channel {
        InstallChannel::Homebrew => vec!["brew update && brew upgrade tolkin".to_string()],
        InstallChannel::Npm => vec![
            "npm update -g tolkin-cli".to_string(),
            "  or: bun add -g tolkin-cli@latest".to_string(),
        ],
        InstallChannel::Source => vec![
            "git pull and rebuild: cargo build --release --manifest-path apps/tolkin-cli/Cargo.toml"
                .to_string(),
        ],
        InstallChannel::Unknown => vec![
            "brew update && brew upgrade tolkin".to_string(),
            "  or: npm update -g tolkin-cli".to_string(),
        ],
    }
}

// ---- network ----------------------------------------------------------------

const NPM_ENDPOINT: &str = "https://registry.npmjs.org/tolkin-cli/latest";

/// Fetch the latest published version of tolkin-cli from the npm registry.
///
/// Issues exactly one HTTPS GET with a 2-second connect timeout and a 4-second
/// overall read timeout. No authentication, no cookies, no identifying headers
/// beyond the ureq defaults.
pub fn fetch_latest() -> Result<String, String> {
    let resp = ureq::get(NPM_ENDPOINT)
        .timeout(std::time::Duration::from_secs(4))
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => {
                format!("npm registry returned HTTP {code} for tolkin-cli")
            }
            ureq::Error::Transport(t) => format!("network error checking for updates: {t}"),
        })?;

    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("failed to parse npm registry response: {e}"))?;

    json.get("version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "npm registry response did not contain a version field".to_string())
}

// ---- public API -------------------------------------------------------------

/// Check for a newer version of tolkin.
///
/// Issues one HTTPS GET to the npm registry (2s connect / 4s read timeout).
/// Returns Err when TOLKIN_NO_UPDATE_CHECK=1 is set, or on any network or
/// parse failure.
pub fn check_now() -> Result<UpdateStatus, String> {
    if std::env::var("TOLKIN_NO_UPDATE_CHECK").as_deref() == Ok("1") {
        return Err("update check disabled by TOLKIN_NO_UPDATE_CHECK".to_string());
    }

    let current = env!("CARGO_PKG_VERSION").to_string();
    let latest = fetch_latest()?;
    let update_available = cmp_semver(&latest, &current) == Ordering::Greater;
    let channel = detect_channel();
    let instructions = instructions_for(&channel);

    Ok(UpdateStatus {
        current,
        latest,
        update_available,
        channel,
        instructions,
    })
}

/// Decide whether the once-a-day passive refresh should fire. Pure.
fn should_refresh(consent: Option<bool>, last_check: Option<u64>, now: u64) -> bool {
    consent == Some(true) && now.saturating_sub(last_check.unwrap_or(0)) >= 86_400
}

/// Consent-gated passive update notice, run by main after a normal command.
///
/// Guards, in order: TOLKIN_NO_UPDATE_CHECK=1, ledger-disabling env (CI and
/// TOLKIN_NO_LEDGER, which also mean "leave no state"), missing config, and
/// consent_update_check anything but Some(true). When due (at most once per
/// 86400s), refreshes the cached latest version with the same single GET as
/// `tolkin update`; the attempt time is recorded even on network failure so
/// an offline machine is not stalled on every run. Returns the stderr notice
/// line, or None. Never prompts, never touches stdout.
pub fn passive_refresh_and_notice() -> Option<String> {
    if std::env::var("TOLKIN_NO_UPDATE_CHECK").as_deref() == Ok("1") {
        return None;
    }
    if crate::ledger::disabled_by_env() {
        return None;
    }
    let mut cfg = crate::ledger::load_config()?;
    if cfg.consent_update_check != Some(true) {
        return None;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    if should_refresh(cfg.consent_update_check, cfg.last_update_check, now) {
        if let Ok(latest) = fetch_latest() {
            cfg.last_seen_latest = Some(latest);
        }
        cfg.last_update_check = Some(now);
        let _ = crate::ledger::save_config(&cfg);
    }
    passive_notice_line(env!("CARGO_PKG_VERSION"), cfg.last_seen_latest.as_deref())
}

/// Pure helper for a passive update notice. No IO, no network.
///
/// Returns Some("tolkin {v} is available (you have {current}). Run: tolkin update")
/// when last_seen_latest is Some and semver-greater than current.
/// `passive_refresh_and_notice` supplies the persisted last-seen value.
pub fn passive_notice_line(current: &str, last_seen_latest: Option<&str>) -> Option<String> {
    known_newer_version(current, last_seen_latest).map(|latest| {
        format!(
            "tolkin {} is available (you have {}). Run: tolkin update",
            latest, current
        )
    })
}

/// The cached known-newer version, if any. Pure read over the persisted
/// last-seen value: zero IO, zero network. The TUI header's update chip
/// consumes this through the snapshot's config.
pub fn known_newer_version<'a>(
    current: &str,
    last_seen_latest: Option<&'a str>,
) -> Option<&'a str> {
    let latest = last_seen_latest?;
    (cmp_semver(latest, current) == Ordering::Greater).then_some(latest)
}

// ---- tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // classify_path

    #[test]
    fn classify_macos_homebrew_cellar() {
        assert_eq!(
            classify_path("/usr/local/Cellar/tolkin/0.12.0/bin/tolkin"),
            InstallChannel::Homebrew
        );
    }

    #[test]
    fn classify_homebrew_opt_prefix() {
        assert_eq!(
            classify_path("/opt/homebrew/bin/tolkin"),
            InstallChannel::Homebrew
        );
    }

    #[test]
    fn classify_linuxbrew() {
        assert_eq!(
            classify_path("/home/linuxbrew/.linuxbrew/bin/tolkin"),
            InstallChannel::Homebrew
        );
    }

    #[test]
    fn classify_npm_node_modules() {
        assert_eq!(
            classify_path("/usr/lib/node_modules/tolkin-cli/bin/tolkin"),
            InstallChannel::Npm
        );
    }

    #[test]
    fn classify_bun_global() {
        assert_eq!(
            classify_path("/home/user/.bun/bin/tolkin"),
            InstallChannel::Npm
        );
    }

    #[test]
    fn classify_npm_global() {
        assert_eq!(
            classify_path("/home/user/.npm/bin/tolkin"),
            InstallChannel::Npm
        );
    }

    #[test]
    fn classify_cargo_release() {
        assert_eq!(
            classify_path("/home/user/repos/monorepo/target/release/tolkin"),
            InstallChannel::Source
        );
    }

    #[test]
    fn classify_cargo_debug() {
        assert_eq!(
            classify_path("/home/user/repos/monorepo/target/debug/tolkin"),
            InstallChannel::Source
        );
    }

    #[test]
    fn classify_windows_cargo_release() {
        // Windows paths from current_exe use backslash separators.
        // The classify_path function checks for both forward and backslash
        // variants of the target/release and target/debug segments.
        assert_eq!(
            classify_path("C:\\Users\\user\\repos\\monorepo\\target\\release\\tolkin.exe"),
            InstallChannel::Source
        );
    }

    #[test]
    fn classify_unknown_arbitrary_path() {
        assert_eq!(
            classify_path("/usr/local/bin/tolkin"),
            InstallChannel::Unknown
        );
    }

    #[test]
    fn classify_windows_npm_appdata() {
        assert_eq!(
            classify_path(
                "C:\\Users\\user\\AppData\\Roaming\\npm\\node_modules\\tolkin-cli\\bin\\tolkin.exe"
            ),
            InstallChannel::Npm
        );
    }

    // cmp_semver

    #[test]
    fn semver_patch_bump() {
        assert_eq!(cmp_semver("0.12.0", "0.13.0"), Ordering::Less);
        assert_eq!(cmp_semver("0.13.0", "0.12.0"), Ordering::Greater);
    }

    #[test]
    fn semver_minor_beats_patch_lexicographically_wrong() {
        // String-compare "0.9.0" > "0.12.0" is wrong; numeric compare must be
        // Greater for 0.12.0 vs 0.9.0.
        assert_eq!(cmp_semver("0.9.0", "0.12.0"), Ordering::Less);
        assert_eq!(cmp_semver("0.12.0", "0.9.0"), Ordering::Greater);
    }

    #[test]
    fn semver_major_wins() {
        assert_eq!(cmp_semver("1.0.0", "0.99.99"), Ordering::Greater);
        assert_eq!(cmp_semver("0.99.99", "1.0.0"), Ordering::Less);
    }

    #[test]
    fn semver_equal() {
        assert_eq!(cmp_semver("0.12.0", "0.12.0"), Ordering::Equal);
        assert_eq!(cmp_semver("1.2.3", "1.2.3"), Ordering::Equal);
    }

    #[test]
    fn semver_prerelease_suffix_stripped() {
        // Pre-release labels are stripped; only the numeric triple is compared.
        assert_eq!(cmp_semver("0.13.0-beta.1", "0.12.0"), Ordering::Greater);
        assert_eq!(cmp_semver("0.12.0", "0.13.0-beta.1"), Ordering::Less);
        assert_eq!(cmp_semver("1.0.0-rc.1", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn semver_build_metadata_stripped() {
        assert_eq!(cmp_semver("1.0.0+build.1", "1.0.0"), Ordering::Equal);
    }

    #[test]
    fn semver_missing_parts_treated_as_zero() {
        assert_eq!(cmp_semver("1", "1.0.0"), Ordering::Equal);
        assert_eq!(cmp_semver("1.2", "1.2.0"), Ordering::Equal);
    }

    // passive_notice_line

    #[test]
    fn passive_notice_returns_some_when_update_available() {
        let line = passive_notice_line("0.12.0", Some("0.13.0"));
        assert!(line.is_some());
        let msg = line.unwrap();
        assert!(msg.contains("0.13.0"));
        assert!(msg.contains("0.12.0"));
        assert!(msg.contains("tolkin update"));
    }

    #[test]
    fn passive_notice_none_when_up_to_date() {
        assert!(passive_notice_line("0.12.0", Some("0.12.0")).is_none());
    }

    #[test]
    fn passive_notice_none_when_current_is_newer() {
        // Possible if someone runs a dev build.
        assert!(passive_notice_line("0.13.0", Some("0.12.0")).is_none());
    }

    #[test]
    fn passive_notice_none_when_no_last_seen() {
        assert!(passive_notice_line("0.12.0", None).is_none());
    }

    // known_newer_version (the TUI update chip seam)

    #[test]
    fn known_newer_version_only_reports_strictly_newer_cached_values() {
        assert_eq!(
            known_newer_version("0.14.0", Some("0.15.0")),
            Some("0.15.0")
        );
        assert_eq!(known_newer_version("0.14.0", Some("0.14.0")), None);
        assert_eq!(known_newer_version("0.15.0", Some("0.14.0")), None);
        assert_eq!(known_newer_version("0.14.0", None), None);
    }

    // should_refresh

    #[test]
    fn refresh_requires_explicit_consent() {
        assert!(!should_refresh(None, None, 1_000_000));
        assert!(!should_refresh(Some(false), None, 1_000_000));
        assert!(should_refresh(Some(true), None, 1_000_000));
    }

    #[test]
    fn refresh_fires_at_most_once_a_day() {
        let day = 86_400u64;
        assert!(!should_refresh(
            Some(true),
            Some(1_000_000),
            1_000_000 + day - 1
        ));
        assert!(should_refresh(Some(true), Some(1_000_000), 1_000_000 + day));
    }

    #[test]
    fn refresh_tolerates_clock_skew() {
        // last_check in the future must not panic or fire.
        assert!(!should_refresh(Some(true), Some(2_000_000), 1_000_000));
    }
}
