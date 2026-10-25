use std::time::Duration;
use tokio::time::Instant;

use crate::fade::curve::{FadeCurve, interpolate};
use crate::fade::fader_law::db_to_pos;
use crate::fade::types::{FadeParameter, FadeSceneIdentity, FadeTargetKey};

pub const TICK_HZ: u64 = 25;
/// Minimum fader position change (0.0–1.0) required to send a SetGain command.
pub const MIN_SEND_DELTA_POS: f64 = 0.002;
/// Minimum pan change (±100) required to include a pan write in a batch.
pub const MIN_SEND_DELTA_PAN: f64 = 0.9;
/// Minimum balance change (±100) required to include a balance write in a batch.
pub const MIN_SEND_DELTA_BALANCE: f64 = 0.9;
/// Minimum width change required to include a width write in a batch.
pub const MIN_SEND_DELTA_WIDTH: f64 = 0.028;
/// Fader position deviation (0.0–1.0) required to declare a manual override.
/// ~2% of full travel — equivalent to a few dB near unity, much more at extremes.
pub const OVERRIDE_THRESHOLD_POS: f64 = 0.02;
pub const PAN_OVERRIDE_THRESHOLD: f64 = 1.8;
pub const PAN_OVERRIDE_CONFIRMATION_COUNT: u8 = 2;

pub(crate) struct ActiveTarget {
    pub(crate) scene: FadeSceneIdentity,
    pub(crate) key: FadeTargetKey,
    pub(crate) start_value: f64,
    pub(crate) target_value: f64,
    /// Last value sent — for override detection and min-delta suppression.
    pub(crate) expected_value: f64,
    pub(crate) override_deviation_count: u8,
    pub(crate) curve: FadeCurve,
    pub(crate) duration: Duration,
    pub(crate) started_at: Instant,
    paused_since: Option<Instant>,
    finish_requested: bool,
}

pub(crate) struct ActiveTargetInit {
    pub(crate) scene: FadeSceneIdentity,
    pub(crate) key: FadeTargetKey,
    pub(crate) start_value: f64,
    pub(crate) target_value: f64,
    pub(crate) curve: FadeCurve,
    pub(crate) duration: Duration,
    pub(crate) started_at: Instant,
}

impl ActiveTarget {
    pub(crate) fn new(init: ActiveTargetInit) -> Self {
        Self {
            scene: init.scene,
            key: init.key,
            start_value: init.start_value,
            target_value: init.target_value,
            expected_value: init.start_value,
            override_deviation_count: 0,
            curve: init.curve,
            duration: init.duration,
            started_at: init.started_at,
            paused_since: None,
            finish_requested: false,
        }
    }

    pub(super) fn pause(&mut self, now: Instant) {
        if self.paused_since.is_none() {
            self.paused_since = Some(now);
        }
    }

    pub(super) fn resume(&mut self, now: Instant) {
        if let Some(paused_since) = self.paused_since.take() {
            self.started_at += now.duration_since(paused_since);
        }
    }

    pub(crate) fn finish_on_next_tick(&mut self) {
        self.finish_requested = true;
    }

    #[cfg(test)]
    pub(super) fn is_paused(&self) -> bool {
        self.paused_since.is_some()
    }

    /// @cc [owner:mixxorz,label:product] parameter-interpolation
    /// `value_at` MUST clamp normalized elapsed progress to `[0, 1]`, return the finite exact target
    /// for zero duration, interpolate faders in measured fader-position space, and interpolate Pan,
    /// Balance, and Width linearly in their native value space.
    pub(crate) fn value_at(&self, now: Instant) -> f64 {
        if self.duration.is_zero() {
            return self.target_value;
        }

        let elapsed = now.duration_since(self.started_at).as_secs_f64();
        let t = elapsed / self.duration.as_secs_f64();
        if self.key.parameter == FadeParameter::FaderDb {
            interpolate(self.start_value, self.target_value, t, self.curve)
        } else {
            let t = t.clamp(0.0, 1.0);
            self.start_value + (self.target_value - self.start_value) * t
        }
    }

    /// Returns true if the fade has completed (t >= 1.0).
    pub(crate) fn is_done(&self, now: Instant) -> bool {
        self.finish_requested || now.duration_since(self.started_at) >= self.duration
    }

    /// @cc [owner:mixxorz,label:safety;product] override-thresholds
    /// Fader override detection MUST compare reported and expected values in measured position
    /// space, Pan MUST compare native values, and Balance and Width MUST never independently declare
    /// an override.
    pub(crate) fn is_override(&self, reported_value: f64) -> bool {
        match self.key.parameter {
            FadeParameter::FaderDb => {
                let reported_pos = db_to_pos(reported_value);
                let expected_pos = db_to_pos(self.expected_value);
                (reported_pos - expected_pos).abs() >= OVERRIDE_THRESHOLD_POS
            }
            FadeParameter::Pan => {
                (reported_value - self.expected_value).abs() >= PAN_OVERRIDE_THRESHOLD
            }
            FadeParameter::Balance | FadeParameter::Width => false,
        }
    }

    /// @cc [owner:mixxorz,label:safety;product] pan-override-confirmation
    /// Pan MUST require two consecutive out-of-threshold reports to confirm override and MUST reset
    /// that evidence on an in-threshold report; fader override confirmation MUST remain immediate.
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

    /// @cc [owner:mixxorz,label:product;safety] parameter-send-proposal
    /// `proposed_send` MUST suppress values whose change from `expected_value` is below the
    /// parameter's minimum delta, comparing faders in measured position space and Pan, Balance, and
    /// Width in native value space, and MUST NOT mutate `expected_value` before mailbox admission.
    pub(crate) fn proposed_send(&self, now: Instant) -> Option<f64> {
        let new_value = if self.is_done(now) {
            self.target_value
        } else {
            self.value_at(now)
        };
        self.should_send(new_value).then_some(new_value)
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn scene(index: i32, name: &str) -> FadeSceneIdentity {
        FadeSceneIdentity {
            index,
            name: name.to_string(),
        }
    }

    fn make_channel(start_db: f64, target_db: f64, duration_ms: u64) -> ActiveTarget {
        ActiveTarget::new(ActiveTargetInit {
            scene: scene(17, "Verse"),
            key: crate::fade::types::FadeTargetKey {
                group: 0,
                channel: 0,
                parameter: FadeParameter::FaderDb,
            },
            start_value: start_db,
            target_value: target_db,
            curve: FadeCurve::Linear,
            duration: Duration::from_millis(duration_ms),
            started_at: Instant::now(),
        })
    }

    #[test]
    fn zero_duration_value_at_is_finite_target() {
        let target = make_channel(-20.0, -10.0, 0);

        let value = target.value_at(target.started_at);

        assert!(value.is_finite());
        assert_eq!(value, -10.0);
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
            scene: scene(17, "Verse"),
            key: crate::fade::types::FadeTargetKey {
                group: 0,
                channel: 0,
                parameter: FadeParameter::Pan,
            },
            start_value: -45.0,
            target_value: 45.0,
            curve: FadeCurve::Linear,
            duration: Duration::from_millis(4000),
            started_at: Instant::now(),
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
    fn instant_finish_preserves_target_ownership_and_exact_value() {
        let mut target = make_channel(-20.0, -10.0, 4_000);
        let owner = target.scene.clone();
        let key = target.key.clone();

        target.finish_on_next_tick();

        assert!(target.is_done(Instant::now()));
        assert_eq!(target.proposed_send(target.started_at), Some(-10.0));
        assert_eq!(target.scene, owner);
        assert_eq!(target.key, key);
    }

    #[test]
    fn instant_finish_remains_done_after_paused_target_resumes() {
        let now = Instant::now();
        let mut target = make_channel(-20.0, -10.0, 4_000);
        target.pause(now);

        target.finish_on_next_tick();
        target.resume(now + Duration::from_secs(2));

        assert!(target.is_done(now + Duration::from_secs(2)));
        assert_eq!(
            target.proposed_send(now + Duration::from_secs(2)),
            Some(-10.0)
        );
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
    fn proposed_send_compares_faders_in_position_space_without_committing() {
        use crate::fade::fader_law::pos_to_db;

        for start_db in [-100.0, -20.0] {
            for direction in [-1.0, 1.0] {
                for (delta, should_send) in [(0.0019, false), (0.0021, true)] {
                    let target_db = pos_to_db(db_to_pos(start_db) + direction * delta);
                    let target = make_channel(start_db, target_db, 4_000);
                    assert_eq!(target.proposed_send(target.started_at), None);
                    for elapsed_ms in [4_000, 5_000] {
                        assert_eq!(
                            target.proposed_send(
                                target.started_at + Duration::from_millis(elapsed_ms)
                            ),
                            should_send.then_some(target_db),
                            "start={start_db}, direction={direction}, position delta={delta}"
                        );
                        assert_eq!(target.expected_value, start_db);
                    }
                }
            }
        }
    }

    #[test]
    fn proposed_send_uses_each_pan_family_native_threshold_without_committing() {
        for (parameter, below, boundary, above) in [
            (FadeParameter::Pan, 0.899, 0.9, 0.901),
            (FadeParameter::Balance, 0.899, 0.9, 0.901),
            (FadeParameter::Width, 0.0279, 0.028, 0.0281),
        ] {
            for direction in [-1.0, 1.0] {
                for (delta, should_send) in [(below, false), (boundary, true), (above, true)] {
                    let mut target = make_pan_family_target(parameter);
                    target.target_value = direction * delta;
                    assert_eq!(target.proposed_send(target.started_at), None);
                    assert_eq!(
                        target.proposed_send(target.started_at + target.duration),
                        should_send.then_some(target.target_value),
                        "{parameter:?}, direction={direction}, native delta={delta}"
                    );
                    assert_eq!(target.expected_value, 0.0);
                }
            }
        }
    }

    #[test]
    fn proposed_send_returns_intermediate_values_without_committing() {
        for (parameter, start, target, midpoint) in [
            (FadeParameter::FaderDb, -20.0, -10.0, -15.0),
            (FadeParameter::Pan, 0.0, 10.0, 5.0),
            (FadeParameter::Balance, 0.0, 10.0, 5.0),
            (FadeParameter::Width, 0.0, 1.0, 0.5),
        ] {
            let mut active = make_channel(start, target, 4_000);
            active.key.parameter = parameter;
            let proposed = active
                .proposed_send(active.started_at + Duration::from_millis(2_000))
                .expect("intermediate change should exceed the parameter threshold");
            assert!((proposed - midpoint).abs() < 1e-10, "{parameter:?}");
            assert_eq!(active.expected_value, start);
        }
    }

    fn make_pan_family_target(parameter: FadeParameter) -> ActiveTarget {
        ActiveTarget::new(ActiveTargetInit {
            scene: scene(17, "Verse"),
            key: crate::fade::types::FadeTargetKey {
                group: 0,
                channel: 0,
                parameter,
            },
            start_value: 0.0,
            target_value: 10.0,
            curve: FadeCurve::Linear,
            duration: Duration::from_millis(4000),
            started_at: Instant::now(),
        })
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
    fn resume_rebases_started_at_by_the_full_pause_duration() {
        let mut target = make_channel(-20.0, -10.0, 1_000);
        let started_at = target.started_at;
        target.pause(started_at + Duration::from_millis(400));
        assert!(target.is_paused());
        target.resume(started_at + Duration::from_millis(2_400));

        assert!(!target.is_paused());
        assert!((target.value_at(started_at + Duration::from_millis(2_400)) - -16.0).abs() < 1e-10);
        assert!(!target.is_done(started_at + Duration::from_millis(2_900)));
        assert!(target.is_done(started_at + Duration::from_millis(3_000)));
    }

    #[test]
    fn repeated_pause_keeps_the_original_pause_boundary() {
        let mut target = make_channel(-20.0, -10.0, 1_000);
        let started_at = target.started_at;
        target.pause(started_at + Duration::from_millis(200));
        target.pause(started_at + Duration::from_millis(600));
        target.resume(started_at + Duration::from_millis(1_200));

        assert_eq!(target.started_at, started_at + Duration::from_millis(1_000));
    }
}
