# Scene-Owned Overlapping Fades Design

## Goal

Implement the two fade follow-up ideas from `IDEAS.md`:

- Allow multiple scene fade events to run at the same time when they control different faders.
- Guarantee that completed channels receive the stored target dB directly on the final send.

This design also changes the finish behavior: there is no global `Finish Now` command or UI control. Recalling the same validated app-managed scene while its fade is still active finishes only that scene's active faders.

## Current Context

The current Phase 7 behavior uses a single active fade model. `SceneRecallFader` validates an incoming scene recall, calls `abort_all_fades()`, then starts the new fade. `FadeEngine` also clears all active channels whenever `StartFade` is received.

That behavior was safe for MVP, but it prevents VENUE-style overlap. It also leaves the final-target guarantee implicit in tick interpolation and `next_send`, which is not enough for the current bug report: the final channel value must be the stored target dB, not a value produced by position/dB interpolation.

## Chosen Approach

Use scene-owned active channels.

`FadeEngine` should track active channels by fader key, with each channel carrying its owning scene identity. Only one active fade can own a fader at a time, but different faders can be owned by different scene recalls and run concurrently.

The scene identity is the existing exact scene match pair:

- `scene_index`
- `scene_name`

This keeps the overlap model aligned with the current safety boundary. A scene-scoped finish is allowed only after the same exact scene identity validates again.

## Fade Engine Model

Add scene ownership to fade commands and active channels.

Conceptually:

```rust
struct FadeSceneIdentity {
    index: i32,
    name: String,
}

struct FadeConfig {
    scene: FadeSceneIdentity,
    targets: Vec<FadeTarget>,
    duration_ms: u64,
    curve: FadeCurve,
}

struct ActiveChannel {
    group: i32,
    channel: i32,
    scene: FadeSceneIdentity,
    start_db: f64,
    target_db: f64,
    expected_db: f64,
    curve: FadeCurve,
    duration: Duration,
    started_at: Instant,
}
```

The active set is keyed by `(group, channel)`. Starting a fade for a new validated scene reads fresh LV1 state, then for each incoming target:

1. Remove any existing active channel with the same `(group, channel)`.
2. Use the current live LV1 value as `start_db`.
3. Insert a new `ActiveChannel` owned by the incoming scene.

Existing active channels for faders not included in the incoming scene continue unchanged.

## Scene Recall Flow

`SceneRecallFader` keeps the existing validation rules and generation guards. Blocked, skipped, stale, duration-zero, or invalid recalls must not mutate fade state.

After a recall validates, `SceneRecallFader` sends one atomic fade-engine command for the validated scene recall. The engine decides from its current active set whether the command means same-scene finish or new/overlapping fade start.

1. If the exact scene identity owns active channels, finish that scene's active channels.
2. Otherwise, start the incoming fade config.

The old `abort_all_fades()` before every valid start is removed. A different scene recall should not stop unrelated active faders.

## Same-Scene Repeat Recall

If a scene is recalled again while that same exact scene owns active channels, the repeat recall finishes only that scene's faders.

Scene-scoped finish behavior:

1. Find all active channels whose owner equals `scene`.
2. Send each channel's stored `target_db` directly with `SetGain`.
3. Remove only those channels from the active set.
4. Leave active channels owned by other scenes running.
5. Emit scene/channel completion events or refresh state so the UI no longer shows those channels as active.
6. Emit the existing global completion status only if the full active set is now empty.

This finish operation is still a fader-moving action, so it must only happen after full scene recall validation succeeds.

## Final Target Send

When a channel naturally reaches its duration, the engine must send the stored target dB directly. The final send must not depend on interpolation output, fader position conversion, or minimum-delta suppression.

Natural completion behavior for each channel:

1. Detect that the channel duration has elapsed.
2. Send `target_db` exactly once.
3. Remove the channel from the active set.

No retry loop or post-send verification is included in this change. If hardware testing shows one exact final send is not enough, retry/verification can be designed later with real evidence.

## Manual Override

Manual override remains channel-scoped.

If LV1 reports a fader value that exceeds the existing override threshold for an active channel, the engine cancels only that active channel. Ownership does not make override scene-wide. Other channels from the same scene and other scenes continue.

## Disconnect, Abort, And Removed Finish Control

`Abort All` remains global and clears every active channel without final sends. It is still the broad emergency stop.

Disconnect still aborts all active channels immediately and stops sending.

The global `Finish Now` command is removed from:

- `FadeEngineHandle`
- `FadeCommand`
- `AppCommandBus`
- Tauri commands
- UI controls
- docs that describe current behavior

External HTTP/WebSocket APIs are not implemented yet, so there is no external compatibility requirement.

## Events And UI State

The UI currently needs to know whether fading is active and whether fade activity completed, aborted, or had manual overrides. With overlapping fades, a single `FadeCompleted` event should mean the entire active set is empty.

Partial events should be added only as needed to keep logs/status honest. Useful low-cost additions are:

- `SceneFadeStarted { scene }`
- `SceneFadeFinished { scene }`
- `ChannelCompleted { group, channel }`

If existing UI state can remain accurate without all of these, prefer the smaller event set. Safety-relevant actions such as same-scene finish and manual override should be visible in logs.

## Documentation Updates

Update project docs that currently describe single-fade replacement or global finish behavior:

- `PHASES.md` Phase 7 overlap policy.
- `PROJECT.md` safety/control descriptions if they mention global finish as current behavior.
- `docs/architecture.md` command flow and automation boundary.
- `IDEAS.md`, removing or revising the implemented overlap and exact-final-target ideas.

Older design specs can remain historical, but new docs should make the current behavior clear.

## Testing Strategy

Use TDD for the implementation.

Fade engine tests:

- Starting a new scene fade does not clear unrelated active channels.
- Incoming scene targets replace only overlapping faders.
- Repeat-scene finish sends exact stored targets only for channels owned by that scene.
- Natural channel completion sends exact stored target dB directly.
- Manual override cancels only the affected active channel.
- Disconnect and abort still clear all active channels.
- The removed global finish command is no longer available in the command path.

Scene recall tests:

- Valid different-scene recall starts the incoming fade without first sending `AbortAll`.
- Valid same-scene repeat recall finishes that scene's active channels instead of restarting them.
- Blocked, skipped, stale, or duration-zero recalls do not start or finish anything.
- Generation changes still prevent stale start or finish commands.

UI/app-state tests:

- The global finish UI action and command are removed.
- Fade status still reflects active, completed, cancelled, overridden, and aborted states clearly enough for the current UI.

## Non-Goals

- No retry loop for final target sends.
- No post-send target verification.
- No broad UI redesign.
- No external HTTP/WebSocket control API.
- No change to exact scene identity validation.
- No change to manual override threshold or detection model.

## Exit Criteria

- Valid recalls for different scenes can overlap when their scoped faders differ.
- Incoming recalls take over only overlapping faders.
- Recalling the same validated scene while its fade is active finishes only that scene's active faders.
- Natural completion and same-scene finish both send stored target dB directly.
- Global finish is removed from backend command paths and the UI.
- Existing lockout, validation, generation, disconnect, manual override, and abort safety behavior is preserved.
