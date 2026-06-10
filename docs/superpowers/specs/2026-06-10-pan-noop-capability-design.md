# PAN No-Op Capability Design

## Context

Scene PAN scope currently treats stored `PanMode::Stereo` as a guarantee that pan, balance, and width values must all exist. Hardware logs show that assumption is too strict:

- `group=12` Link channels and `group=24` `HidLink:0` report pan mode `i:0`, meaning no pan controls.
- `group=3 channel=0` LR reports pan mode `i:2`, but its width notification is inactive and the surface does not expose a pan knob.
- Width is not present in `/Channels`; it only becomes usable from active `/Notify/PanArcWidth` messages.

PAN scope should therefore model available pan-family values, not block recall when a scoped channel has no applicable pan controls.

## Desired Behavior

- Add an explicit no-pan mode for LV1 pan mode `i:0`.
- `PanMode::None` contributes no PAN targets and does not block recall.
- `PanMode::Mono` contributes a pan target only when a stored pan value exists.
- `PanMode::Stereo` contributes each available stored target independently: pan, balance, and width.
- Missing pan-family values are treated as no-op for that parameter, not recall blockers.
- PAN-only scenes with no applicable targets should no-op rather than block.
- FADER scope behavior is unchanged. Fader targets still validate strictly when fader scope is enabled.

## Safety Model

This change does not send guessed pan-family values. If LV1 did not provide a stored pan, balance, or active width value, the app sends nothing for that parameter.

This avoids the unsafe alternative of defaulting missing stereo width to `1.0`, which could move an unobserved control.

## Recall Policy

Recall validation should still block for connection, lockout, scene identity mismatch, missing live topology, and missing fader values when FADER scope is enabled.

For PAN scope, recall should build targets only from stored values that are present and applicable. If PAN scope produces no targets, recall should continue if another enabled scope produced targets. If no enabled scope produces any targets, recall should return a no-op skip rather than a blocked safety failure.

## UI Implications

The frontend should label Link channels instead of grouping them as `Unknown` once the group mapping is updated:

- `group=12`: Links
- `group=24`: hidden/internal link entry, not user-facing if it appears in topology

PAN summaries may show no pan values for channels that do not expose pan-family controls.

## Tests

Add regression coverage for:

- `i:0` parses as explicit no-pan mode.
- PAN recall skips no-pan scoped channels without blocking.
- PAN-only recall with only no-op channels skips/no-ops instead of blocking.
- Stereo channels with missing width recall available pan and balance targets only.
- Existing fader-scope strict validation remains unchanged.
