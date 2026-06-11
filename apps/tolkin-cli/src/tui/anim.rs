//! Animation engine: tweens keyed by surface, an injected clock, and a
//! reduced-motion kill switch.
//!
//! Nothing in the TUI calls `Instant::now()` directly except [`SystemClock`];
//! every consumer samples through the [`Animator`], so tests drive time with
//! [`ManualClock`] and the compact frame renders deterministically with the
//! animator disabled (every `go` snaps to its target instantly).

#[cfg(test)]
use std::cell::Cell;
use std::collections::HashMap;
#[cfg(test)]
use std::rc::Rc;
use std::time::{Duration, Instant};

/// Time source seam. The dashboard installs [`SystemClock`]; tests install
/// [`ManualClock`] and step it explicitly.
pub trait Clock {
    fn now(&self) -> Instant;
}

pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }
}

/// Hand-stepped clock for unit tests. Clones share the same underlying
/// instant, so the test holds one clone and the animator the other.
#[cfg(test)]
#[derive(Clone)]
pub struct ManualClock {
    now: Rc<Cell<Instant>>,
}

#[cfg(test)]
impl ManualClock {
    pub fn new() -> ManualClock {
        ManualClock {
            now: Rc::new(Cell::new(Instant::now())),
        }
    }

    pub fn advance(&self, by: Duration) {
        self.now.set(self.now.get() + by);
    }
}

#[cfg(test)]
impl Clock for ManualClock {
    fn now(&self) -> Instant {
        self.now.get()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    Linear,
    OutCubic,
    /// Part of the fixed animation inventory; first consumer arrives with
    /// the motion wave.
    #[allow(dead_code)]
    InOutCubic,
}

impl Ease {
    fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Ease::Linear => t,
            Ease::OutCubic => 1.0 - (1.0 - t).powi(3),
            Ease::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
        }
    }
}

/// Identity of one animated value. Panels and rows are small indices so the
/// key stays `Copy`. Rows of reorderable data (machine projects, list
/// reveals) key by an identity hash instead of a position, so sorting and
/// filtering never reassign a tween to a different row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimKey {
    /// Header tab underline x position (in tab-index units).
    TabUnderline,
    /// Overview hero card count-up, by card index.
    Card(u8),
    /// Horizontal bar fill, by (panel, row). Position-keyed panels only
    /// (load profile buckets, day strips), where the index IS the identity.
    Bar { panel: u8, row: u8 },
    /// Slim percent gauge fill, by gauge index.
    Gauge(u8),
    /// Machine project weight bar, keyed by `ident` of the project key so
    /// filtered and re-sorted views sample their own tween.
    Weight(u64),
    /// Staggered row reveal, keyed by (list, `ident` of the row subject).
    Reveal { panel: u8, id: u64 },
    /// Toast slide-in progress, keyed by the toast's stack id.
    Toast(u64),
}

/// FNV-1a hash of a row's identity string for [`AnimKey::Weight`] and
/// [`AnimKey::Reveal`]. Stable across sorts and filters by construction.
pub fn ident(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

#[derive(Clone, Copy, Debug)]
struct Tween {
    from: f32,
    to: f32,
    start: Instant,
    dur: Duration,
    ease: Ease,
}

impl Tween {
    /// A delayed tween (its `start` in the future) samples at `from` until
    /// its birth: `saturating_duration_since` yields zero elapsed there.
    fn sample(&self, now: Instant) -> f32 {
        if self.dur.is_zero() {
            return if now < self.start { self.from } else { self.to };
        }
        let elapsed = now.saturating_duration_since(self.start);
        let t = elapsed.as_secs_f32() / self.dur.as_secs_f32();
        self.from + (self.to - self.from) * self.ease.apply(t)
    }

    fn finished(&self, now: Instant) -> bool {
        now >= self.start && now.saturating_duration_since(self.start) >= self.dur
    }
}

/// Tween table plus clock. `enabled = false` is reduced motion: `go` snaps
/// instantly and `active()` reports false, so the event loop idles.
pub struct Animator {
    tweens: HashMap<AnimKey, Tween>,
    settled: HashMap<AnimKey, f32>,
    clock: Box<dyn Clock>,
    /// Creation instant: the phase reference for [`Animator::pulse`].
    born: Instant,
    enabled: bool,
}

impl Animator {
    pub fn new(clock: Box<dyn Clock>, enabled: bool) -> Animator {
        let born = clock.now();
        Animator {
            tweens: HashMap::new(),
            settled: HashMap::new(),
            clock,
            born,
            enabled,
        }
    }

    /// Production animator on the system clock.
    pub fn system(enabled: bool) -> Animator {
        Animator::new(Box::new(SystemClock), enabled)
    }

    /// Reduced-motion animator: every `go` snaps, `active()` is false.
    /// The compact frame uses this for deterministic output.
    pub fn disabled() -> Animator {
        Animator::system(false)
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The animator's clock, exposed for non-tween time needs (toast TTLs).
    /// Keeps the no-direct-Instant rule intact: tests inject ManualClock.
    pub fn now(&self) -> Instant {
        self.clock.now()
    }

    /// Start (or retarget) the tween for `key`. Retargeting an active tween
    /// starts from its CURRENT sampled value, so interruptions stay smooth.
    /// A key never animated before starts from its settled value, falling
    /// back to 0.0 (bars grow in from empty on first data).
    pub fn go(&mut self, key: AnimKey, to: f32, dur: Duration, ease: Ease) {
        self.go_delayed(key, to, dur, ease, Duration::ZERO);
    }

    /// `go` with a birth delay: the tween holds its `from` value until
    /// `delay` elapses, then runs `dur` to `to`. This is the true row
    /// stagger from the animation inventory (row i delayed i x 30 ms),
    /// replacing the wave 1 duration-lengthening approximation.
    pub fn go_delayed(
        &mut self,
        key: AnimKey,
        to: f32,
        dur: Duration,
        ease: Ease,
        delay: Duration,
    ) {
        if !self.enabled {
            // Reduced motion: nothing to store. `value` returns the caller's
            // fallback (the model's current target), so disabled rendering
            // always reflects the data, never a stale snap.
            return;
        }
        let now = self.clock.now();
        let from = match self.tweens.get(&key) {
            Some(tween) => tween.sample(now),
            None => self.settled.get(&key).copied().unwrap_or(0.0),
        };
        self.tweens.insert(
            key,
            Tween {
                from,
                to,
                start: now + delay,
                dur,
                ease,
            },
        );
    }

    /// Sample the current value for `key`. Keys without an active tween
    /// return their settled value, then `fallback` (callers pass the model's
    /// target so a pruned tween renders at rest). A disabled animator
    /// always returns the fallback: render exactly what the model says.
    pub fn value(&self, key: AnimKey, fallback: f32) -> f32 {
        if !self.enabled {
            return fallback;
        }
        match self.tweens.get(&key) {
            Some(tween) => tween.sample(self.clock.now()),
            None => self.settled.get(&key).copied().unwrap_or(fallback),
        }
    }

    /// True while any tween is unfinished. Drives the 33 ms frame cadence.
    pub fn active(&self) -> bool {
        if !self.enabled {
            return false;
        }
        let now = self.clock.now();
        self.tweens.values().any(|t| !t.finished(now))
    }

    /// The setup-card breathing phase: a 0..=1 sine over `period_ms`,
    /// sampled against the animator's birth instant. Reduced motion pins
    /// the value at 0.5 (the middle ramp step, a plain accent border).
    pub fn pulse(&self, period_ms: u64) -> f32 {
        if !self.enabled || period_ms == 0 {
            return 0.5;
        }
        let elapsed = self.clock.now().duration_since(self.born).as_millis() as u64;
        let t = (elapsed % period_ms) as f32 / period_ms as f32;
        0.5 + 0.5 * (t * std::f32::consts::TAU).sin()
    }

    /// Drop every trace of `key` (active tween and settled value). Used
    /// when the keyed surface dies for good (an expired toast), so the
    /// settled table cannot grow without bound over a long session.
    pub fn forget(&mut self, key: AnimKey) {
        self.tweens.remove(&key);
        self.settled.remove(&key);
    }

    /// Move finished tweens to the settled table. Called on the tick path,
    /// never inside view.
    pub fn prune(&mut self) {
        let now = self.clock.now();
        let finished: Vec<AnimKey> = self
            .tweens
            .iter()
            .filter(|(_, t)| t.finished(now))
            .map(|(k, _)| *k)
            .collect();
        for key in finished {
            if let Some(t) = self.tweens.remove(&key) {
                self.settled.insert(key, t.to);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manual_animator() -> (Animator, ManualClock) {
        let clock = ManualClock::new();
        let animator = Animator::new(Box::new(clock.clone()), true);
        (animator, clock)
    }

    #[test]
    fn tween_progresses_with_linear_easing() {
        let (mut a, clock) = manual_animator();
        a.go(
            AnimKey::TabUnderline,
            10.0,
            Duration::from_secs(1),
            Ease::Linear,
        );
        assert_eq!(a.value(AnimKey::TabUnderline, 99.0), 0.0);
        clock.advance(Duration::from_millis(500));
        let mid = a.value(AnimKey::TabUnderline, 99.0);
        assert!((mid - 5.0).abs() < 0.01, "midpoint was {mid}");
        clock.advance(Duration::from_millis(500));
        assert_eq!(a.value(AnimKey::TabUnderline, 99.0), 10.0);
        assert!(!a.active(), "finished tween must not keep the loop hot");
    }

    #[test]
    fn out_cubic_front_loads_motion() {
        let (mut a, clock) = manual_animator();
        a.go(
            AnimKey::Card(0),
            100.0,
            Duration::from_secs(1),
            Ease::OutCubic,
        );
        clock.advance(Duration::from_millis(500));
        let mid = a.value(AnimKey::Card(0), 0.0);
        assert!(mid > 80.0, "OutCubic at t=0.5 should pass 80%, got {mid}");
    }

    #[test]
    fn retarget_starts_from_current_value() {
        let (mut a, clock) = manual_animator();
        a.go(
            AnimKey::Bar { panel: 0, row: 1 },
            10.0,
            Duration::from_secs(1),
            Ease::Linear,
        );
        clock.advance(Duration::from_millis(500));
        a.go(
            AnimKey::Bar { panel: 0, row: 1 },
            0.0,
            Duration::from_secs(1),
            Ease::Linear,
        );
        let v = a.value(AnimKey::Bar { panel: 0, row: 1 }, 99.0);
        assert!(
            (v - 5.0).abs() < 0.01,
            "retarget must start at 5.0, got {v}"
        );
        clock.advance(Duration::from_millis(500));
        let v = a.value(AnimKey::Bar { panel: 0, row: 1 }, 99.0);
        assert!((v - 2.5).abs() < 0.01, "halfway back to zero, got {v}");
    }

    #[test]
    fn prune_moves_finished_to_settled_and_new_go_resumes_there() {
        let (mut a, clock) = manual_animator();
        a.go(
            AnimKey::Card(1),
            42.0,
            Duration::from_millis(100),
            Ease::Linear,
        );
        clock.advance(Duration::from_millis(200));
        a.prune();
        // Settled value survives pruning; fallback is ignored.
        assert_eq!(a.value(AnimKey::Card(1), 7.0), 42.0);
        // A fresh go for the same key starts from the settled value.
        a.go(AnimKey::Card(1), 52.0, Duration::from_secs(1), Ease::Linear);
        clock.advance(Duration::from_millis(500));
        let v = a.value(AnimKey::Card(1), 0.0);
        assert!(
            (v - 47.0).abs() < 0.01,
            "count-up from previous value, got {v}"
        );
    }

    #[test]
    fn unknown_key_returns_fallback() {
        let (a, _clock) = manual_animator();
        assert_eq!(a.value(AnimKey::Gauge(3), 0.81), 0.81);
    }

    #[test]
    fn reduced_motion_renders_model_truth_and_never_reports_active() {
        let clock = ManualClock::new();
        let mut a = Animator::new(Box::new(clock.clone()), false);
        a.go(
            AnimKey::Card(0),
            10.0,
            Duration::from_secs(1),
            Ease::OutCubic,
        );
        // Disabled sampling ignores tweens entirely: the fallback (the
        // model's current target) is the rendered value, instantly.
        assert_eq!(a.value(AnimKey::Card(0), 10.0), 10.0);
        assert!(!a.active());
        clock.advance(Duration::from_millis(10));
        // A data change shows up immediately even without a new go().
        assert_eq!(a.value(AnimKey::Card(0), 12.5), 12.5);
    }

    #[test]
    fn zero_duration_snaps() {
        let (mut a, _clock) = manual_animator();
        a.go(AnimKey::Card(9), 3.0, Duration::ZERO, Ease::Linear);
        assert_eq!(a.value(AnimKey::Card(9), 0.0), 3.0);
        assert!(!a.active());
    }

    #[test]
    fn delayed_tween_holds_from_until_birth_then_runs() {
        let (mut a, clock) = manual_animator();
        let key = AnimKey::Reveal { panel: 0, id: 7 };
        a.go_delayed(
            key,
            1.0,
            Duration::from_millis(100),
            Ease::Linear,
            Duration::from_millis(60),
        );
        // Pre-birth: holds the from value (0.0 for a fresh key) and keeps
        // the loop hot so the birth frame renders on time.
        assert_eq!(a.value(key, 9.0), 0.0);
        assert!(a.active(), "delayed tween must keep the cadence at 30 fps");
        clock.advance(Duration::from_millis(59));
        assert_eq!(a.value(key, 9.0), 0.0, "still pre-birth at 59 ms");
        a.prune();
        assert!(
            a.value(key, 9.0) == 0.0 && a.active(),
            "prune must not settle a tween that has not been born"
        );
        // Birth plus half the duration: halfway through the ramp.
        clock.advance(Duration::from_millis(51));
        let v = a.value(key, 9.0);
        assert!((v - 0.5).abs() < 0.01, "midpoint after birth, got {v}");
        clock.advance(Duration::from_millis(50));
        assert_eq!(a.value(key, 9.0), 1.0);
        assert!(!a.active());
    }

    #[test]
    fn staggered_rows_birth_in_index_order() {
        let (mut a, clock) = manual_animator();
        for row in 0..3u64 {
            a.go_delayed(
                AnimKey::Reveal { panel: 1, id: row },
                1.0,
                Duration::from_millis(30),
                Ease::Linear,
                Duration::from_millis(30 * row),
            );
        }
        clock.advance(Duration::from_millis(45));
        let v0 = a.value(AnimKey::Reveal { panel: 1, id: 0 }, 0.0);
        let v1 = a.value(AnimKey::Reveal { panel: 1, id: 1 }, 0.0);
        let v2 = a.value(AnimKey::Reveal { panel: 1, id: 2 }, 0.0);
        assert_eq!(v0, 1.0, "row 0 finished");
        assert!((v1 - 0.5).abs() < 0.01, "row 1 mid-ramp, got {v1}");
        assert_eq!(v2, 0.0, "row 2 not yet born");
    }

    #[test]
    fn reduced_motion_ignores_delays_too() {
        let clock = ManualClock::new();
        let mut a = Animator::new(Box::new(clock.clone()), false);
        a.go_delayed(
            AnimKey::Toast(1),
            1.0,
            Duration::from_millis(150),
            Ease::OutCubic,
            Duration::from_millis(90),
        );
        assert_eq!(a.value(AnimKey::Toast(1), 1.0), 1.0, "snap to fallback");
        assert!(!a.active());
    }

    #[test]
    fn pulse_breathes_on_a_sine_and_pins_at_half_when_disabled() {
        let clock = ManualClock::new();
        let a = Animator::new(Box::new(clock.clone()), true);
        assert!(
            (a.pulse(4_600) - 0.5).abs() < 0.001,
            "phase 0 sits mid-ramp"
        );
        clock.advance(Duration::from_millis(1_150));
        assert!(a.pulse(4_600) > 0.99, "quarter period peaks");
        clock.advance(Duration::from_millis(2_300));
        assert!(a.pulse(4_600) < 0.01, "three quarters bottoms out");
        clock.advance(Duration::from_millis(1_150));
        assert!((a.pulse(4_600) - 0.5).abs() < 0.001, "full period wraps");
        let disabled = Animator::new(Box::new(clock.clone()), false);
        clock.advance(Duration::from_millis(1_150));
        assert_eq!(disabled.pulse(4_600), 0.5, "reduced motion pins the ramp");
        assert_eq!(a.pulse(0), 0.5, "zero period never divides by zero");
    }

    #[test]
    fn forget_drops_tween_and_settled_state() {
        let (mut a, clock) = manual_animator();
        a.go(
            AnimKey::Toast(3),
            1.0,
            Duration::from_millis(10),
            Ease::Linear,
        );
        clock.advance(Duration::from_millis(20));
        a.prune();
        assert_eq!(
            a.value(AnimKey::Toast(3), 0.25),
            1.0,
            "settled before forget"
        );
        a.forget(AnimKey::Toast(3));
        assert_eq!(
            a.value(AnimKey::Toast(3), 0.25),
            0.25,
            "fallback after forget"
        );
        assert!(!a.active());
    }

    #[test]
    fn ident_is_stable_and_separates_paths() {
        assert_eq!(ident("src/lib.rs"), ident("src/lib.rs"));
        assert_ne!(ident("src/lib.rs"), ident("src/main.rs"));
        assert_ne!(ident(""), ident(" "));
    }
}
