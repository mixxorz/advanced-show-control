//! Fade curve interpolation.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FadeCurve {
    LinearDb,
    EaseInOutDb,
}

pub fn interpolate(start_db: f64, target_db: f64, t: f64, curve: FadeCurve) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let t_shaped = match curve {
        FadeCurve::LinearDb => t,
        FadeCurve::EaseInOutDb => t * t * (3.0 - 2.0 * t),
    };
    start_db + (target_db - start_db) * t_shaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_db_at_t0_returns_start() {
        assert_eq!(interpolate(-20.0, -10.0, 0.0, FadeCurve::LinearDb), -20.0);
    }

    #[test]
    fn linear_db_at_t1_returns_target() {
        assert_eq!(interpolate(-20.0, -10.0, 1.0, FadeCurve::LinearDb), -10.0);
    }

    #[test]
    fn linear_db_at_midpoint_is_halfway() {
        let v = interpolate(-20.0, -10.0, 0.5, FadeCurve::LinearDb);
        assert!((v - -15.0).abs() < 1e-10);
    }

    #[test]
    fn ease_in_out_at_t0_returns_start() {
        assert_eq!(interpolate(-20.0, -10.0, 0.0, FadeCurve::EaseInOutDb), -20.0);
    }

    #[test]
    fn ease_in_out_at_t1_returns_target() {
        assert_eq!(interpolate(-20.0, -10.0, 1.0, FadeCurve::EaseInOutDb), -10.0);
    }

    #[test]
    fn ease_in_out_at_midpoint_is_halfway() {
        // smoothstep(0.5) = 0.5 * 0.5 * (3 - 2*0.5) = 0.25 * 2 = 0.5
        let v = interpolate(-20.0, -10.0, 0.5, FadeCurve::EaseInOutDb);
        assert!((v - -15.0).abs() < 1e-10);
    }

    #[test]
    fn ease_in_out_has_slow_start() {
        // At t=0.1, ease-in-out should be less than linear
        let linear = interpolate(0.0, 1.0, 0.1, FadeCurve::LinearDb);
        let eased = interpolate(0.0, 1.0, 0.1, FadeCurve::EaseInOutDb);
        assert!(eased < linear);
    }

    #[test]
    fn ease_in_out_has_slow_end() {
        // At t=0.9, ease-in-out should be greater than linear (closer to target)
        let linear = interpolate(0.0, 1.0, 0.9, FadeCurve::LinearDb);
        let eased = interpolate(0.0, 1.0, 0.9, FadeCurve::EaseInOutDb);
        assert!(eased > linear);
    }

    #[test]
    fn clamps_t_below_zero() {
        assert_eq!(interpolate(-20.0, -10.0, -1.0, FadeCurve::LinearDb), -20.0);
    }

    #[test]
    fn clamps_t_above_one() {
        assert_eq!(interpolate(-20.0, -10.0, 2.0, FadeCurve::LinearDb), -10.0);
    }

    #[test]
    fn works_with_fade_up() {
        let v = interpolate(-30.0, -10.0, 0.5, FadeCurve::LinearDb);
        assert!((v - -20.0).abs() < 1e-10);
    }

    #[test]
    fn works_with_fade_down() {
        let v = interpolate(-10.0, -30.0, 0.5, FadeCurve::LinearDb);
        assert!((v - -20.0).abs() < 1e-10);
    }
}
