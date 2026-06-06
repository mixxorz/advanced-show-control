//! Fade engine actor — animates LV1 faders over time.

use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use crate::fade::curve::{FadeCurve, interpolate};

pub const TICK_HZ: u64 = 25;
pub const MIN_SEND_DELTA_DB: f64 = 0.1;
pub const OVERRIDE_THRESHOLD_DB: f64 = 0.5;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
}

#[derive(Debug, Clone)]
pub struct FadeConfig {
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

pub enum FadeCommand {
    StartFade { config: FadeConfig },
    AbortAll,
    FinishNow,
    Subscribe { tx: mpsc::Sender<FadeEvent> },
}

#[derive(Debug, Clone)]
pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelOverride { group: i32, channel: i32 },
    ChannelCancelled { group: i32, channel: i32 },
}

// ---------------------------------------------------------------------------
// Internal tick logic (pure — no async, easy to test)
// ---------------------------------------------------------------------------

pub(crate) struct ActiveChannel {
    pub group: i32,
    pub channel: i32,
    pub start_db: f64,
    pub target_db: f64,
    pub expected_db: f64,
    pub curve: FadeCurve,
    pub duration: Duration,
    pub started_at: Instant,
}

impl ActiveChannel {
    pub(crate) fn new(
        group: i32,
        channel: i32,
        start_db: f64,
        target_db: f64,
        curve: FadeCurve,
        duration: Duration,
        started_at: Instant,
    ) -> Self {
        Self {
            group,
            channel,
            start_db,
            target_db,
            expected_db: start_db,
            curve,
            duration,
            started_at,
        }
    }

    /// Returns the interpolated dB value at `now`.
    pub(crate) fn value_at(&self, now: Instant) -> f64 {
        let elapsed = now.duration_since(self.started_at).as_secs_f64();
        let t = elapsed / self.duration.as_secs_f64();
        interpolate(self.start_db, self.target_db, t, self.curve)
    }

    /// Returns true if the fade has completed (t >= 1.0).
    pub(crate) fn is_done(&self, now: Instant) -> bool {
        now.duration_since(self.started_at) >= self.duration
    }

    /// Returns true if `reported` deviates from `expected_db` by >= threshold.
    pub(crate) fn is_override(&self, reported_db: f64) -> bool {
        (reported_db - self.expected_db).abs() >= OVERRIDE_THRESHOLD_DB
    }

    /// Returns Some(new_db) if the value has moved enough to warrant sending.
    pub(crate) fn next_send(&mut self, now: Instant) -> Option<f64> {
        let new_db = if self.is_done(now) {
            self.target_db
        } else {
            self.value_at(now)
        };

        if (new_db - self.expected_db).abs() >= MIN_SEND_DELTA_DB {
            self.expected_db = new_db;
            Some(new_db)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn make_channel(start_db: f64, target_db: f64, duration_ms: u64) -> ActiveChannel {
        ActiveChannel::new(
            0, 0, start_db, target_db,
            FadeCurve::LinearDb,
            Duration::from_millis(duration_ms),
            Instant::now(),
        )
    }

    #[test]
    fn value_at_start_is_start_db() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let v = ch.value_at(ch.started_at);
        assert!((v - -20.0).abs() < 1e-10);
    }

    #[test]
    fn value_at_end_is_target_db() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        let v = ch.value_at(end);
        assert!((v - -10.0).abs() < 1e-10);
    }

    #[test]
    fn is_done_false_before_duration() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let mid = ch.started_at + Duration::from_millis(2000);
        assert!(!ch.is_done(mid));
    }

    #[test]
    fn is_done_true_at_duration() {
        let ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        assert!(ch.is_done(end));
    }

    #[test]
    fn is_override_true_when_deviation_exceeds_threshold() {
        let ch = make_channel(-20.0, -10.0, 4000);
        // expected_db starts at start_db (-20.0)
        assert!(ch.is_override(-20.0 + OVERRIDE_THRESHOLD_DB + 0.1));
    }

    #[test]
    fn is_override_false_when_deviation_below_threshold() {
        let ch = make_channel(-20.0, -10.0, 4000);
        assert!(!ch.is_override(-20.0 + OVERRIDE_THRESHOLD_DB - 0.1));
    }

    #[test]
    fn next_send_returns_none_when_below_min_delta() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        // At t=0, value is -20.0 = expected_db, delta is 0 — no send
        let now = ch.started_at;
        assert!(ch.next_send(now).is_none());
    }

    #[test]
    fn next_send_returns_some_when_above_min_delta() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        // At t=1.0 (end), value is -10.0, delta from -20.0 is 10.0 >= 0.1
        let end = ch.started_at + Duration::from_millis(4000);
        let result = ch.next_send(end);
        assert!(result.is_some());
        assert!((result.unwrap() - -10.0).abs() < 1e-10);
    }

    #[test]
    fn next_send_updates_expected_db() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(4000);
        ch.next_send(end);
        assert!((ch.expected_db - -10.0).abs() < 1e-10);
    }

    #[test]
    fn next_send_at_done_returns_exact_target() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(5000);
        let result = ch.next_send(end).unwrap();
        assert!((result - -10.0).abs() < 1e-10);
    }
}
