# Pan Fade Support Design

## Goal

Add scene-managed fading for pan-family parameters without changing the existing fader safety model.

The feature adds one new scene scope toggle, `PAN`, alongside the existing `FADERS` toggle. Both toggles use the same per-scene scoped channel list. When an LV1 scene is recalled, enabled `FadeParameter` targets for scoped channels fade over the scene duration.

Development should follow “make the change easy, then make the easy change.” First refactor the existing `FadeEngine` so it can represent parameter-aware fade targets while preserving current fader behavior. After that refactor is tested, add pan and width as new parameters using the same engine path.

## Scope

In scope:

- Store pan/balance and width targets for channels in scene configs.
- Add a scene-level `PAN` toggle.
- Fade pan/balance and width over the same scene duration as faders.
- Keep one scoped channel list per scene.
- Make manual override detection parameter-specific.
- Preserve existing lockout, exact scene identity validation, generation guards, disconnect behavior, and blocked/skipped recall behavior.

Out of scope:

- Separate pan-specific scoped channel lists.
- Separate per-channel or per-parameter scope controls.
- A separate pan or width fade engine.
- Companion, HTTP, WebSocket, or external control changes.

## Protocol

Existing behavior:

- Fader reads use `/Notify/Track/Out/Gain`.
- Fader writes use `/Set/Track/Out/Gain`.
- Mute support exists separately and is not part of fade behavior.

New behavior:

Use only the explicit track pan addresses:

| Parameter | Read Notification | Write Command | Value Range |
|---|---|---|---:|
| Pan | `/Notify/Track/Pan` | `/Set/Track/Pan` | observed `-45.0..=45.0` degrees |
| Balance / rotation | `/Notify/Balance` | `/Set/Track/Pan/Balance` | observed `-45.0..=45.0` degrees |
| Width | `/Notify/PanArcWidth` | `/Set/Track/Pan/Width` | observed `-1.4..=1.4` direct linear values |

The app should label the scene toggle as `PAN`. The toggle controls the pan family, not a single parameter. Mono channels require pan only. Stereo channels require pan, balance/rotation, and width.

Hardware capture `logs/pan-balance-test/lv1-probe-1781060903.jsonl` confirmed `/Notify/Track/Pan` for a manual hard-left, hard-right, center sequence on channel `group=0, channel=1`. The observed values were approximately `-45`, `45`, and `0`.

Hardware captures `logs/width-test/lv1-probe-1781062730.jsonl` and `logs/width-test/lv1-probe-1781062883.jsonl` confirmed stereo balance/rotation movement emits `/Notify/Balance`. Active write testing in `logs/pan-write-test/lv1-pan-probe-1781063385.jsonl` confirmed `/Set/Track/Pan/Balance` controls that value. The earlier balance candidates `/Set/Balance` and `/Set/Track/Balance` are not the correct commands for this feature.

Hardware capture `logs/width-test/lv1-probe-1781061389.jsonl` confirmed manual stereo width movement emits `/Notify/PanArcWidth`, not `/Notify/Track/Pan/Width`, on channel `group=0, channel=2`. Active write testing confirmed `/Set/Track/Pan/Width` controls the observed `/Notify/PanArcWidth` value.

Bounds captures in `logs/bounds-test/` confirmed LV1 clamps pan and balance to `-45..45`. Sending pan `60` echoed `45`; sending balance `60` echoed `45`. Sending width `2` echoed `1.4`, and sending width `-2` echoed `-1.4`, so stereo width should be treated as at least `-1.4..=1.4`. Stereo default width is `1.0`. Active width `0.0` is meaningful and represents collapsed mono width, not “center.” Do not infer width availability from the numeric width value alone; use channel mode plus the notification's active/valid flag where available.

## Data Model

Existing behavior:

- `ChannelConfig` stores `group`, `channel`, and optional `faderDb`.
- `SceneScopeToggles` stores `faders`.
- `SceneConfig` stores one `scopedChannels` list.
- Show files persist scene configs and deserialize existing fader-only data.

New behavior:

Extend stored channel configuration with optional pan-family targets:

```ts
type ChannelConfig = {
  group: number;
  channel: number;
  faderDb: number | null;
  pan: number | null; // degrees, observed -45..45
  balance: number | null; // stereo balance/rotation degrees, null for mono channels
  width: number | null; // stereo width value, null for mono channels
  panMode: "mono" | "stereo" | null;
};
```

Extend scene scope toggles:

```ts
type SceneScopeToggles = {
  faders: boolean;
  pan: boolean;
};
```

Existing show files without `pan`, `balance`, `width`, or `scopeToggles.pan` should deserialize safely using defaults. `scopeToggles.pan` defaults to `false` for existing and newly stored scenes. This keeps current fader-only workflows unchanged until the engineer opts into `PAN`.

Store captures current live `faderDb`, `pan`, `balance`, `width`, and `panMode` values for every known channel once the live state mirror can confirm those values. Recall always starts from current live values, not stored start values.

Mono channels expose pan only. Stereo channels expose pan, balance/rotation, and width. `panMode` should be derived from the LV1 channel metadata observed in `/Channels`; field `16` is currently the likely channel mode indicator, with observed `i:1` for mono channels and `i:2` for stereo channels.

## Scene Scope Semantics

Existing behavior:

- Each scene has one `scopedChannels` list.
- `FADERS` controls whether scoped faders move on validated scene recall.
- If fader scope is disabled in the current app, recall automation skips that scene without aborting existing fades because no other scene scopes exist yet.

New behavior:

Each scene has one `scopedChannels` list. The list means “channels app-managed by enabled scene scopes.”

Recall behavior:

- `FADERS` on, `PAN` off: fade scoped faders only.
- `FADERS` off, `PAN` on: fade scoped pan-family parameters only.
- Both on: fade scoped faders and available pan-family parameters over the same scene duration.
- Both off: skip the recall and do not abort existing fades.

Duration `0` means enabled targets move immediately to exact stored values.

## Recall Validation

Recall validation remains strict and visible.

Existing behavior to preserve:

- Lockout blocks recall automation.
- LV1 must be connected.
- Current scene snapshot must be available.
- Recalled scene identity must exactly match the fresh LV1 snapshot.
- Live channel topology must be available for enabled fader fades.
- Enabled fader scope validates scoped channel presence and stored fader targets.
- Blocked recalls, skipped recalls, and disabled scope recalls do not abort existing fades.

New behavior:

For enabled `FADERS`:

- Every scoped channel must exist in live topology.
- Every scoped channel must have a stored `faderDb`.
- Missing live topology or stored fader data blocks the recall.

For enabled `PAN`:

- Every scoped channel must exist in live topology.
- Every scoped channel must have a stored `panMode` and stored `pan`.
- Mono scoped channels require stored `pan` only.
- Stereo scoped channels require stored `pan`, stored `balance`, and stored `width`.
- Missing live topology, `panMode`, required `pan`, required `balance`, or required `width` blocks the recall.

Disabled scope toggles do not require data for their parameters. If both toggles are disabled, recall is skipped rather than blocked.

Blocked, skipped, or disabled recalls must not abort existing fades.

## Fade Engine

Existing behavior to preserve:

- `FadeEngine` owns fade timing and LV1 fader writes.
- One scheduler drives active fades.
- Fades start from current live values.
- Scene-owned overlap allows unrelated faders from different scenes to fade at the same time.
- A new validated scene recall takes over overlapping active targets.
- Same-scene repeat behavior uses the validated fade-start path.
- Abort stops active fades.
- Duration `0` sends exact final values immediately for enabled targets.
- Completed targets receive exact final sends when still owned.
- Disconnect and generation safety prevent stale sends.

New behavior:

Extend the existing `FadeEngine` into a parameter-aware fade engine. Do not introduce a separate `PanEngine` or parallel scheduler. Fader, pan, balance, and width fades share the same timing loop, scene ownership rules, overlap handling, abort behavior, duration `0` handling, final exact-send behavior, event flow, and generation safety.

This should be implemented in two steps:

1. Refactor the current fader-only engine to use parameter-aware target identity while still supporting only faders. This step should preserve all existing behavior and tests.
2. Add pan and width parameters using the new target model.

The engine should keep common fade mechanics in one path and isolate only the parameter-specific pieces:

- How to interpolate values.
- How to decide minimum send delta.
- How to detect manual override.
- Which LV1 set command to send.

Represent fade targets by parameter:

```rust
enum FadeParameter {
    FaderDb,
    Pan,
    Balance,
    Width,
}
```

Active fades are keyed by `(group, channel, parameter)`. Scene-owned overlap applies at that key level. A new validated scene recall takes over only overlapping parameter targets, except for the pan-family override rule described below.

Examples:

- A new pan fade for channel 1 cancels or replaces only channel 1 pan.
- It does not cancel channel 1 fader, balance, or width unless those exact targets are part of the new fade.
- Same-scene repeat behavior continues through the same validated fade-start path, now per parameter target.

Interpolation:

- Faders keep the existing fader curve and fader-law behavior.
- Pan uses direct linear interpolation in observed degree values, currently `-45.0..=45.0`.
- Balance uses direct linear interpolation in observed degree values, currently `-45.0..=45.0`.
- Width uses direct linear interpolation in observed stereo width values, currently `-1.4..=1.4` with default stereo width `1.0`.

The main behavioral difference is fader law. Pan, balance, and width do not need fader-law conversion; they use direct linear values for interpolation, send-delta checks, and override checks.

Final sends:

- Fader sends use `/Set/Track/Out/Gain`.
- Pan sends use `/Set/Track/Pan`.
- Balance sends use `/Set/Track/Pan/Balance`.
- Width sends use `/Set/Track/Pan/Width`.
- Each active target gets an exact final send when it completes and is still owned.

## Manual Override Detection

Manual override detection is parameter-specific.

Existing behavior to preserve:

- Fader manual override detection uses fader position law.
- A fader override cancels only that active fader target.
- Fader override events remain visible through fade events and logs.

New behavior:

- Fader override uses the existing fader position law and `/Notify/Track/Out/Gain`.
- Pan override uses direct linear difference between reported `/Notify/Track/Pan` and expected pan.
- Balance override uses direct linear difference between reported `/Notify/Balance` and expected balance.
- Width override uses direct linear difference between reported `/Notify/PanArcWidth` and expected width.

Override cancellation is grouped by control family:

- Fader override cancels only the matching `(group, channel, FaderDb)` target.
- Pan override cancels `(group, channel, Pan)`, `(group, channel, Balance)`, and `(group, channel, Width)` if any are active.
- Balance override cancels `(group, channel, Pan)`, `(group, channel, Balance)`, and `(group, channel, Width)` if any are active.
- Width override cancels `(group, channel, Pan)`, `(group, channel, Balance)`, and `(group, channel, Width)` if any are active.

Pan, balance, and width cancel together because they share one `PAN` scene scope and all affect the spatial position of a sound. Overriding any one should give the engineer full manual control of the channel's pan-family position. Pan-family overrides do not stop the same channel's fader fade.

Suggested initial thresholds:

| Parameter | Override Threshold |
|---|---:|
| Fader | Existing position-space threshold |
| Pan | `0.02` |
| Balance | `0.02` |
| Width | `0.02` |

Pan, balance, and width thresholds are linear value thresholds, not fader-law thresholds.

## LV1 State Mirror

Existing behavior:

- `Lv1Actor` owns the TCP connection and mirrored LV1 state.
- `ChannelInfo` stores group, channel, name, gain, and mute state.
- `/Channels` populates channel topology and fader values.
- `/Notify/Track/Out/Gain` updates live fader values and emits fader events.
- `/Notify/Track/Out/Mute` updates live mute state and emits mute events.

New behavior:

Extend live channel state with optional pan-family values:

```rust
pub struct ChannelInfo {
    pub group: i32,
    pub channel: i32,
    pub name: String,
    pub gain_db: f64,
    pub muted: bool,
    pub pan: Option<f64>,
    pub balance: Option<f64>,
    pub width: Option<f64>,
    pub pan_mode: Option<PanMode>,
}
```

The actor should parse:

- `/Notify/Track/Pan` into `pan` and fan out a parameter-specific event.
- `/Notify/Balance` into `balance` and fan out a parameter-specific event.
- `/Notify/PanArcWidth` into `width` and fan out a parameter-specific event.

The `/Channels` batch has 19 fields per channel. Hardware captures now identify these fields:

- Field `3`: fader dB.
- Field `4`: stereo balance/rotation value for stereo channels.
- Field `16`: likely pan mode or channel mode, observed `1` for mono and `2` for stereo.
- Field `18`: pan degrees.

For mono channels, field `4` may contain a sentinel or non-balance value and should not be treated as balance. For stereo channels, field `4` should be stored as the initial balance/rotation value if the mode mapping is confirmed by tests. Width initial state comes from `/Notify/PanArcWidth`; it is not currently observed in `/Channels`.

## UI And Workflow

Existing behavior:

- The Scene tab has a scene list, selected scene editor, `Store` button, duration control, `FADERS` toggle, and one scoped channel grid.
- `Store` captures current LV1 channel state for the selected scene config.
- The scoped channel grid controls one shared list of app-managed channels.

New behavior:

The Scene tab keeps the existing structure:

- Scene list.
- Selected scene editor.
- Store button.
- One scoped channel grid.

Scene scope controls show two toggles:

- `FADERS`
- `PAN`

The scene row summary shows both toggles:

```text
4.0s · FADERS on · PAN off · 12/64 scoped
```

The scoped channel grid still controls the single shared scoped channel list. Tooltip or secondary display can include stored fader, pan, balance, and width values when available.

Store captures all available values from current LV1 state. There is no separate “store pan” action.

## Testing

Existing coverage to preserve or extend:

- Fader storage and scene config serialization tests.
- Scene recall policy tests for lockout, scene identity, missing config, fader scope disablement, missing topology, stored fader targets, and duration `0`.
- Fade engine tests for timing, fader-law interpolation, overlap, abort, final sends, and fader manual override.
- LV1 actor tests for fader and mute parsing/sending.
- Frontend typecheck/build coverage for the current Scene tab.

New coverage:

Core tests:

- Show storage serializes and deserializes `scopeToggles.pan`, `pan`, `balance`, and `width`.
- Existing show files default `scopeToggles.pan` to `false` and missing `pan`/`balance`/`width` to `None`.
- Store captures fader, pan, balance, and width from live channel snapshots.
- Recall policy builds only fader targets when `FADERS` is on.
- Recall policy builds only available pan-family targets when `PAN` is on.
- Recall policy blocks when `PAN` is on and a scoped mono channel lacks `pan`.
- Recall policy blocks when `PAN` is on and a scoped stereo channel lacks `pan`, `balance`, or `width`.
- Recall policy skips without aborting when both toggles are off.
- Fade overlap is parameter-specific.
- Manual fader override cancels only the matching fader target; manual pan-family override cancels pan, balance, and width for that channel.
- Duration `0` sends exact final values for all enabled `FadeParameter` targets.
- LV1 actor parses `/Notify/Track/Pan`, `/Notify/Balance`, and `/Notify/PanArcWidth`.
- LV1 actor sends `/Set/Track/Pan`, `/Set/Track/Pan/Balance`, and `/Set/Track/Pan/Width`.

Frontend checks:

- TypeScript view model includes `scopeToggles.pan`, `pan`, `balance`, and `width`.
- Scene tab displays the `PAN` toggle and scene summaries correctly.
- Existing fader UI behavior remains intact.

Verification commands:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
npm run typecheck
npm run build
```

## Protocol Notes To Preserve

Before implementation, add a short protocol note or test fixture documenting the observed `/Channels` field mapping from `logs/pan-balance-test/lv1-probe-1781060903.jsonl`, `logs/width-test/lv1-probe-1781061389.jsonl`, and related width/balance captures.

Active write testing confirmed:

- `/Set/Track/Pan` controls `/Notify/Track/Pan` using degree values.
- `/Set/Track/Pan/Balance` controls `/Notify/Balance` using degree values.
- `/Set/Track/Pan/Width` controls the observed `/Notify/PanArcWidth` value.
