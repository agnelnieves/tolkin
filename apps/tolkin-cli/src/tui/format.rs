//! Formatting helpers for the dashboard: number humanizing, currency,
//! relative time, path splitting, left truncation, and a tiny base64
//! encoder for the OSC52 copy path. Pure functions only; no IO, no clock
//! reads, no environment access.

/// Thousands separators: 1234567 -> "1,234,567".
pub fn commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(char::from(*b));
    }
    out
}

/// Dollar formatting with the same thresholds the plain stats surface uses.
pub fn usd(v: f64) -> String {
    if v == 0.0 {
        "$0".to_string()
    } else if v < 0.01 {
        format!("${v:.5}")
    } else if v < 1.0 {
        format!("${v:.3}")
    } else {
        format!("${v:.2}")
    }
}

/// Token counts at card scale: 950 -> "950", 12_400 -> "12.4k",
/// 1_230_000 -> "1.2M", 3_400_000_000 -> "3.4B".
pub fn humanize_tokens(n: u64) -> String {
    if n < 1_000 {
        return n.to_string();
    }
    let (value, suffix) = if n < 1_000_000 {
        (n as f64 / 1_000.0, "k")
    } else if n < 1_000_000_000 {
        (n as f64 / 1_000_000.0, "M")
    } else {
        (n as f64 / 1_000_000_000.0, "B")
    };
    if value >= 100.0 {
        format!("{value:.0}{suffix}")
    } else {
        format!("{value:.1}{suffix}")
    }
}

/// Relative age for "data 2m" style chrome labels. `now` and `ts` are epoch
/// seconds; a `ts` in the future clamps to "now".
pub fn relative_time(now: u64, ts: u64) -> String {
    let delta = now.saturating_sub(ts);
    if delta < 60 {
        "now".to_string()
    } else if delta < 3_600 {
        format!("{}m", delta / 60)
    } else if delta < 86_400 {
        format!("{}h", delta / 3_600)
    } else {
        format!("{}d", delta / 86_400)
    }
}

/// Same as [`relative_time`] but reading as prose: "2m ago", "now".
pub fn relative_time_ago(now: u64, ts: u64) -> String {
    let short = relative_time(now, ts);
    if short == "now" {
        short
    } else {
        format!("{short} ago")
    }
}

/// Milliseconds as a short human duration: 450 -> "0.5s", 1234 -> "1.2s",
/// 95_000 -> "95s".
pub fn duration_short(ms: u64) -> String {
    let secs = ms as f64 / 1_000.0;
    if secs < 10.0 {
        format!("{secs:.1}s")
    } else {
        format!("{secs:.0}s")
    }
}

/// Show the rightmost characters of a string that does not fit, with a
/// leading ASCII ellipsis (portable with terminals that lack U+2026).
pub fn truncate_left(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let take = width.saturating_sub(3);
    let suffix: String = s
        .chars()
        .rev()
        .take(take)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("...{suffix}")
}

/// Split a path into (directory part with trailing separator, file name).
/// The directory part renders muted and the file name bright. A path with
/// no separator yields an empty directory part.
pub fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(idx) => (&path[..=idx], &path[idx + 1..]),
        None => ("", path),
    }
}

/// RFC 4648 standard base64 with padding. Used by the OSC52 clipboard
/// escape; tiny on purpose so the zero-dependency contract holds.
pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) as usize & 0x3F] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3F] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(triple >> 6) as usize & 0x3F] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[triple as usize & 0x3F] as char);
        } else {
            out.push('=');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commas_groups_thousands() {
        assert_eq!(commas(0), "0");
        assert_eq!(commas(999), "999");
        assert_eq!(commas(1_000), "1,000");
        assert_eq!(commas(1_234_567), "1,234,567");
    }

    #[test]
    fn usd_thresholds() {
        assert_eq!(usd(0.0), "$0");
        assert_eq!(usd(0.004), "$0.00400");
        assert_eq!(usd(0.5), "$0.500");
        assert_eq!(usd(12.345), "$12.35");
    }

    #[test]
    fn humanize_token_scales() {
        assert_eq!(humanize_tokens(0), "0");
        assert_eq!(humanize_tokens(950), "950");
        assert_eq!(humanize_tokens(12_400), "12.4k");
        assert_eq!(humanize_tokens(104_000), "104k");
        assert_eq!(humanize_tokens(1_230_000), "1.2M");
        assert_eq!(humanize_tokens(3_400_000_000), "3.4B");
    }

    #[test]
    fn relative_time_buckets() {
        assert_eq!(relative_time(1_000, 990), "now");
        assert_eq!(relative_time(1_000, 1_500), "now"); // future clamps
        assert_eq!(relative_time(10_000, 9_000), "16m");
        assert_eq!(relative_time(50_000, 10_000), "11h");
        assert_eq!(relative_time(1_000_000, 10_000), "11d");
        assert_eq!(relative_time_ago(10_000, 9_000), "16m ago");
        assert_eq!(relative_time_ago(1_000, 990), "now");
    }

    #[test]
    fn duration_short_scales() {
        assert_eq!(duration_short(450), "0.5s");
        assert_eq!(duration_short(1_234), "1.2s");
        assert_eq!(duration_short(95_000), "95s");
    }

    #[test]
    fn truncate_left_keeps_short_and_ellipsizes_long() {
        assert_eq!(truncate_left("abc", 10), "abc");
        let out = truncate_left("/very/deep/nested/path/to/repo", 12);
        assert!(out.starts_with("..."));
        assert_eq!(out.chars().count(), 12);
    }

    #[test]
    fn split_path_separates_dir_and_name() {
        assert_eq!(split_path("a/b/c.md"), ("a/b/", "c.md"));
        assert_eq!(split_path("CLAUDE.md"), ("", "CLAUDE.md"));
        assert_eq!(split_path(".claude/rules.md"), (".claude/", "rules.md"));
    }

    #[test]
    fn base64_rfc4648_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }
}
