# Measured Fader Law Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Recalibrate the LV1 fader-law lookup table from measured full-travel pixel distances.

**Architecture:** `src/fade/fader_law.rs` remains the single source of truth for translating between normalized fader position and dB. The change replaces approximate normalized knots with measured pixel-derived knots while preserving the existing piecewise linear interpolation API used by `src/fade/curve.rs` and the fade engine.

**Tech Stack:** Rust, Cargo unit tests, existing `fade` module.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src/fade/fader_law.rs` | Modify | Store measured fader-law knots and unit tests for position/dB conversion. |
| `docs/superpowers/specs/2026-06-06-measured-fader-law-design.md` | Reference only | Approved calibration source measurements and normalized knot formulas. |

No new production files are needed. `src/fade/curve.rs` and `src/fade/engine.rs` should not change because they already consume `db_to_pos` and `pos_to_db` through the existing API.

---

### Task 1: Recalibrate Fader-Law Knots

**Files:**
- Modify: `src/fade/fader_law.rs`

- [ ] **Step 1: Write failing tests for measured dB positions**

In `src/fade/fader_law.rs`, add this helper and test inside the existing `#[cfg(test)] mod tests` block, after `fn db_to_pos_at_plus_10_is_1()`:

```rust
    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn db_to_pos_uses_measured_pixel_law() {
        assert_close(db_to_pos(-144.0), 0.0 / 516.0);
        assert_close(db_to_pos(-60.0), 29.0 / 516.0);
        assert_close(db_to_pos(-50.0), 59.0 / 516.0);
        assert_close(db_to_pos(-30.0), 151.0 / 516.0);
        assert_close(db_to_pos(-20.0), 211.0 / 516.0);
        assert_close(db_to_pos(-10.0), 272.0 / 516.0);
        assert_close(db_to_pos(-5.0), 333.0 / 516.0);
        assert_close(db_to_pos(0.0), 394.0 / 516.0);
        assert_close(db_to_pos(5.0), 455.0 / 516.0);
        assert_close(db_to_pos(10.0), 516.0 / 516.0);
    }
```

- [ ] **Step 2: Run the fader-law test and verify it fails**

Run:

```bash
cargo test --lib fade::fader_law::tests::db_to_pos_uses_measured_pixel_law
```

Expected: FAIL because the current approximate table returns old positions such as `0.75` for `0 dB`, not `394.0 / 516.0`.

- [ ] **Step 3: Replace the knot table with measured normalized positions**

In `src/fade/fader_law.rs`, replace the existing `KNOTS` constant with:

```rust
/// Lookup table: (normalized_position, gain_db) knots, sorted by position ascending.
/// Positions are derived from measured LV1 fader pixel distances over 516 px of travel.
const KNOTS: &[(f64, f64)] = &[
    (0.0 / 516.0, -144.0),
    (29.0 / 516.0, -60.0),
    (59.0 / 516.0, -50.0),
    (151.0 / 516.0, -30.0),
    (211.0 / 516.0, -20.0),
    (272.0 / 516.0, -10.0),
    (333.0 / 516.0, -5.0),
    (394.0 / 516.0, 0.0),
    (455.0 / 516.0, 5.0),
    (516.0 / 516.0, 10.0),
];
```

- [ ] **Step 4: Update existing tests that assert old approximate positions**

In `src/fade/fader_law.rs`, replace `db_to_pos_at_0_is_0_75` with:

```rust
    #[test]
    fn db_to_pos_at_0_uses_measured_position() {
        assert!((db_to_pos(0.0) - 394.0 / 516.0).abs() < 1e-10);
    }
```

Update `pos_to_db_at_unity_is_0` because the measured `0 dB` position is no longer `0.750`:

```rust
    #[test]
    fn pos_to_db_at_measured_unity_is_0() {
        assert!((pos_to_db(394.0 / 516.0) - 0.0).abs() < 1e-10);
    }
```

Update `pos_to_db_midpoint_of_segment_is_midpoint_db` to use a measured segment midpoint:

```rust
    #[test]
    fn pos_to_db_midpoint_of_segment_is_midpoint_db() {
        // Between measured -10 dB and -5 dB positions, midpoint maps to -7.5 dB.
        let mid_pos = ((272.0 / 516.0) + (333.0 / 516.0)) / 2.0;
        let v = pos_to_db(mid_pos);
        assert!((v - -7.5).abs() < 1e-10);
    }
```

- [ ] **Step 5: Run fader-law tests and verify they pass**

Run:

```bash
cargo test --lib fade::fader_law
```

Expected: PASS for all `fade::fader_law` tests.

- [ ] **Step 6: Run fade-curve tests and verify the API consumer still passes**

Run:

```bash
cargo test --lib fade::curve
```

Expected: PASS for all `fade::curve` tests. The exact midpoint value changes through `pos_to_db`, but the test computes its expected value through the fader-law API.

- [ ] **Step 7: Run the full library test suite**

Run:

```bash
cargo test --lib
```

Expected: PASS.

- [ ] **Step 8: Review the diff**

Run:

```bash
git diff -- src/fade/fader_law.rs
```

Expected: the diff only updates the `KNOTS` positions, comments documenting the measured law, and tests for the new measured positions.

- [ ] **Step 9: Commit the implementation**

Run:

```bash
git status --short
git add src/fade/fader_law.rs docs/superpowers/plans/2026-06-06-measured-fader-law.md
git commit -m "fix: calibrate LV1 fader law from measurements"
```

Expected: a commit containing the fader-law implementation and this implementation plan. Do not stage unrelated modified or untracked files.

---

## Self-Review Notes

Spec coverage: the plan implements the measured `516 px` full-travel calibration, updates `src/fade/fader_law.rs` only, preserves piecewise interpolation, keeps the fade engine unchanged, and verifies fader-law plus fade-curve tests.

Placeholder scan: no placeholders or deferred implementation steps remain.

Type consistency: the plan uses the existing `db_to_pos(db: f64) -> f64` and `pos_to_db(pos: f64) -> f64` APIs throughout.
