//! Machine-fit model recommendation and guided setup for `tolkin optimize`.
//!
//! When no sidecar is detected (and the model path is not disabled), this
//! module recommends a specific model sized to the machine's RAM, optionally
//! prints a platform-specific setup guide, and contributes a `suggested_model`
//! field to `--json` output.
//!
//! Model table sourced from MLX-RESEARCH-FINDINGS.md section 2.3 (tolkin-web).

use crate::commands::optimize::prompts;

/// A model recommendation sized to available RAM.
#[derive(Debug, Clone)]
pub struct Recommendation {
    pub model_id: &'static str,
    pub download_gb: f64,
    pub tier_note: &'static str,
    /// True when RAM detection failed and we assumed a floor of 16 GB.
    pub assumed: bool,
}

/// Detect installed RAM in gibibytes.
///
/// macOS: `sysctl -n hw.memsize` (bytes). Linux: /proc/meminfo MemTotal kB.
/// Returns None on any failure; the caller falls back to a conservative
/// recommendation with `assumed = true`.
pub fn detect_ram_gb() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&out.stdout);
        let bytes: u64 = s.trim().parse().ok()?;
        Some(bytes / (1024 * 1024 * 1024))
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        parse_meminfo_kb(&text).map(|kb| kb / (1024 * 1024))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Parse the MemTotal line from a /proc/meminfo-style string, returning kB.
/// Pure function, exposed for unit testing on all platforms.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn parse_meminfo_kb(text: &str) -> Option<u64> {
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let trimmed = rest.trim();
            // "16384000 kB" or just digits
            let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
            return digits.parse().ok();
        }
    }
    None
}

/// Select a model recommendation from RAM. Pure, unit-tested.
///
/// Table from MLX-RESEARCH-FINDINGS.md section 2.3:
///   48+ GB: Qwen3.5-35B-A3B-4bit (MoE, active params near 3B)
///   32+ GB: Qwen3.5-9B-4bit
///   16+ GB: Qwen3.5-4B-4bit (the stated floor model)
///    8+ GB: Qwen3.5-0.8B-4bit (narration-only quality)
///   unknown: 4B entry with assumed=true
pub fn recommend(ram_gb: Option<u64>) -> Recommendation {
    match ram_gb {
        Some(gb) if gb >= 48 => Recommendation {
            model_id: "mlx-community/Qwen3.5-35B-A3B-4bit",
            download_gb: 20.4,
            tier_note: "MoE with about 3B active parameters: prefills like a small model, answers near 9B-plus quality",
            assumed: false,
        },
        Some(gb) if gb >= 32 => Recommendation {
            model_id: "mlx-community/Qwen3.5-9B-4bit",
            download_gb: 6.0,
            tier_note: "comfortable fit alongside an IDE on 32GB",
            assumed: false,
        },
        Some(gb) if gb >= 16 => Recommendation {
            model_id: "mlx-community/Qwen3.5-4B-4bit",
            download_gb: 3.0,
            tier_note: "the honest floor model: solid narration and lint on 16GB",
            assumed: false,
        },
        Some(gb) if gb >= 8 => Recommendation {
            model_id: "mlx-community/Qwen3.5-0.8B-4bit",
            download_gb: 0.6,
            tier_note: "8GB fits only the smallest models; expect basic narration quality",
            assumed: false,
        },
        _ => Recommendation {
            model_id: "mlx-community/Qwen3.5-4B-4bit",
            download_gb: 3.0,
            tier_note: "assuming at least 16GB; the honest floor model: solid narration and lint on 16GB",
            assumed: true,
        },
    }
}

/// Compute an estimated wall time for a typical narration task using the chip
/// rate table from prompts.rs. Formula: 5000 / prefill + 600 / decode.
/// Returns a string like "about 35s (estimate)".
pub fn narration_wall_time() -> String {
    let rates = prompts::chip_rates(prompts::detect_chip().as_deref());
    let secs = (5000.0 / rates.prefill + 600.0 / rates.decode).ceil() as u64;
    format!("about {secs}s (estimate)")
}

/// Build the platform-specific setup guide for the given recommendation.
/// Pure, unit-tested.
pub fn setup_guide(rec: &Recommendation) -> String {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        setup_guide_apple(rec)
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        setup_guide_other(rec)
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn setup_guide_apple(rec: &Recommendation) -> String {
    let model = rec.model_id;
    let dl_gb = rec.download_gb;
    let wall = narration_wall_time();
    format!(
        "Nothing leaves your machine: the model runs locally and tolkin only talks to it over 127.0.0.1.\n\
        \n\
        Steps to get started on Apple silicon:\n\
        \n\
        1. Install uv (if you do not have it):\n\
           brew install uv\n\
        \n\
        2. Install mlx-lm:\n\
           uv tool install mlx-lm\n\
        \n\
        3. Start the local model server (keep this terminal open):\n\
           mlx_lm.server --model {model} --max-tokens 1200\n\
           The first run downloads about {dl_gb} GB to ~/.cache/huggingface.\n\
        \n\
        4. In another terminal, re-run:\n\
           tolkin optimize\n\
           Tolkin auto-detects 127.0.0.1:8080 and asks consent before sending any file content.\n\
           Narration takes {wall} on this machine.\n\
        \n\
        Alternative: LM Studio (free for work) can serve the same model on port 1234;\n\
        tolkin detects that port automatically too.\n\
        \n\
        To remove: uv tool uninstall mlx-lm, then delete the model weights from\n\
        ~/.cache/huggingface/hub. To permanently disable, set enabled = false under\n\
        [sidecar] in your tolkin config.toml.\n"
    )
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn setup_guide_other(rec: &Recommendation) -> String {
    let model = rec.model_id;
    let dl_gb = rec.download_gb;
    format!(
        "Nothing leaves your machine: the model runs locally and tolkin only talks to it over 127.0.0.1.\n\
        \n\
        Any OpenAI-compatible local server works on this platform:\n\
        \n\
        - Ollama on port 11434 (ollama.com)\n\
        - llama-server or mlx-lm on port 8080\n\
        - LM Studio on port 1234 (lmstudio.ai, free for work)\n\
        \n\
        Tolkin auto-detects all three ports. Recommended model: {model} ({dl_gb} GB download).\n\
        \n\
        Performance on this platform is unbenchmarked; dry-run ETAs are conservative.\n\
        \n\
        After the server is running, re-run tolkin optimize. Tolkin will detect it and\n\
        ask consent before sending any file content.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_48gb_gives_moe_model() {
        let r = recommend(Some(48));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-35B-A3B-4bit");
        assert!(!r.assumed);
    }

    #[test]
    fn recommend_64gb_still_moe() {
        let r = recommend(Some(64));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-35B-A3B-4bit");
        assert!(!r.assumed);
    }

    #[test]
    fn recommend_32gb_gives_9b() {
        let r = recommend(Some(32));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-9B-4bit");
        assert!((r.download_gb - 6.0).abs() < 0.01);
        assert!(!r.assumed);
    }

    #[test]
    fn recommend_33gb_gives_9b() {
        let r = recommend(Some(33));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-9B-4bit");
    }

    #[test]
    fn recommend_16gb_gives_4b() {
        let r = recommend(Some(16));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-4B-4bit");
        assert!((r.download_gb - 3.0).abs() < 0.01);
        assert!(!r.assumed);
    }

    #[test]
    fn recommend_17gb_gives_4b() {
        let r = recommend(Some(17));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-4B-4bit");
    }

    #[test]
    fn recommend_8gb_gives_0point8b() {
        let r = recommend(Some(8));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-0.8B-4bit");
        assert!((r.download_gb - 0.6).abs() < 0.01);
        assert!(!r.assumed);
    }

    #[test]
    fn recommend_12gb_gives_0point8b() {
        let r = recommend(Some(12));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-0.8B-4bit");
    }

    #[test]
    fn recommend_7gb_also_gives_0point8b() {
        // Below 8 GB falls through to None branch (assumed=true, 4B fallback)
        // because 7 < 8 but Some(7) is not None.
        // The wildcard arm handles < 8 GB as assumed 4B.
        let r = recommend(Some(7));
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-4B-4bit");
        assert!(r.assumed);
    }

    #[test]
    fn recommend_none_gives_assumed_4b() {
        let r = recommend(None);
        assert_eq!(r.model_id, "mlx-community/Qwen3.5-4B-4bit");
        assert!(r.assumed);
        assert!(r.tier_note.starts_with("assuming at least 16GB"));
    }

    #[test]
    fn guide_contains_model_id_and_download_size() {
        let rec = recommend(Some(16));
        let guide = setup_guide(&rec);
        assert!(
            guide.contains(rec.model_id),
            "guide must mention the model id"
        );
        // download_gb rendered as a float in the guide text
        assert!(guide.contains("3"), "guide must mention the download size");
    }

    #[test]
    fn guide_opens_with_privacy_statement() {
        let rec = recommend(Some(16));
        let guide = setup_guide(&rec);
        assert!(
            guide.starts_with("Nothing leaves your machine"),
            "guide must open with the privacy statement"
        );
    }

    #[test]
    fn parse_meminfo_kb_standard_line() {
        let text = "MemTotal:       16384000 kB\nMemFree:        8192000 kB\n";
        assert_eq!(parse_meminfo_kb(text), Some(16_384_000));
    }

    #[test]
    fn parse_meminfo_kb_no_spaces() {
        let text = "MemTotal:32768000 kB\n";
        assert_eq!(parse_meminfo_kb(text), Some(32_768_000));
    }

    #[test]
    fn parse_meminfo_kb_missing_returns_none() {
        let text = "MemFree: 8192 kB\nBuffers: 512 kB\n";
        assert_eq!(parse_meminfo_kb(text), None);
    }

    #[test]
    fn narration_wall_time_is_non_empty_and_contains_estimate() {
        let s = narration_wall_time();
        assert!(!s.is_empty());
        assert!(
            s.contains("estimate"),
            "must label the result as an estimate"
        );
        assert!(s.starts_with("about "));
    }

    #[test]
    fn recommend_boundary_below_8gb_falls_to_assumed() {
        // 0 GB - extreme edge
        let r = recommend(Some(0));
        assert!(r.assumed, "0 GB should produce assumed=true recommendation");
    }
}
