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
/// key stays `Copy`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnimKey {
    /// Header tab underline x position (in tab-index units).
    TabUnderline,
    /// Overview hero card count-up, by card index.
    Card(u8),
    /// Horizontal bar fill, by (panel, row).
    Bar { panel: u8, row: u8 },
    /// Slim percent gauge fill, by gauge index.
    Gauge(u8),
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
    fn sample(&self, now: Instant) -> f32 {
        if self.dur.is_zero() {
            return self.to;
        }
        let elapsed = now.saturating_duration_since(self.start);
        let t = elapsed.as_secs_f32() / self.dur.as_secs_f32();
        self.from + (self.to - self.from) * self.ease.apply(t)
    }

    fn finished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.dur
    }
}

/// Tween table plus clock. `enabled = false` is reduced motion: `go` snaps
/// instantly and `active()` reports false, so the event loop idles.
pub struct Animator {
    tweens: HashMap<AnimKey, Tween>,
    settled: HashMap<AnimKey, f32>,
    clock: Box<dyn Clock>,
    enabled: bool,
}

impl Animator {
    pub fn new(clock: Box<dyn Clock>, enabled: bool) -> Animator {
        Animator {
            tweens: HashMap::new(),
            settled: HashMap::new(),
            clock,
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

    /// Start (or retarget) the tween for `key`. Retargeting an active tween
    /// starts from its CURRENT sampled value, so interruptions stay smooth.
    /// A key never animated before starts from its settled value, falling
    /// back to 0.0 (bars grow in from empty on first data).
    pub fn go(&mut self, key: AnimKey, to: f32, dur: Duration, ease: Ease) {
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
                start: now,
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
}
