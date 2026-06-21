# Isolated Tauri Debug Smoke Test Design

## Purpose

Add a development-only Tauri debug app for running full-workflow smoke tests against actual LV1 hardware. The smoke tests validate the same app path an engineer uses in the production UI: Tauri command adapters, actor runtime, event bus, operational logging, projector output, and LV1 hardware feedback.

The debug app is separate from the production app UI so hardware-test controls, dangerous confirmations, and internal diagnostics do not clutter or expose production workflows.

## Goals

- Launch a separate Tauri debug app for hardware smoke testing.
- Reuse the production backend runtime, actors, command adapters, and projector.
- Start individual smoke tests from debug-only Tauri commands.
- Have each backend smoke-test command install event/log observers before triggering the tested action through normal command/actor paths.
- Capture DEBUG-level `tracing` events for backend smoke assertions.
- Have the frontend independently validate `app-status-changed` projector output for the same test run.
- Pass a test only when both backend event/log expectations and frontend projector expectations pass.
- Require two dedicated LV1 test scenes and explicit test-channel inputs.
- Verify actual fader movement on real hardware.
- Leave the console in the final smoke-test state for manual inspection.

## Non-Goals

- Do not add hidden smoke-test controls to the production React app.
- Do not add a new production control API.
- Do not let debug commands directly recall scenes, directly mutate show state, directly write faders, bypass lockout, bypass scene identity checks, bypass generation guards, or emit projector state. When a smoke test needs an app action, it must route that action through the same production command or actor path used by the app.
- Do not restore faders automatically at the end of the smoke test.
- Do not package the debug app for release in the first implementation.

## Architecture

The production app and debug app share backend initialization but register different command sets.

```text
production React app
  -> production Tauri commands
    -> shared runtime setup
      -> AppLifecycle / ShowState / Lv1Actor / FadeEngine / ScenesActor
        -> AppEventBus
          -> projector
            -> app-status-changed

debug React app
  -> debug-only Tauri smoke-test commands for test runs
  -> production Tauri commands for setup and manual actions
    -> shared runtime setup
      -> AppLifecycle / ShowState / Lv1Actor / FadeEngine / ScenesActor
        -> AppEventBus
          -> projector
            -> app-status-changed
```

Rust additions:

- Extract shared app runtime setup from `src-tauri/src/ui/mod.rs` so production and debug builders cannot drift.
- Keep `advanced_show_control::ui::build_app()` as the production app builder.
- Add `advanced_show_control::ui::debug::build_debug_app()` as the debug app builder.
- Add `src-tauri/src/bin/advanced-show-control-debug.rs` as the debug app binary.
- Add `src-tauri/tauri.debug.conf.json` with debug product name, identifier, window title, dev URL, and frontend dist.
- Add `src-tauri/src/ui/commands/debug_smoke.rs` for debug-only smoke-test runner commands.
- Add a debug smoke tracing capture layer that records DEBUG-and-above `tracing` events for the active test run.

Frontend additions:

- Add a debug React entrypoint separate from `ui/src/main.tsx`.
- Add a debug smoke-test UI that collects hardware-test inputs, displays live projector state, starts individual backend smoke tests, independently tracks projector expectations, and renders combined pass/fail details.
- Reuse existing Tauri command names for setup and manual actions initiated by the debug frontend.
- Add debug command wrappers for starting individual smoke tests and receiving backend test results.

## Command Boundaries

General setup and manual actions initiated by the debug frontend should use existing production commands:

- `frontend_ready`
- `refresh_lv1_discovery`
- `connect_lv1_system`
- `disconnect_lv1`
- `new_show_file`
- `store_scene_config`
- `set_channel_scoped`
- `set_all_channels_scoped`
- `set_scene_scope_faders_enabled`
- `set_scene_scope_pan_enabled`
- `set_scene_duration_ms`
- `recall_scene`
- `abort_all_fades`
- `set_lockout`

Debug smoke-test commands may orchestrate a single named test, but they must still use the same lifecycle and actor command paths that production Tauri command adapters use for behavior under test. A backend smoke-test command does not need to invoke another Tauri command by name; it must avoid duplicating business logic or bypassing the actor/policy path. Debug smoke-test commands are allowed to subscribe to runtime facts and DEBUG tracing before starting the test so they can verify the backend sequence that occurred.

Candidate commands:

- `debug_smoke_run_connection_test`
- `debug_smoke_run_scene_recall_test`
- `debug_smoke_run_fade_starts_test`
- `debug_smoke_run_fade_completes_test`
- `debug_smoke_run_decreasing_xfade_test`
- `debug_smoke_run_lockout_blocks_recall_test`

Each debug command returns structured backend results suitable for a checklist UI:

```ts
type SmokeStepResult = {
  ok: boolean;
  step: string;
  message: string;
  observed?: unknown;
};

type SmokeBackendResult = {
  ok: boolean;
  testId: string;
  startedAt: string;
  finishedAt: string;
  steps: SmokeStepResult[];
  observedEvents: unknown[];
  observedTraces: unknown[];
};
```

The debug frontend combines backend results with its own projector-observation result:

```ts
type SmokeCombinedResult = {
  ok: boolean;
  backend: SmokeBackendResult;
  projector: SmokeStepResult[];
};
```

The final test status is pass only when `backend.ok` is true and every projector step is true.

## Backend Observer Model

Each backend smoke-test command follows this sequence:

1. Validate required test parameters.
2. Subscribe to `AppEventBus` before starting the action under test.
3. Start or attach a DEBUG-level `tracing` capture window before starting the action under test.
4. Record a test start timestamp and active runtime generation.
5. Trigger the action under test through the normal app path. If the action already has a production Tauri command adapter, the smoke runner should call the same underlying lifecycle/actor command path rather than duplicating behavior.
6. Wait for expected events and DEBUG trace events with explicit timeouts.
7. Return observed events, observed trace events, pass/fail steps, and diagnostics.

Backend smoke-test commands must not emit `app-status-changed`. The projector remains the only frontend state projection source.

The backend should verify facts from these sources:

- `AppEvent::Runtime` for generation changes.
- `AppEvent::Lv1` for connection, scene, channel, and fader facts.
- `AppEvent::Fade` for fade start, progress-relevant completion/idle facts, aborts, and manual override events.
- `AppEvent::Show` for show metadata, lockout, and scene-config projection changes.
- DEBUG-level `tracing` events for detailed workflow, validation, fader, manual-override, and safety assertions.

The frontend should verify facts from these sources:

- `app-status-changed` snapshots only.
- Projected logs included in `AppViewState.logs`.
- Projected connection, current scene, fade state, lockout, scene config, and state version changes.

UI-facing logs are not sufficient for backend smoke-test assertions. They remain useful frontend projector output, but backend pass/fail checks must use DEBUG-level tracing capture plus runtime events.

## DEBUG Trace Capture

The debug app must provide an in-process smoke-test trace capture path. It should capture structured `tracing` events at DEBUG level and above for the duration of a specific test run.

The capture layer should record at least:

- Timestamp.
- Level.
- Target/module path.
- Structured `event` field when present.
- Message field when present.
- Other structured fields as serializable string/debug values.
- Runtime generation when the event includes one.

The capture should be bounded so a stalled or noisy test cannot grow memory without limit. A ring buffer scoped to the active test run is sufficient for the initial implementation.

Smoke tests should assert against stable structured fields such as `event = "scene_recall_ready"`, not against free-form human messages. If a needed assertion currently only has a human message or no trace event, implementation should add a targeted DEBUG trace event at the production code point being verified.

The production app should not expose the smoke trace capture commands. The debug app may install the capture layer as part of debug logging setup.

## Smoke-Test Workflow

The debug UI requires these inputs before enabling the run button:

- LV1 target identity, from discovery or manual host/port entry.
- Two dedicated LV1 test scene IDs: `scene_a_id` and `scene_b_id`.
- One scoped test channel for the initial smoke test: `group` and `channel`.
- Fade duration in milliseconds.
- Fader movement tolerance in dB, defaulting to `0.5 dB`.
- Maximum connect wait, scene wait, fade-start wait, fade-finish wait, and fader-movement wait timeouts.
- Explicit danger acknowledgement that the test recalls scenes and moves hardware faders.
- Explicit acknowledgement that the console will be left in the final smoke-test state.

The first implementation should capture target fader values from the two dedicated LV1 scenes instead of requiring the user to type dB targets. This keeps the test aligned with the actual LV1 show state and avoids asking the operator to duplicate console values manually.

Required LV1 scene preparation:

- `scene_a_id` and `scene_b_id` must exist in LV1 before the test starts.
- The selected test channel must have intentionally different fader values in the two scenes.
- The fader delta between the two scenes must be at least the configured movement threshold, defaulting to `3.0 dB`.
- The selected test channel should be safe to move and should be scoped out of LV1 scene recall if the operator wants to observe only the app-managed fade motion.

Captured runtime parameters:

- `scene_a_target_db`: captured from LV1 actor state after recalling or observing scene A.
- `scene_b_target_db`: captured from LV1 actor state after recalling or observing scene B.
- `start_db`: sampled immediately before the app-managed recall that should start the fade.
- `observed_min_db`, `observed_max_db`, and `final_db`: sampled by the backend assertion loop from LV1 actor `GetState` snapshots.

Shared setup workflow:

1. Start projector through `frontend_ready` and subscribe to `app-status-changed`.
2. Connect to LV1 through `connect_lv1_system`.
3. Assert connection through projector state and `debug_smoke_run_connection_test`.
4. Create a fresh app show overlay through `new_show_file`.
5. Recall scene A through `recall_scene` or require the operator to put LV1 on scene A before capture.
6. Wait for projector `currentScene` to match scene A and assert the LV1 actor snapshot also reports scene A.
7. Capture `scene_a_target_db` for the test channel through a debug read-only LV1 snapshot command.
8. Store scene config for scene A through `store_scene_config`.
9. Scope only the selected test channel through `set_all_channels_scoped(scene_a_id, false)` and `set_channel_scoped(scene_a_id, group, channel, true)`.
10. Enable fader scope and disable pan scope for scene A.
11. Set scene A fade duration.
12. Repeat capture and app-scene setup for scene B, producing `scene_b_target_db`.
13. Verify the captured scene targets differ by at least the movement threshold.
14. After setup, individual smoke tests can be run independently from the debug UI.

If the workflow fails while a fade may be active, the debug UI should make the failure prominent and run or offer `abort_all_fades` through the normal command path. It should not restore fader values.

## Test Suite

### Connection Test

Purpose: verify the app can establish a real LV1 connection and project connected state.

Backend command: `debug_smoke_run_connection_test`.

Inputs:

- LV1 identity.
- Connection timeout.

Backend expectations:

- Runtime generation advances for connection attempt.
- LV1 connection event reaches connected state for the active generation.
- Show metadata records the connected identity.
- DEBUG traces include connection request, connecting, and connected events.
- No connection failure or stale-generation safety log appears for the test run.

Frontend projector expectations:

- `connection` becomes `connecting` or pending identity appears, then `connected`.
- `connectedLv1Identity` matches the selected identity.
- Scene and channel counts become non-zero when LV1 provides them.

### Scene Recall Test

Purpose: verify the app can recall a dedicated LV1 scene through the normal recall path.

Backend command: `debug_smoke_run_scene_recall_test`.

Inputs:

- Target `scene_id`.
- Recall timeout.

Backend expectations:

- Recall request is accepted through scene recall policy.
- LV1 scene recall command is sent through the LV1 actor.
- LV1 scene change event exactly matches target scene index and name.
- DEBUG traces include recall request, recall validation success, and LV1 recall dispatch events.
- No lockout, stale-state, scene-identity mismatch, or LV1-unavailable block appears.

Frontend projector expectations:

- `currentScene` changes to the target scene.
- `stateVersion` advances after recall.
- No projected error log appears for the recall.

### Fade Starts Test

Purpose: verify recalling an app-managed scene starts a fade.

Backend command: `debug_smoke_run_fade_starts_test`.

Inputs:

- Source `scene_id`.
- Target `scene_id`.
- Test channel.
- Fade-start timeout.

Backend expectations:

- Scene recall validation passes.
- Fade engine receives a validated scene fade request.
- Fade start event is published for the active generation.
- LV1 fader events begin moving the scoped channel in the expected direction.
- DEBUG traces include fade start or equivalent fade-engine workflow event.

Frontend projector expectations:

- `currentScene` reaches the target scene.
- `fadeState` becomes `running`.

### Fade Completes And Moves Fader Test

Purpose: verify the app-managed fade reaches the expected target value on LV1 hardware.

Backend command: `debug_smoke_run_fade_completes_test`.

Inputs:

- Source `scene_id`.
- Target `scene_id`.
- Test channel.
- Expected target dB captured during setup.
- Movement tolerance dB.
- Minimum movement dB.
- Fade-finish timeout.
- Sampling interval.

Backend expectations:

- Fade start event occurs.
- LV1 fader events are observed for the scoped channel.
- Observed movement reaches at least the minimum movement threshold or reaches target tolerance.
- Final sampled LV1 channel value is within tolerance of expected target.
- Fade completion/idle event occurs without manual override or abort.
- DEBUG traces do not include manual override or safety block events for the test channel.

Frontend projector expectations:

- `fadeState` becomes `running` then `idle`.
- Projected logs include expected fade start/completion messages when those logs are available.
- No projected manual override or safety-block error appears.

### Decreasing X-Fade False-Override Test

Purpose: verify repeated fades with decreasing durations do not produce false manual override detection.

Backend command: `debug_smoke_run_decreasing_xfade_test`.

Inputs:

- Scene A ID.
- Scene B ID.
- Test channel.
- Durations: `5000 ms`, `3000 ms`, `1000 ms`, `500 ms`.
- Movement tolerance dB.
- Per-fade timeout multiplier or explicit timeout.

Backend expectations for each duration:

- Scene config duration is set through the normal show command path before recall.
- Recall starts a fade for the active generation.
- LV1 fader events move toward the expected target.
- Fade reaches idle/completion within timeout.
- No manual override event is published.
- No DEBUG trace event reports manual override for the app-owned fade.

Frontend projector expectations for each duration:

- Projected scene changes to the recalled target.
- `fadeState` becomes `running` then `idle`.
- Projected logs do not show manual override or safety-block messages.

The test alternates A -> B -> A -> B across the duration list so each fade has a real target change.

### Lockout Blocks Recall Test

Purpose: verify lockout prevents recall automation and no fade or fader movement occurs.

Backend command: `debug_smoke_run_lockout_blocks_recall_test`.

Inputs:

- Current/source `scene_id`.
- Blocked target `scene_id`.
- Test channel.
- Observation timeout.

Backend expectations:

- Lockout is enabled through normal show command path before recall.
- Recall attempt is blocked by scene recall policy.
- No LV1 scene recall command is sent for the blocked target.
- No fade start event occurs.
- No scoped-channel fader movement beyond tolerance is observed.
- DEBUG traces include a lockout/safety block event.

Frontend projector expectations:

- `lockout` projects as `true` before recall attempt.
- `currentScene` remains the source scene.
- `fadeState` remains `idle`.
- Projected logs include the lockout block message.

## Internal State Checks

The debug backend can inspect internal state only for verification. It may read:

- Current lifecycle generation and connection metadata.
- Current LV1 actor snapshot through the existing `Lv1Command::GetState` command.
- Show actor state through existing or newly added read-only commands.
- Projected fade state through the frontend's `app-status-changed` subscription.
- Recent projector snapshots if a cache/listener is introduced for debug observations.

Any new actor commands for this work must be read-only and explicit. They must not hide production behavior behind convenience methods or mutate actor state.

## Backend Assertion Semantics

The backend smoke-test commands should return structured pass/fail results and enough observed data to explain failures. They should use monotonic timeouts and sample intervals that are explicit in the request or defaults.

Connection assertion:

- Input: expected identity and timeout.
- Passes when lifecycle/show metadata indicates the expected connected identity and the current LV1 actor snapshot reports `Connected`.
- Fails with the latest connection state and connected identity if the timeout expires.

Scene assertion:

- Input: expected `scene_id` and timeout.
- Passes when the LV1 actor snapshot current scene exactly matches the parsed scene index and name.
- The debug UI separately verifies the projector `currentScene` so the report distinguishes backend state failures from projection failures.
- Fails if LV1 reports a different scene, no scene, or no connected runtime before timeout.

Scene config assertion:

- Input: scene ID, expected scoped channel, expected duration, and expected scope toggles.
- Passes when the projected `sceneConfigs` and read-only show state agree that only the selected test channel is scoped for the scene, fader scope is enabled, pan scope is disabled, and duration matches.
- Fails with the observed config details when any field differs.

Fader movement assertion:

- Inputs: group, channel, expected start value, expected target value, tolerance dB, minimum movement dB, timeout, and sample interval.
- Samples LV1 actor `GetState` snapshots until timeout or success.
- Passes when all of the following are true:
  - The channel appears in LV1 state.
  - At least two distinct fader samples are observed.
  - The observed movement in the expected direction is at least `minimum_movement_db` or reaches the target tolerance sooner.
  - The final observed value is within `tolerance_db` of the expected target value.
- Fails with observed start, min, max, final, sample count, and timeout details.

Fade projection assertion:

- Input: expected transition and timeout.
- Runs in the debug UI from `app-status-changed` snapshots, not by reading the fade actor directly in the first implementation.
- Passes when the UI observes `fadeState: "running"` after recall and later observes `fadeState: "idle"`.
- Fails with the latest projected fade state and state version if the transition does not occur.

No-safety-error assertion:

- Input: optional trace scan window.
- Passes when DEBUG trace events since the start of the smoke test do not include safety blocks unrelated to the expected test flow.
- Fails with matching trace events.

## Launch Model

The first implementation uses a simple two-terminal launch:

```bash
npm --prefix ui run dev:debug
```

```bash
cd src-tauri
cargo run --bin advanced-show-control-debug
```

The debug Tauri config points to the debug frontend dev server. A single wrapper script can be added later after the debug app is stable.

The debug app is not part of production packaging for the initial implementation.

## Safety Requirements

- The debug app must require explicit hardware-movement acknowledgement before running the suite.
- The debug app must require dedicated test scene IDs before running the suite.
- Debug commands must not bypass lockout checks.
- Debug commands must not bypass exact scene identity validation.
- Debug commands must not bypass generation guards.
- Debug commands must not send fader commands.
- Debug commands must not emit `app-status-changed`.
- Projector output remains the only frontend state channel.
- Smoke-test failure reasons must be visible in the debug UI.
- Abort-on-failure behavior, when used, must call the normal `abort_all_fades` command path.

## Testing Strategy

Backend tests:

- Production builder does not register debug commands.
- Debug builder registers production commands plus debug commands.
- Shared setup initializes the same managed state for production and debug builders.
- Debug smoke commands route app actions through normal command/actor paths, observe events/DEBUG traces, and return structured failures on unavailable runtime state.
- Debug trace capture records DEBUG events only in the debug app and is bounded to an active test run.
- Any new actor read commands are covered by actor tests.

Frontend tests:

- Debug smoke UI keeps run disabled until required inputs and acknowledgements are present.
- Debug smoke UI invokes production command wrappers for setup and manual actions.
- Debug smoke UI invokes debug command wrappers to run named backend-observed tests.
- Failure reports include the failed step and observed details.

Manual hardware verification:

- Launch debug app.
- Connect to LV1 hardware.
- Run the smoke test with two dedicated scenes and a safe test channel.
- Confirm fader movement is observed and the final report passes.

## Open Implementation Details

- Decide whether debug frontend lives under a Vite multi-page entry or a separate Vite config.
- Decide whether later versions should expose selected live fader values in `AppViewState`; the first implementation should use backend LV1 snapshots for fader movement assertions because current projector output does not include fader values.
- Decide whether a later CLI/export report format is useful after the debug UI report exists.

These decisions do not change the approved architecture: app actions go through production command or actor paths, state reaches the UI through projector output, and debug commands observe/assert backend events and DEBUG traces without bypassing safety.
