# Phase 7 Scene Recall Fader Design

## Goal

Phase 7 makes app-managed scenes with nonzero duration respond automatically to LV1 scene recalls. When LV1 reports a scene change, the app validates the recalled scene against stored scene fade config and starts a fader fade for the scoped stored targets when it is safe to do so.

## Current Architecture Context

The backend is actor-oriented:

- `Lv1Actor` owns the LV1 connection and state mirror, then publishes LV1 events through `AppEventBus`.
- `FadeEngine` owns active fade timing, sends gain commands through `AppCommandBus`, and publishes fade events.
- `ShellState` owns the UI projection, show-file state, scene configs, lockout state, and logs.
- The Tauri runtime installs generation-scoped LV1, fade, and shell projector tasks during `connect`.

Phase 7 should preserve these boundaries. Scene recall automation should use `AppEventBus` and `AppCommandBus`, not concrete LV1 or fade actor internals.

## Chosen Approach

Add a dedicated backend task named `SceneRecallFader`.

`SceneRecallFader` subscribes to `AppEventBus`, listens for `Lv1Event::SceneChanged`, asks `ShellState` to validate and build a fade request for the recalled scene, and uses `AppCommandBus` to abort/start fades.

This approach keeps responsibilities clear:

- `Lv1Actor` detects scene changes.
- `ShellState` validates app/show state and records visible logs.
- `FadeEngine` executes fades and remains responsible for manual override and disconnect cancellation.
- `SceneRecallFader` bridges LV1 scene recall events to fade commands.

The module/task should be named around this specific behavior, for example `scene_recall_fader` with `spawn_scene_recall_fader(...)`.

## Runtime Lifecycle

`SceneRecallFader` is installed during `connect` alongside the current LV1 actor, fade engine, command bus, and shell-state projector.

The task is generation-scoped. It receives the active generation at spawn time and asks `ShellState` for validation only if that generation is still current. Stale tasks from reconnects must not start fades.

Disconnect/reconnect cleanup should abort the `SceneRecallFader` task with the rest of the runtime handles. Existing generation guards remain the primary protection against stale events.

## Recall Flow

When `SceneRecallFader` receives `Lv1Event::SceneChanged(scene)`:

1. Ask `ShellState` to validate the recalled scene for the active generation.
2. If validation returns a skip or block result, do not send fade commands.
3. If validation returns a fade request, call `abort_all_fades()` first to enforce the overlap policy.
4. Start the new fade with `start_fade(FadeConfig)`.
5. Record visible logs for successful starts and safety blocks.

## Validation Rules

Validation should be centralized in `ShellState` or a helper owned by the app-state layer because it depends on app/show state.

Validation order:

1. Current runtime generation still matches.
2. LV1 is connected and has a current scene snapshot.
3. Recalled scene matches exactly by `index + name`.
4. A scene config exists for the exact scene id.
5. `duration_ms > 0`; duration `0` means the scene is disabled for automatic fade recall.
6. Lockout is off.
7. A live channel snapshot exists and is not empty.
8. Every scoped channel resolves to a stored channel config with `Some(fader_db)`.
9. Every scoped target still exists in live channel topology.
10. At least one valid target remains.

The stored scene id continues to use the existing exact-match form: `{index}::{name}`.

## Safety Behavior

Automatic fade recall must be blocked when any validation rule fails.

Important safety blocks:

- LV1 connection is unavailable or not connected.
- Scene identity is mismatched or ambiguous.
- Lockout is enabled.
- No live fader/channel snapshot is available.
- A scoped channel cannot be mapped to a stored target value.
- A scoped channel no longer exists in live topology.
- The resulting target list is empty.

Scenes with `duration_ms == 0` are disabled for automatic fade recall. This is a normal skip, not an error.

The app-created store workflow currently writes stored fader values as `Some(channel.gain_db)` and only allows scoping channels that exist in `channel_configs`. However, `ChannelConfig.fader_db` and show-file data currently allow `None`, so `SceneRecallFader` should keep defensive validation for malformed, hand-edited, or legacy show files. A separate follow-up is tracked in `IDEAS.md` to tighten Rust data models so invalid states are harder to represent.

## Overlap Policy

MVP overlap policy: recalling a new valid app-managed scene while a fade is running cancels the previous fade and starts the new one only after validation.

Operationally:

1. Validate the new recall first.
2. If valid, call `abort_all_fades()`.
3. Then call `start_fade(...)` for the new scene.

Invalid, disabled, or blocked recalls must not abort an existing fade. This avoids a duration `0` or unsafe recall unintentionally stopping an active fade.

## Fade Request Construction

On successful validation, build `FadeConfig` from the matched scene config:

- `duration_ms` comes from the scene config.
- `targets` contains one `FadeTarget` per scoped channel.
- Each `FadeTarget.target_db` comes from the stored channel config `fader_db`.
- The curve uses the existing MVP default fade curve until per-scene curve storage is added.

Fade starts from current live fader values because `FadeEngine` already reads the current LV1 state when `start_fade` is received.

## Logs And UI

Phase 7 does not require a new major UI surface.

The current header already exposes current scene, fade state, lockout, abort, and finish controls. The Scene tab already exposes duration and scope editing. The Logs tab should make automation behavior clear.

Useful logs:

- Scene recall detected.
- Auto fade started for scene `index: name`.
- Auto fade skipped because duration is `0`.
- Auto fade blocked by lockout.
- Auto fade blocked because LV1 is disconnected or state is unavailable.
- Auto fade blocked because a scoped target is missing stored fader data.
- Auto fade blocked because a scoped target is missing from live topology.
- Previous fade aborted before starting a new scene recall fade.

Logs should use existing app/fade/LV1 log sources and severities. Safety blocks should be visible warnings. Duration `0` skips can be informational. To avoid noisy logs, repeated duration `0` skips for the same scene should not be logged more than once per active connection generation.

## Events

No new public UI event type is required for Phase 7. The existing `app-status-changed` snapshots and logs are enough for MVP UI status.

If useful internally, `AutomationEvent` can be expanded later, but Phase 7 should not add API-facing event commitments that Phase 8 may need to redesign.

## Testing Strategy

Most coverage should be Rust tests. Prefer testing a validation helper plus task behavior with fake command targets or direct command receiver assertions, avoiding live LV1.

Core cases:

- `SceneChanged` for a configured scene with nonzero duration starts a fade with scoped stored targets.
- Duration `0` scene does not start a fade.
- Lockout blocks auto fade.
- Missing scene config does not start a fade.
- Exact scene identity is required: `index + name` must match.
- Missing live channel snapshot blocks auto fade.
- Every scoped channel must resolve to stored channel config with `Some(fader_db)`.
- Scoped target missing from live topology blocks auto fade.
- Valid recall while a fade is running aborts first, then starts the new fade.
- Invalid, disabled, or blocked recall while a fade is running does not abort the existing fade.
- Generation mismatch prevents stale tasks from starting fades.

## Non-Goals

- No per-scene curve editing or storage.
- No HTTP/WebSocket external recall API; that belongs to Phase 8.
- No Bitfocus Companion support; that belongs to Phase 9.
- No durable scene rename/reorder remapping.
- No broad UI redesign.
- No Rust data-model tightening beyond defensive validation in this phase.

## Exit Criteria

- Normal LV1 scene recall triggers the correct scoped fade for matching stored scene configs with nonzero duration.
- Scenes with duration `0` do not trigger fades.
- Safety blocks prevent unsafe sending and are visible in logs.
- A valid new recall cancels the previous fade and starts the new fade after validation.
- Fade status remains clear through existing UI state and logs.
