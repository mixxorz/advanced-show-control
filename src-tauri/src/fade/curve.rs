//! Fade curve interpolation in fader position space.

use crate::fade::fader_law::{db_to_pos, pos_to_db};

/// Fade curve — controls how the fader position moves over time.
///
/// "Linear" means linear in fader position (using the LV1 fader law),
/// not linear in dB. This matches the perceptual feel of moving a physical fader.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FadeCurve {
    Linear,
}

/// Interpolate between `start_db` and `target_db` at normalized time `t` (0.0–1.0).
///
/// Interpolation happens in fader position space so that the movement feels
/// linear from the fader's perspective rather than the dB scale.
pub fn interpolate(start_db: f64, target_db: f64, t: f64, _curve: FadeCurve) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let start_pos = db_to_pos(start_db);
    let target_pos = db_to_pos(target_db);
    let pos = start_pos + (target_pos - start_pos) * t;
    pos_to_db(pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_t0_returns_start() {
        let v = interpolate(-20.0, -10.0, 0.0, FadeCurve::Linear);
        assert!((v - -20.0).abs() < 1e-10);
    }

    #[test]
    fn at_t1_returns_target() {
        let v = interpolate(-20.0, -10.0, 1.0, FadeCurve::Linear);
        assert!((v - -10.0).abs() < 1e-10);
    }

    #[test]
    fn at_t0_with_wide_range_returns_start() {
        let v = interpolate(-144.0, 0.0, 0.0, FadeCurve::Linear);
        assert!((v - -144.0).abs() < 1e-10);
    }

    #[test]
    fn at_t1_with_wide_range_returns_target() {
        let v = interpolate(-144.0, 0.0, 1.0, FadeCurve::Linear);
        assert!((v - 0.0).abs() < 1e-10);
    }

    #[test]
    fn midpoint_is_linear_in_position_space() {
        // Midpoint in position space should be derived from the measured fader law.
        use crate::fade::fader_law::{db_to_pos, pos_to_db};
        let expected_pos = db_to_pos(-144.0) + (db_to_pos(0.0) - db_to_pos(-144.0)) * 0.5;
        let expected = pos_to_db(expected_pos);
        let v = interpolate(-144.0, 0.0, 0.5, FadeCurve::Linear);
        assert!((v - expected).abs() < 1e-10);
    }

    #[test]
    fn midpoint_is_not_midpoint_in_db_space() {
        // Confirms fader-law interpolation differs from dB interpolation for wide ranges
        let fader_mid = interpolate(-144.0, 0.0, 0.5, FadeCurve::Linear);
        let db_mid = -72.0; // naive dB midpoint
        assert!((fader_mid - db_mid).abs() > 1.0);
    }

    #[test]
    fn clamps_t_below_zero() {
        let v = interpolate(-20.0, -10.0, -1.0, FadeCurve::Linear);
        assert!((v - -20.0).abs() < 1e-10);
    }

    #[test]
    fn clamps_t_above_one() {
        let v = interpolate(-20.0, -10.0, 2.0, FadeCurve::Linear);
        assert!((v - -10.0).abs() < 1e-10);
    }

    #[test]
    fn works_with_fade_up() {
        let v = interpolate(-30.0, -10.0, 0.5, FadeCurve::Linear);
        // Should be between start and target
        assert!(v > -30.0 && v < -10.0);
    }

    #[test]
    fn works_with_fade_down() {
        let v = interpolate(-10.0, -30.0, 0.5, FadeCurve::Linear);
        assert!(v > -30.0 && v < -10.0);
    }
}
