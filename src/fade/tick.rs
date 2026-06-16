use std::time::{Duration, Instant};

use crate::fade::curve::{FadeCurve, interpolate};
use crate::fade::fader_law::db_to_pos;
use crate::fade::types::{FadeParameter, FadeTargetKey};

pub const TICK_HZ: u64 = 25;
/// Minimum fader position change (0.0–1.0) required to send a SetGain command.
pub const MIN_SEND_DELTA_POS: f64 = 0.002;
/// Minimum pan change (±100) required to send a SetPan command.
pub const MIN_SEND_DELTA_PAN: f64 = 0.9;
/// Minimum balance change (±100) required to send a SetBalance command.
pub const MIN_SEND_DELTA_BALANCE: f64 = 0.9;
/// Minimum width change required to send a SetWidth command.
pub const MIN_SEND_DELTA_WIDTH: f64 = 0.028;
/// Fader position deviation (0.0–1.0) required to declare a manual override.
/// ~2% of full travel — equivalent to a few dB near unity, much more at extremes.
pub const OVERRIDE_THRESHOLD_POS: f64 = 0.02;
pub const PAN_OVERRIDE_THRESHOLD: f64 = 1.8;
pub const PAN_OVERRIDE_CONFIRMATION_COUNT: u8 = 2;

pub(crate) struct ActiveTarget {
    pub(crate) key: FadeTargetKey,
    pub(crate) group: i32,
    pub(crate) channel: i32,
    pub(crate) start_value: f64,
    pub(crate) target_value: f64,
    /// Last value sent — for override detection and min-delta suppression.
    pub(crate) expected_value: f64,
    pub(crate) override_deviation_count: u8,
    pub(crate) curve: FadeCurve,
    pub(crate) duration: Duration,
    pub(crate) started_at: Instant,
    pub(crate) expected_generation: Option<u64>,
}

pub(crate) struct ActiveTargetInit {
    pub(crate) key: FadeTargetKey,
    pub(crate) group: i32,
    pub(crate) channel: i32,
    pub(crate) start_value: f64,
    pub(crate) target_value: f64,
    pub(crate) curve: FadeCurve,
    pub(crate) duration: Duration,
    pub(crate) started_at: Instant,
    pub(crate) expected_generation: Option<u64>,
}

impl ActiveTarget {
    pub(crate) fn new(init: ActiveTargetInit) -> Self {
        Self {
            key: init.key,
            group: init.group,
            channel: init.channel,
            start_value: init.start_value,
            target_value: init.target_value,
            expected_value: init.start_value,
            override_deviation_count: 0,
            curve: init.curve,
            duration: init.duration,
            started_at: init.started_at,
            expected_generation: init.expected_generation,
        }
    }

    fn is_fader(&self) -> bool {
        self.key.parameter == FadeParameter::FaderDb
    }

    fn override_threshold(&self) -> Option<f64> {
        match self.key.parameter {
            FadeParameter::FaderDb => Some(OVERRIDE_THRESHOLD_POS),
            FadeParameter::Pan => Some(PAN_OVERRIDE_THRESHOLD),
            FadeParameter::Balance | FadeParameter::Width => None,
        }
    }

    /// Returns the interpolated value at `now`.
    pub(crate) fn value_at(&self, now: Instant) -> f64 {
        let elapsed = now.duration_since(self.started_at).as_secs_f64();
        let t = elapsed / self.duration.as_secs_f64();
        if self.is_fader() {
            interpolate(self.start_value, self.target_value, t, self.curve)
        } else {
            let t = t.clamp(0.0, 1.0);
            self.start_value + (self.target_value - self.start_value) * t
        }
    }

    /// Returns true if the fade has completed (t >= 1.0).
    pub(crate) fn is_done(&self, now: Instant) -> bool {
        now.duration_since(self.started_at) >= self.duration
    }

    /// Returns true if the current parameter value indicates a manual override.
    /// Faders are compared in position space. Pan uses a direct threshold. Balance
    /// and width do not participate in override cancellation.
    pub(crate) fn is_override(&self, reported_value: f64) -> bool {
        if self.is_fader() {
            let reported_pos = db_to_pos(reported_value);
            let expected_pos = db_to_pos(self.expected_value);
            (reported_pos - expected_pos).abs() >= OVERRIDE_THRESHOLD_POS
        } else if let Some(threshold) = self.override_threshold() {
            (reported_value - self.expected_value).abs() >= threshold
        } else {
            false
        }
    }

    /// Records an override report and returns true when override is confirmed.
    pub(crate) fn record_override_report(&mut self, reported_value: f64) -> bool {
        if self.key.parameter != FadeParameter::Pan {
            return self.is_override(reported_value);
        }

        if self.is_override(reported_value) {
            self.override_deviation_count = self.override_deviation_count.saturating_add(1);
            self.override_deviation_count >= PAN_OVERRIDE_CONFIRMATION_COUNT
        } else {
            self.override_deviation_count = 0;
            false
        }
    }

    /// Returns Some(new_value) if the target has moved enough to warrant sending.
    pub(crate) fn next_send(&mut self, now: Instant) -> Option<f64> {
        let new_value = if self.is_done(now) {
            self.target_value
        } else {
            self.value_at(now)
        };

        if self.should_send(new_value) {
            self.expected_value = new_value;
            Some(new_value)
        } else {
            None
        }
    }

    fn should_send(&self, new_value: f64) -> bool {
        let delta_threshold = match self.key.parameter {
            FadeParameter::FaderDb => {
                let new_pos = db_to_pos(new_value);
                let expected_pos = db_to_pos(self.expected_value);
                return (new_pos - expected_pos).abs() >= MIN_SEND_DELTA_POS;
            }
            FadeParameter::Pan => MIN_SEND_DELTA_PAN,
            FadeParameter::Balance => MIN_SEND_DELTA_BALANCE,
            FadeParameter::Width => MIN_SEND_DELTA_WIDTH,
        };
        (new_value - self.expected_value).abs() >= delta_threshold
    }

    pub(crate) fn exact_final_send(&mut self) -> f64 {
        self.expected_value = self.target_value;
        self.target_value
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::types::FadeParameter;
    use std::time::Duration;

    fn make_channel(start_db: f64, target_db: f64, duration_ms: u64) -> ActiveTarget {
        ActiveTarget::new(ActiveTargetInit {
            key: crate::fade::types::FadeTargetKey {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
            },
            group: 0,
            channel: 0,
            start_value: start_db,
            target_value: target_db,
            curve: FadeCurve::Linear,
            duration: Duration::from_millis(duration_ms),
            started_at: Instant::now(),
            expected_generation: None,
        })
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
    fn value_at_midpoint_interpolates_pan_linearly() {
        let ch = ActiveTarget::new(ActiveTargetInit {
            key: crate::fade::types::FadeTargetKey {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
            },
            group: 0,
            channel: 0,
            start_value: -45.0,
            target_value: 45.0,
            curve: FadeCurve::Linear,
            duration: Duration::from_millis(4000),
            started_at: Instant::now(),
            expected_generation: None,
        });

        let mid = ch.started_at + Duration::from_millis(2000);

        assert!((ch.value_at(mid) - 0.0).abs() < 1e-10);
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
    fn is_override_true_when_position_deviation_exceeds_threshold() {
        use crate::fade::fader_law::{db_to_pos, pos_to_db};
        let ch = make_channel(-20.0, -10.0, 4000);
        // expected_db = start_db = -20.0; compute a reported_db far enough in position space
        let expected_pos = db_to_pos(-20.0);
        let over_pos = expected_pos + OVERRIDE_THRESHOLD_POS + 0.001;
        let reported_db = pos_to_db(over_pos);
        assert!(ch.is_override(reported_db));
    }

    #[test]
    fn is_override_false_when_position_deviation_below_threshold() {
        use crate::fade::fader_law::{db_to_pos, pos_to_db};
        let ch = make_channel(-20.0, -10.0, 4000);
        let expected_pos = db_to_pos(-20.0);
        let under_pos = expected_pos + OVERRIDE_THRESHOLD_POS - 0.001;
        let reported_db = pos_to_db(under_pos);
        assert!(!ch.is_override(reported_db));
    }

    #[test]
    fn override_uses_position_space_so_wide_range_is_not_false_positive() {
        use crate::fade::fader_law::db_to_pos;
        let ch = make_channel(-144.0, 0.0, 4000);
        // At -144 dB, the fader law maps large dB gaps to small position gaps.
        // A 5 dB deviation near the bottom should NOT be an override in position space.
        let reported_db = -139.0; // 5 dB above start
        let pos_deviation = (db_to_pos(reported_db) - db_to_pos(-144.0)).abs();
        // Verify position deviation is well below threshold
        assert!(pos_deviation < OVERRIDE_THRESHOLD_POS);
        assert!(!ch.is_override(reported_db));
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
        // At t=1.0 (end), value is -10.0; position delta from -20→-10 is well above MIN_SEND_DELTA_POS
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
        assert!((ch.expected_value - -10.0).abs() < 1e-10);
    }

    #[test]
    fn next_send_at_done_returns_exact_target() {
        let mut ch = make_channel(-20.0, -10.0, 4000);
        let end = ch.started_at + Duration::from_millis(5000);
        let result = ch.next_send(end).unwrap();
        assert!((result - -10.0).abs() < 1e-10);
    }

    fn make_pan_family_target(parameter: FadeParameter) -> ActiveTarget {
        ActiveTarget::new(ActiveTargetInit {
            key: crate::fade::types::FadeTargetKey {
                group: 0,
                channel: 0,
                parameter,
            },
            group: 0,
            channel: 0,
            start_value: 0.0,
            target_value: 10.0,
            curve: FadeCurve::Linear,
            duration: Duration::from_millis(4000),
            started_at: Instant::now(),
            expected_generation: None,
        })
    }

    #[test]
    fn next_send_pan_respects_minimum_send_delta() {
        let mut target = make_pan_family_target(FadeParameter::Pan);
        assert!(target.next_send(target.started_at).is_none());

        let later = target.started_at + Duration::from_millis(400);
        assert!(target.next_send(later).is_some());
    }

    #[test]
    fn pan_override_requires_two_consecutive_out_of_threshold_reports() {
        let mut target = make_pan_family_target(FadeParameter::Pan);

        assert!(!target.record_override_report(2.0));
        assert_eq!(target.override_deviation_count, 1);

        assert!(target.record_override_report(2.0));
        assert_eq!(
            target.override_deviation_count,
            PAN_OVERRIDE_CONFIRMATION_COUNT
        );
    }

    #[test]
    fn pan_override_confirmation_resets_after_in_threshold_report() {
        let mut target = make_pan_family_target(FadeParameter::Pan);

        assert!(!target.record_override_report(2.0));
        assert_eq!(target.override_deviation_count, 1);

        assert!(!target.record_override_report(1.0));
        assert_eq!(target.override_deviation_count, 0);

        assert!(!target.record_override_report(2.0));
        assert_eq!(target.override_deviation_count, 1);
    }

    #[test]
    fn fader_override_remains_immediate() {
        use crate::fade::fader_law::{db_to_pos, pos_to_db};

        let mut target = make_channel(-20.0, -10.0, 4000);
        let expected_pos = db_to_pos(-20.0);
        let over_pos = expected_pos + OVERRIDE_THRESHOLD_POS + 0.001;
        let reported_db = pos_to_db(over_pos);

        assert!(target.record_override_report(reported_db));
        assert_eq!(target.override_deviation_count, 0);
    }

    #[test]
    fn next_send_balance_respects_minimum_send_delta() {
        let mut target = make_pan_family_target(FadeParameter::Balance);
        assert!(target.next_send(target.started_at).is_none());

        let later = target.started_at + Duration::from_millis(400);
        assert!(target.next_send(later).is_some());
    }

    #[test]
    fn next_send_width_respects_minimum_send_delta() {
        let mut target = make_pan_family_target(FadeParameter::Width);
        assert!(target.next_send(target.started_at).is_none());

        let later = target.started_at + Duration::from_millis(200);
        assert!(target.next_send(later).is_some());
    }
}
