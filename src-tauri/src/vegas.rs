use crate::fade::pos_to_db;

pub const WAVE_WIDTH_FADERS: f64 = 8.0;
pub const GROUP_STRIDE: i32 = 128;
pub const PHASE_STEP: f64 = std::f64::consts::TAU / 32.0;

pub fn stable_index(group: i32, channel: i32) -> i32 {
    group * GROUP_STRIDE + channel
}

pub fn fader_position_at(group: i32, channel: i32, tick: u64) -> f64 {
    let index = stable_index(group, channel) as f64;
    let phase = (index / WAVE_WIDTH_FADERS) * std::f64::consts::TAU + tick as f64 * PHASE_STEP;
    ((phase.sin() + 1.0) / 2.0).clamp(0.0, 1.0)
}

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
    fn gain_db_matches_measured_fader_law() {
        for group in 0..=8 {
            for channel in 0..=128 {
                for tick in 0..=64 {
                    let position = fader_position_at(group, channel, tick);
                    let expected = pos_to_db(position);
                    assert_close(gain_db_at(group, channel, tick), expected);
                }
            }
        }
    }
}
