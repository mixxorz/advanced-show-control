use crate::fade::pos_to_db;

pub const WAVE_WIDTH_FADERS: f64 = 8.0;
pub const GROUP_STRIDE: i32 = 128;
pub const PHASE_STEP: f64 = std::f64::consts::TAU / 32.0;

/// @cc [owner:mixxorz,label:testing] vegas-stable-channel-index
/// For LV1 identities with `group` in `0..=12` and `channel` in `0..=127`, the same pair MUST map to
/// `group * 128 + channel` so Vegas animation phases remain stable across runs and callers.
pub fn stable_index(group: i32, channel: i32) -> i32 {
    group * GROUP_STRIDE + channel
}

/// @cc [owner:mixxorz,label:testing] vegas-explicit-time-wave
/// For LV1 identities with `group` in `0..=12` and `channel` in `0..=127`, the output MUST be a
/// deterministic unit-interval fader position derived only from group, channel, and the explicit
/// tick. Channel indices eight apart MUST have equivalent phase modulo `TAU` at the same tick, and
/// each tick MUST advance phase by `TAU / 32`.
pub fn fader_position_at(group: i32, channel: i32, tick: u64) -> f64 {
    let index = stable_index(group, channel) as f64;
    let phase = (index / WAVE_WIDTH_FADERS) * std::f64::consts::TAU + tick as f64 * PHASE_STEP;
    ((phase.sin() + 1.0) / 2.0).clamp(0.0, 1.0)
}

/// @cc [owner:mixxorz,label:testing;lv1] vegas-measured-fader-law
/// For LV1 identities with `group` in `0..=12` and `channel` in `0..=127`, the deterministic Vegas
/// position MUST be converted to gain with the production measured LV1 fader law; this helper MUST
/// NOT introduce a separate gain curve.
pub fn gain_db_at(group: i32, channel: i32, tick: u64) -> f64 {
    pos_to_db(fader_position_at(group, channel, tick))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-10,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn stable_index_uses_group_stride_of_128() {
        assert_eq!(stable_index(0, 0), 0);
        assert_eq!(stable_index(0, 7), 7);
        assert_eq!(stable_index(1, 0), 128);
    }

    #[test]
    fn faders_eight_apart_share_phase_at_same_tick() {
        assert_close(fader_position_at(0, 0, 0), fader_position_at(0, 8, 0));
        assert_close(fader_position_at(2, 3, 17), fader_position_at(2, 11, 17));
    }

    #[test]
    fn tick_advancement_changes_phase() {
        let a = fader_position_at(4, 9, 0);
        let b = fader_position_at(4, 9, 1);

        assert!(
            (a - b).abs() > 1e-12,
            "expected different values, got {a} and {b}"
        );
    }

    #[test]
    fn fader_position_stays_within_unit_interval() {
        for group in 0..=8 {
            for channel in 0..=128 {
                for tick in 0..=64 {
                    let position = fader_position_at(group, channel, tick);
                    assert!(
                        (0.0..=1.0).contains(&position),
                        "out of range for group={group}, channel={channel}, tick={tick}: {position}"
                    );
                }
            }
        }
    }

    #[test]
    fn gain_db_uses_measured_fader_law() {
        assert_close(gain_db_at(0, 0, 0), -12.295081967213115);
        assert_close(gain_db_at(0, 2, 0), 10.0);
        assert_close(gain_db_at(0, 6, 0), -144.0);
        assert_close(gain_db_at(0, 0, 8), 10.0);
    }
}
