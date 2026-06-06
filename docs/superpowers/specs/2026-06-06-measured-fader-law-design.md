# Measured Fader Law Calibration Design

## Purpose

Tighten the LV1 fader law used by fade interpolation. The current lookup table uses approximate normalized positions. New pixel measurements cover the full LV1 fader travel from `+10 dB` at the top to `-144 dB` at the bottom, so the table should be recalibrated from those measured segment distances.

## Source Measurements

Measured segment distances, top to bottom:

| Segment | Pixels |
|---|---:|
| `+10` to `+5` | 61 |
| `+5` to `0` | 61 |
| `0` to `-5` | 61 |
| `-5` to `-10` | 61 |
| `-10` to `-20` | 61 |
| `-20` to `-30` | 60 |
| `-30` to `-50` | 92 |
| `-50` to `-60` | 30 |
| `-60` to `-144` | 29 |

Total measured travel: `516 px`.

## Design

Use these measurements as the authoritative LV1 fader scale. Convert cumulative distance from the bottom into normalized fader position where `0.0` is `-144 dB` and `1.0` is `+10 dB`.

The resulting knots are:

| dB | Position formula | Normalized position |
|---:|---|---:|
| `-144` | `0 / 516` | `0.000000` |
| `-60` | `29 / 516` | `0.056202` |
| `-50` | `59 / 516` | `0.114341` |
| `-30` | `151 / 516` | `0.292636` |
| `-20` | `211 / 516` | `0.408915` |
| `-10` | `272 / 516` | `0.527132` |
| `-5` | `333 / 516` | `0.645349` |
| `0` | `394 / 516` | `0.763566` |
| `+5` | `455 / 516` | `0.881783` |
| `+10` | `516 / 516` | `1.000000` |

`src/fade/fader_law.rs` remains the single source of truth. `db_to_pos` and `pos_to_db` continue to use piecewise linear interpolation between knots. The fade engine does not change because it already interpolates in fader position space.

## Testing

Update fader-law tests to assert the measured positions for key dB marks, especially `0 dB`, `-20 dB`, `-30 dB`, `-50 dB`, and `-60 dB`. Keep existing clamp and roundtrip tests.

Run the fader-law and fade-curve unit tests after the change.
