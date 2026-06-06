//! LV1 fader law: piecewise linear lookup between (position, dB) knots.
//!
//! Position is normalized 0.0–1.0 (fader bottom to top).
//! Based on empirical analysis of the Waves LV1 fader response.

/// Lookup table: (normalized_position, gain_db) knots, sorted by position ascending.
const KNOTS: &[(f64, f64)] = &[
    (0.000, -144.0),
    (0.050, -60.0),
    (0.100, -50.0),
    (0.300, -30.0),
    (0.400, -20.0),
    (0.500, -10.0),
    (0.625, -5.0),
    (0.750, 0.0),
    (0.875, 5.0),
    (1.000, 10.0),
];

/// Convert a normalized fader position (0.0–1.0) to dB.
/// Clamps input to [0.0, 1.0].
pub fn pos_to_db(pos: f64) -> f64 {
    let pos = pos.clamp(0.0, 1.0);
    // Find the surrounding knots and interpolate linearly between them.
    for i in 1..KNOTS.len() {
        let (p0, db0) = KNOTS[i - 1];
        let (p1, db1) = KNOTS[i];
        if pos <= p1 {
            let t = (pos - p0) / (p1 - p0);
            return db0 + (db1 - db0) * t;
        }
    }
    KNOTS.last().unwrap().1
}

/// Convert a dB value to normalized fader position (0.0–1.0).
/// Clamps input to the table's dB range.
pub fn db_to_pos(db: f64) -> f64 {
    let db_min = KNOTS.first().unwrap().1;
    let db_max = KNOTS.last().unwrap().1;
    let db = db.clamp(db_min, db_max);
    for i in 1..KNOTS.len() {
        let (p0, db0) = KNOTS[i - 1];
        let (p1, db1) = KNOTS[i];
        if db <= db1 {
            let t = (db - db0) / (db1 - db0);
            return p0 + (p1 - p0) * t;
        }
    }
    KNOTS.last().unwrap().0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pos_to_db_at_bottom_is_minus_144() {
        assert!((pos_to_db(0.0) - -144.0).abs() < 1e-10);
    }

    #[test]
    fn pos_to_db_at_top_is_plus_10() {
        assert!((pos_to_db(1.0) - 10.0).abs() < 1e-10);
    }

    #[test]
    fn pos_to_db_at_unity_is_0() {
        assert!((pos_to_db(0.750) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn pos_to_db_midpoint_of_segment_is_midpoint_db() {
        // Between (0.5, -10) and (0.625, -5): midpoint pos=0.5625 → db=-7.5
        let v = pos_to_db(0.5625);
        assert!((v - -7.5).abs() < 1e-10);
    }

    #[test]
    fn db_to_pos_at_minus_144_is_0() {
        assert!((db_to_pos(-144.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn db_to_pos_at_plus_10_is_1() {
        assert!((db_to_pos(10.0) - 1.0).abs() < 1e-10);
    }

    #[test]
    fn db_to_pos_at_0_is_0_75() {
        assert!((db_to_pos(0.0) - 0.750).abs() < 1e-10);
    }

    #[test]
    fn roundtrip_pos_to_db_to_pos() {
        for &(pos, _) in KNOTS {
            let db = pos_to_db(pos);
            let pos2 = db_to_pos(db);
            assert!((pos2 - pos).abs() < 1e-10, "roundtrip failed for pos={pos}: got {pos2}");
        }
    }

    #[test]
    fn roundtrip_db_to_pos_to_db() {
        for &(_, db) in KNOTS {
            let pos = db_to_pos(db);
            let db2 = pos_to_db(pos);
            assert!((db2 - db).abs() < 1e-10, "roundtrip failed for db={db}: got {db2}");
        }
    }

    #[test]
    fn clamps_pos_below_zero() {
        assert!((pos_to_db(-1.0) - -144.0).abs() < 1e-10);
    }

    #[test]
    fn clamps_pos_above_one() {
        assert!((pos_to_db(2.0) - 10.0).abs() < 1e-10);
    }

    #[test]
    fn clamps_db_below_min() {
        assert!((db_to_pos(-200.0) - 0.0).abs() < 1e-10);
    }

    #[test]
    fn clamps_db_above_max() {
        assert!((db_to_pos(100.0) - 1.0).abs() < 1e-10);
    }
}
