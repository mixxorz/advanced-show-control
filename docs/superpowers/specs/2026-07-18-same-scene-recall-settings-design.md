# Same-Scene Recall Settings Design

## Context

Advanced Show Control currently treats an accepted repeated recall of a timed scene specially. If that exact LV1 scene still owns active fade targets, the fade engine marks those targets for exact completion after the post-recall readiness barrier. Scene notifications that repeat the last accepted identity within 500 ms are suppressed before normal recall validation.

On hardware, a single scene recall may be producing a later duplicate scene notification that passes the fixed 500 ms suppression window. That would select same-scene finishing and make the fade appear to jump to its stored targets. The behavior is not consistently reproducible with the simulator, so the operator needs independent controls for the finishing behavior and the repeat suppression threshold.

## Goals

- Allow the operator to enable or disable same-scene finishing.
- Preserve current same-scene finishing behavior by default.
- When finishing is disabled, process an accepted repeated recall through the existing different-scene target replacement behavior.
- Allow the operator to configure the same-identity repeat suppression threshold from 0 through 5000 ms in 100 ms increments.
- Preserve the existing 500 ms threshold by default.
- Keep all recall validation, readiness, generation, lockout, disconnect, manual override, abort, and overlap safety behavior unchanged.
- Provide enough operational logging to distinguish finishing from current-value timeline replacement during hardware diagnosis.

## Non-Goals

- Do not change the 25 ms scene notification settle delay.
- Do not change the 2-second connection-generation arming window.
- Do not change the 500 ms scene-list-edit suppression window.
- Do not change the 2-second fresh LV1 scene snapshot timeout.
- Do not correlate scene notifications exclusively with app-issued recall commands. External LV1 recalls must continue to drive automation.
- Do not change zero-duration scene behavior.
- Do not add a separate per-scene setting. Both settings are application-wide.

## Settings Model

`AppSettings` gains two persisted, projected fields:

| Rust field | Serialized and TypeScript field | Type | Default | Normalization |
| --- | --- | --- | --- | --- |
| `same_scene_recall_enabled` | `sameSceneRecallEnabled` | boolean | `true` | unchanged |
| `same_scene_recall_threshold_ms` | `sameSceneRecallThresholdMs` | integer | `500` | clamp to `0..=5000` |

The existing `#[serde(default)]` settings behavior supplies these defaults when an older `settings.json` omits the fields. Full-object settings replacement, immediate persistence, projector delivery, and settings write error behavior remain unchanged.

The existing `settings_updated` tracing event includes both new values as diagnostic fields.

## Recall Trigger Gating

The configurable threshold replaces only the existing `SAME_SCENE_REPEAT_DELAY` behavior. It applies whether same-scene finishing is enabled or disabled.

The threshold controls these two comparisons:

- A scene identity equal to the last accepted trigger is suppressed while elapsed time is less than the configured threshold.
- A scene identity equal to the arming baseline is suppressed while elapsed time from the baseline observation is less than the configured threshold.

The boundary remains inclusive: a repeat observed at exactly the configured threshold is accepted. A threshold of 0 therefore accepts every otherwise eligible same-identity observation.

All independent timing and safety gates remain unchanged:

- The scenes actor coalesces scene observations through the 25 ms settle delay.
- Each connection generation begins with the 2-second arming window.
- Scene-list changes open the separate 500 ms edit suppression window.
- An accepted observation still requires a fresh connected LV1 snapshot with exact scene index and name.
- Normal scene configuration, target, topology, lockout, and generation validation still runs before a fade command is sent.

## Settings Delivery And Ordering

Lifecycle obtains the current `AppSettings` snapshot when constructing a connected scenes actor. The scenes actor retains the relevant settings and updates them from ordered `SettingsEvent::StateChanged` facts.

The settings actor publishes `SettingsEvent::StateChanged` before acknowledging a successful replacement. Event-bus ordering therefore lets a scenes actor consume that settings fact before a later LV1 scene fact produced by a subsequent recall. A settings replacement that fails to persist publishes no event, so connected actors retain the last accepted settings.

The scenes actor selects one explicit execution mode after full recall validation and includes it in `FadeCommand::RecallSceneFade`. The fade actor does not independently subscribe to the setting for this decision. This avoids a race in which the scenes and fade actors could apply different settings snapshots to one recall.

An already-dispatched fade command keeps the mode selected for that recall. Later settings changes affect later accepted scene observations.

## Fade Execution Modes

The validated recall command carries an explicit mode with these semantics.

### Finish Active Scene Targets

When `sameSceneRecallEnabled` is true, current behavior remains unchanged:

- If the exact scene index and name own active timed targets, rewrite all targets owned by that scene for completion on the next eligible scheduler tick.
- Preserve each target's stored exact final value, parameter key, scene ownership, and generation.
- Do not create replacement timelines from the incoming target list.
- Targets owned by other scenes remain active.
- If the exact scene owns no active targets, start the incoming timed targets normally.

### Override Matching Targets

When `sameSceneRecallEnabled` is false, the fade actor bypasses scene-owned finishing and uses the existing different-scene overlap path even when the incoming scene identity matches active target ownership:

- For each incoming target key, derive the start value through the existing active/current-live-value rules.
- Remove the existing target with that key.
- Create a full-duration replacement timeline from the derived current value to the incoming stored target.
- Do not move a fader back to the original start of the first fade.
- Leave active targets with non-matching keys unchanged, including targets owned by the same scene that are absent from the incoming target list.

This mode is target replacement, not immediate completion and not a rewind-and-restart operation.

### Shared Readiness And Safety

Both modes use the latest validated recall to start or reset the connection-wide post-recall readiness barrier. No parameter write occurs until two qualifying newer same-generation LV1 pings release the barrier. The existing five-second timeout aborts all paused fades.

Both modes retain existing generation checks, fresh LV1 state acquisition, exact scene identity validation, manual override cancellation, Abort All, disconnect handling, event-bus lag handling, and exact final-value writes. Blocked, skipped, disabled, stale, or unsafe recalls never reach either mode and cannot alter active fade state.

Zero-duration recalls continue through their existing immediate-write path before timed same-scene mode selection.

## User Interface

The Settings tab adds two rows in the General section:

- **Same scene recall finishing** uses a toggle and is enabled by default. Help text explains that a repeated accepted recall completes that scene's active fade targets when enabled, and replaces matching fades from their current values when disabled.
- **Same scene recall threshold** uses a stepper, displays milliseconds, has a range of 0 through 5000, increments by 100, and defaults to 500. Help text explains that it suppresses repeated identical LV1 scene notifications below the threshold.

The threshold remains editable while finishing is disabled because duplicate suppression applies in both execution modes.

`StepperControl` gains reusable optional step-size and display-format support. Existing callers retain a step size of 1 and the current numeric display without changes.

The Settings tab continues composing and submitting full `AppSettings` replacements, including rapid updates before projector refreshes.

## Logging And Diagnosis

Current same-scene finishing retains the `fade_same_scene_finishing` `INFO` event.

When finishing is disabled and an accepted repeated exact-scene recall replaces matching active targets, the fade layer emits one `INFO` event with stable event name `fade_same_scene_overriding`. Its complete user-facing message states that the repeated scene recall is overriding active fade targets from their current values. Diagnostic fields include scene index, scene name, and replacement target count.

Ordinary first recalls and recalls for scenes that own no active targets retain the normal `fade_started` outcome. Readiness barrier progress remains `DEBUG` to avoid frontend noise.

Together with extensive diagnostics, these events let hardware testing distinguish:

- Duplicate observations suppressed below the threshold.
- Accepted repeats selecting immediate finishing.
- Accepted repeats selecting current-value target replacement.
- Fader movement unrelated to same-scene behavior.

## Testing

Tests follow the repository's approved categories.

### Pure Rust Unit Tests

- `AppSettings::default()` enables same-scene finishing and uses a 500 ms threshold.
- Older partial settings documents deserialize with both defaults.
- Normalization clamps threshold values to `0..=5000`.
- The recall gate suppresses a same identity below a supplied threshold and accepts it exactly at the threshold.
- A zero threshold accepts an otherwise eligible same-identity observation.
- Baseline echo suppression uses the supplied threshold.

### Rust Actor Tests

Scenes actor tests interact through the mailbox and `AppEventBus`:

- Initial settings are applied before scene observations.
- A successful `SettingsEvent::StateChanged` changes the threshold used by later observations.
- The scenes actor sends finish mode when enabled and override mode when disabled.
- Lockout, missing configuration, empty targets, stale generation, disconnected state, topology failures, and exact identity mismatch still send no fade command.

Fade actor tests interact through the fade mailbox, fake LV1 mailbox, `AppEventBus`, tracing listener where relevant, and observable LV1 writes:

- Enabled repeated exact-scene recall finishes only that scene's active targets after readiness.
- Disabled repeated exact-scene recall replaces matching keys with full-duration timelines from their current values and does not send an immediate target write.
- Disabled mode leaves non-matching targets active.
- Both modes preserve the two-ping readiness barrier.
- Manual override, Abort All, disconnect, generation change, and readiness timeout prevent unsafe deferred writes.
- Disabled same-scene replacement emits one `fade_same_scene_overriding` log with the exact scene and replacement count.

### Frontend Tests

Vitest and Storybook interaction coverage verifies:

- Both settings render from projected state.
- The toggle submits a full settings replacement.
- The threshold stepper submits 100 ms changes and respects 0 and 5000 bounds.
- The stepper displays the `ms` unit without changing existing sensitivity behavior.
- Rapid updates compose against the latest draft settings.

Intentional Settings-tab visual changes update the Docker-backed visual snapshot.

### Verification

Use targeted Rust and frontend tests during implementation, then run `make check`. Run `make visual-test` or update the visual baseline when the Settings-tab change intentionally alters it.

When an LV1-compatible hardware target is available, run `make smoke`, inspect `logs/debug-smoke-report.txt`, and manually exercise both execution modes and multiple thresholds. Hardware smoke evidence is required before drawing conclusions about LV1's duplicate scene-notification timing, but inability to access hardware does not block implementation verification through automated tests.

## Documentation

- Update `docs/architecture.md` to describe settings-driven same-scene finish versus override behavior and the configurable repeat gate.
- Update `site/docs/settings.md` with both active settings and their diagnostic purpose.
- Keep the default behavior documentation consistent with enabled finishing and a 500 ms threshold.

## Acceptance Criteria

- Existing settings files load with same-scene finishing enabled and a 500 ms threshold.
- The threshold is persisted, projected, editable from 0 through 5000 ms in 100 ms increments, and applied to same-identity repeat and baseline-echo suppression.
- Disabling finishing never makes an accepted repeated recall immediately complete active timed targets.
- Disabled mode replaces matching timelines from their current values for the full configured duration without rewinding faders.
- Enabling finishing preserves exact scene-owned completion after the readiness barrier.
- Other timing gates and all safety-critical recall and fade behavior remain unchanged.
- Logs clearly distinguish finishing from disabled-mode current-value replacement.
