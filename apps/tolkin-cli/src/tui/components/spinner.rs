//! Braille spinner frames for busy states (scan, reload). The frame index
//! is owned by the model and advanced on ticks; this module only maps an
//! index to a glyph, so rendering stays pure and deterministic.

pub const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Static fallback when animations are disabled (reduced motion, compact
/// frame): a quiet ellipsis instead of a frozen spinner frame.
pub const STATIC_FALLBACK: &str = "⋯";

/// Spinner advances one frame per 80 ms of accumulated busy time.
pub const FRAME_MS: u64 = 80;

/// Map accumulated busy milliseconds to a frame. Disabled animation always
/// yields the static fallback.
pub fn frame(busy_ms: u64, animations_enabled: bool) -> &'static str {
    if !animations_enabled {
        return STATIC_FALLBACK;
    }
    FRAMES[((busy_ms / FRAME_MS) % FRAMES.len() as u64) as usize]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_advance_every_80ms_and_wrap() {
        assert_eq!(frame(0, true), FRAMES[0]);
        assert_eq!(frame(79, true), FRAMES[0]);
        assert_eq!(frame(80, true), FRAMES[1]);
        assert_eq!(frame(800, true), FRAMES[0]);
    }

    #[test]
    fn disabled_yields_static_fallback() {
        assert_eq!(frame(0, false), STATIC_FALLBACK);
        assert_eq!(frame(400, false), STATIC_FALLBACK);
    }
}
