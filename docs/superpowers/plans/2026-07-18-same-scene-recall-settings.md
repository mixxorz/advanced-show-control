# Same-Scene Recall Settings Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add application settings that independently control same-scene finishing and the same-identity repeat suppression threshold so hardware duplicate-recall behavior can be diagnosed safely.

**Architecture:** Persist and project both values through the existing `AppSettings` full-object replacement flow. Lifecycle subscribes each connected scenes actor to settings facts before acquiring its current settings snapshot, and the actor refreshes settings through the app-lifetime settings mailbox after any subscriber lag; this prevents stale finishing policy from surviving a missed event. The scenes actor applies the configurable threshold and sends an explicit finish-or-override execution mode with each validated fade command, while the fade actor remains responsible for safe target execution and readiness.

**Tech Stack:** Rust, Tokio actors, Tauri, serde, tracing, React, TypeScript, Vitest, Storybook, Playwright visual tests, Zensical.

## Global Constraints

- `sameSceneRecallEnabled` defaults to `true` for existing and new settings files.
- `sameSceneRecallThresholdMs` defaults to `500`, is clamped to `0..=5000`, and the UI changes it in 100 ms increments.
- The configurable threshold replaces only the existing same-identity repeat delay; the 25 ms settle delay, 2-second arming window, 500 ms scene-list-edit suppression window, and 2-second fresh-state timeout remain unchanged.
- Threshold suppression applies whether finishing is enabled or disabled; exactly-at-threshold observations are accepted, and threshold `0` accepts every otherwise eligible repeat.
- Disabled finishing replaces matching target timelines from current interpolated/live values for the full configured duration. It must not rewind to the first fade's original start or immediately send exact targets.
- Both execution modes preserve exact scene identity validation, lockout, current-generation guards, post-recall two-ping readiness, five-second readiness timeout, manual override, Abort All, disconnect, overlap, and zero-duration behavior.
- Blocked, skipped, stale, disconnected, or unsafe recalls must not alter active fades.
- A scenes actor that cannot refresh settings after event-bus lag must stop processing recalls rather than continue with potentially stale same-scene policy.
- Rust behavior tests must use pure unit tests or actor-mailbox/`AppEventBus` tests; do not inspect or mutate private actor state for side-effecting behavior.
- Use `cargo nextest run`, not `cargo test`, for Rust verification.
- Do not claim hardware behavior is verified unless `make smoke` runs against an LV1-compatible target and `logs/debug-smoke-report.txt` shows the authoritative passing result.

---

## File Structure

- `src-tauri/src/settings/types.rs`: persisted/projected setting fields, defaults, and threshold normalization.
- `src-tauri/src/settings/actor.rs`: `settings_updated` diagnostic fields and actor-level persistence/event tests.
- `src-tauri/src/fade/commands.rs`: explicit `SameSceneRecallBehavior` command contract.
- `src-tauri/src/fade/actor.rs`: finish versus current-value override selection, readiness integration, and operational logs.
- `src-tauri/src/scenes/state.rs`: parameterized same-identity repeat gate.
- `src-tauri/src/scenes/actor.rs`: connected settings snapshot ownership, lag recovery through `SettingsHandle`, settings event updates, and validated mode dispatch.
- `src-tauri/src/lifecycle/mod.rs`: current settings lookup before constructing connected scenes actors.
- `src-tauri/src/show/actor.rs`: test call-site updates for the scenes actor constructor.
- `src-tauri/dev-tools/src/bin/lv1-probe.rs`, `src-tauri/tests/fade_engine.rs`, and `src-tauri/tests/runtime_bus.rs`: explicit behavior field at direct fade-command call sites.
- `src-tauri/src/ui/debug.rs`: production settings command registration for deterministic hardware smoke setup.
- `ui/src/debug/main.tsx`: enabled and disabled hardware smoke coverage with deterministic settings restoration.
- `ui/src/types.ts`: TypeScript settings contract, corrected Rust mirror reference, and disconnected defaults.
- `ui/src/components/StepperControl.tsx`: reusable optional step size and value formatter.
- `ui/src/components/SettingsTab.tsx`: toggle and threshold controls.
- `ui/src/components/SettingsTab.test.tsx`: full-object replacement and bounded 100 ms interaction tests.
- `ui/tests/visual/storybook.visual.spec.ts-snapshots/settings-settingstab--default.png` and `ui/tests/visual/storybook.visual.spec.ts-snapshots/app-appshell--settings-tab.png`: intentional Settings UI baselines.
- `docs/architecture.md`: settings-driven repeat gate and fade execution semantics.
- `site/docs/settings.md`: operator-facing settings documentation.
- `site/docs/assets/screenshots/settings.png`: copied revision-matched Settings visual baseline.

---

### Task 1: Persist And Project The Settings Contract

**Files:**
- Modify: `src-tauri/src/settings/types.rs:5-139`
- Modify: `src-tauri/src/settings/actor.rs:149-160,188-242,471-487`
- Modify: `ui/src/types.ts:1,31-38,151-168`
- Modify: `ui/src/components/SettingsTab.test.tsx:20-59`

**Interfaces:**
- Produces: `AppSettings::same_scene_recall_enabled: bool`
- Produces: `AppSettings::same_scene_recall_threshold_ms: u64`
- Produces: serialized fields `sameSceneRecallEnabled` and `sameSceneRecallThresholdMs`
- Produces: normalized threshold range `0..=5000`
- Consumes: existing `#[serde(default)]`, full-object replacement, settings projector, and TypeScript mirror.

- [ ] **Step 1: Add failing Rust default, compatibility, and normalization tests**

Extend `settings::types::tests` with these assertions:

```rust
#[test]
fn default_settings_enable_same_scene_finishing_with_500ms_threshold() {
    let settings = AppSettings::default();

    assert!(settings.same_scene_recall_enabled);
    assert_eq!(settings.same_scene_recall_threshold_ms, 500);
}

#[test]
fn partial_settings_use_same_scene_defaults() {
    let settings: AppSettings = serde_json::from_str(r#"{"autoSaveSessions":true}"#)
        .expect("partial settings should deserialize");

    assert!(settings.same_scene_recall_enabled);
    assert_eq!(settings.same_scene_recall_threshold_ms, 500);
}

#[test]
fn normalization_clamps_same_scene_threshold() {
    let settings = AppSettings {
        same_scene_recall_threshold_ms: 9_999,
        ..Default::default()
    }
    .normalized();

    assert_eq!(settings.same_scene_recall_threshold_ms, 5_000);
}
```

- [ ] **Step 2: Run the focused Rust tests and verify failure**

Run: `cargo nextest run -p advanced-show-control settings::types::tests`

Expected: compilation fails because the two fields do not exist.

- [ ] **Step 3: Add the Rust settings fields, defaults, and normalization**

Update `AppSettings` and its implementations:

```rust
pub struct AppSettings {
    pub auto_load_last_show_file: bool,
    pub auto_save_sessions: bool,
    pub keyboard_shortcuts: KeyboardShortcutSettings,
    pub time_display: TimeDisplayFormat,
    pub fader_override_sensitivity: u8,
    pub enable_extensive_diagnostics: bool,
    pub same_scene_recall_enabled: bool,
    pub same_scene_recall_threshold_ms: u64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            auto_load_last_show_file: false,
            auto_save_sessions: false,
            keyboard_shortcuts: KeyboardShortcutSettings::default(),
            time_display: TimeDisplayFormat::TwentyFourHour,
            fader_override_sensitivity: 9,
            enable_extensive_diagnostics: false,
            same_scene_recall_enabled: true,
            same_scene_recall_threshold_ms: 500,
        }
    }
}

impl AppSettings {
    pub fn normalized(mut self) -> Self {
        self.fader_override_sensitivity = self.fader_override_sensitivity.clamp(1, 10);
        self.same_scene_recall_threshold_ms = self.same_scene_recall_threshold_ms.clamp(0, 5_000);
        self.keyboard_shortcuts = self.keyboard_shortcuts.normalized();
        self
    }
}
```

Keep `#[serde(rename_all = "camelCase")]` and `#[serde(default)]` unchanged so old files receive defaults.

- [ ] **Step 4: Extend actor persistence and settings log tests**

Add fields to `CapturedLogEvent` and its visitor:

```rust
struct CapturedLogEvent {
    event: Option<String>,
    enable_extensive_diagnostics: Option<bool>,
    same_scene_recall_enabled: Option<bool>,
    same_scene_recall_threshold_ms: Option<String>,
}
```

Record `same_scene_recall_enabled` as a boolean and `same_scene_recall_threshold_ms` using the existing debug-string pattern. Extend `actor_normalizes_replacement_saves_file_and_publishes_event` to submit `same_scene_recall_threshold_ms: 9_999`, assert the saved JSON contains `"sameSceneRecallThresholdMs": 5000`, and assert the event carries `5000`.

Replace the focused logging test body with:

```rust
super::log_settings_updated(&AppSettings {
    enable_extensive_diagnostics: true,
    same_scene_recall_enabled: false,
    same_scene_recall_threshold_ms: 1_200,
    ..Default::default()
});

let events = captured.0.lock().unwrap();
assert!(events.iter().any(|event| {
    event.event.as_deref() == Some("settings_updated")
        && event.enable_extensive_diagnostics == Some(true)
        && event.same_scene_recall_enabled == Some(false)
        && event.same_scene_recall_threshold_ms.as_deref() == Some("1200")
}));
```

- [ ] **Step 5: Add both diagnostic fields to `settings_updated`**

Extend `log_settings_updated`:

```rust
same_scene_recall_enabled = settings.same_scene_recall_enabled,
same_scene_recall_threshold_ms = settings.same_scene_recall_threshold_ms,
```

Keep the complete message `"Settings updated"`; do not add a duplicate log at the command adapter or projector.

- [ ] **Step 6: Run focused Rust settings tests**

Run: `cargo nextest run -p advanced-show-control settings`

Expected: all settings type, state, actor, persistence, and log tests pass.

- [ ] **Step 7: Add the TypeScript fields and disconnected defaults**

Update `AppSettings`:

```ts
export type AppSettings = {
  autoLoadLastShowFile: boolean;
  autoSaveSessions: boolean;
  keyboardShortcuts: KeyboardShortcutSettings;
  timeDisplay: TimeDisplayFormat;
  faderOverrideSensitivity: number;
  enableExtensiveDiagnostics: boolean;
  sameSceneRecallEnabled: boolean;
  sameSceneRecallThresholdMs: number;
};
```

Add these defaults to `disconnectedAppViewState.settings` and the explicit settings fixture in `SettingsTab.test.tsx`:

```ts
sameSceneRecallEnabled: true,
sameSceneRecallThresholdMs: 500,
```

Correct the contract comment at `ui/src/types.ts:1` while this boundary is being edited:

```ts
// Keep these types in sync with src-tauri/src/projector/view.rs; Rust owns AppViewState snapshots; TS mirrors serialized Tauri event payloads; update both and run npm run typecheck.
```

- [ ] **Step 8: Verify contract formatting and type safety**

Run: `cargo fmt --all -- --check && npm --prefix ui run format:check && npm --prefix ui run typecheck`

Expected: all commands pass.

- [ ] **Step 9: Commit the settings contract**

```bash
git add src-tauri/src/settings/types.rs src-tauri/src/settings/actor.rs ui/src/types.ts ui/src/components/SettingsTab.test.tsx
git commit -m "feat: add same-scene recall settings"
```

---

### Task 2: Add Explicit Fade Finish And Override Modes

**Files:**
- Modify: `src-tauri/src/fade/commands.rs:1-16`
- Modify: `src-tauri/src/fade/mod.rs:11-17`
- Modify: `src-tauri/src/fade/actor.rs:46-50,112-151,374-519,710-834,2632-2839,3500-3572`
- Modify: `src-tauri/src/scenes/actor.rs:500-509,2302-2320`
- Modify: `src-tauri/dev-tools/src/bin/lv1-probe.rs:750-775,1155-1180`
- Modify: `src-tauri/tests/fade_engine.rs:220-242`
- Modify: `src-tauri/tests/runtime_bus.rs:88-104`

**Interfaces:**
- Produces: `SameSceneRecallBehavior::{FinishActiveTargets, OverrideMatchingTargets}`
- Produces: `FadeCommand::RecallSceneFade { same_scene_behavior, .. }`
- Consumes: `EngineState::finish_scene_on_next_tick`, current parameter-key replacement path, fresh LV1 snapshot, generation checks, and readiness barrier.

- [ ] **Step 1: Add failing actor tests for disabled-mode current-value replacement**

Add an explicit behavior argument helper beside `start_fade_for_generation`:

```rust
async fn start_fade_with_behavior(
    engine: &FadeEngineHandle,
    config: FadeConfig,
    expected_generation: Option<u64>,
    same_scene_behavior: SameSceneRecallBehavior,
) -> Result<(), AppCommandError> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    engine
        .send(FadeCommand::RecallSceneFade {
            config,
            same_scene_behavior,
            expected_generation,
            reply: Some(reply),
        })
        .await
        .map_err(|_| AppCommandError::FadeUnavailable)?;
    rx.await.map_err(|_| AppCommandError::ReplyChannelClosed)?
}
```

Keep `start_fade_for_generation` as a wrapper that supplies `FinishActiveTargets`, preserving all existing tests. Add an actor test named `repeated_scene_override_replaces_from_current_value_without_immediate_finish` that:

```rust
let config = fade_config(
    scene(17, "Verse"),
    vec![FadeTarget {
        group: 0,
        channel: 1,
        parameter: FadeParameter::FaderDb,
        target: 0.0,
    }],
    1_000,
);

start_fade_for_generation(&engine, config.clone(), Some(7)).await.unwrap();
publish_ping(&event_bus, 7, 41);
publish_ping(&event_bus, 7, 42);
tokio::time::advance(Duration::from_millis(250)).await;
let before_override = next_write_batch(&mut write_rx).await;
let before_value = before_override
    .iter()
    .find(|write| write.channel == 1)
    .expect("initial fade should be moving")
    .value;
while write_rx.try_recv().is_ok() {}

start_fade_with_behavior(
    &engine,
    config,
    Some(7),
    SameSceneRecallBehavior::OverrideMatchingTargets,
)
.await
.unwrap();
tokio::task::yield_now().await;
assert!(matches!(
    write_rx.try_recv(),
    Err(tokio::sync::mpsc::error::TryRecvError::Empty)
));
publish_ping(&event_bus, 7, 43);
tokio::task::yield_now().await;
assert!(matches!(
    write_rx.try_recv(),
    Err(tokio::sync::mpsc::error::TryRecvError::Empty)
));
publish_ping(&event_bus, 7, 44);
tokio::time::advance(Duration::from_millis(100)).await;
let after_override = next_write_batch(&mut write_rx).await;
let after_value = after_override
    .iter()
    .find(|write| write.channel == 1)
    .expect("replacement fade should resume")
    .value;

assert!(after_value >= before_value, "replacement must not rewind");
assert!(after_value < 0.0, "replacement must not finish immediately");
```

Use non-advancing `try_recv` checks here rather than the existing 100 ms timeout helper so readiness assertions do not consume the replacement timeline under paused Tokio time.

Continue the test through the replacement duration:

```rust
tokio::time::advance(Duration::from_millis(800)).await;
let before_deadline = next_write_batch(&mut write_rx).await;
assert!(before_deadline.iter().any(|write| {
    write.channel == 1
        && write.parameter == Lv1WriteParameter::FaderDb
        && write.value < 0.0
}));

tokio::time::advance(Duration::from_millis(200)).await;
let at_deadline = next_write_batch(&mut write_rx).await;
assert!(at_deadline.contains(&Lv1ParameterWrite {
    group: 0,
    channel: 1,
    parameter: Lv1WriteParameter::FaderDb,
    value: 0.0,
}));
```

This proves the second recall uses a full new duration rather than merely avoiding the first immediate write.

Add `repeated_scene_override_leaves_omitted_same_scene_target_active`. Build the initial targets and reduced override explicitly:

```rust
let initial = fade_config(
    scene(17, "Verse"),
    vec![
        FadeTarget { group: 0, channel: 1, parameter: FadeParameter::FaderDb, target: -10.0 },
        FadeTarget { group: 0, channel: 2, parameter: FadeParameter::FaderDb, target: -5.0 },
    ],
    1_000,
);
let reduced = fade_config(
    scene(17, "Verse"),
    vec![FadeTarget { group: 0, channel: 1, parameter: FadeParameter::FaderDb, target: 0.0 }],
    1_000,
);
```

Start `initial`, release readiness, advance 200 ms, clear observed writes, then send `reduced` with `OverrideMatchingTargets` and release the reset barrier. Assert the next moving batch contains both channels and that channel 2 is not sent its exact `-5.0` target early:

```rust
let resumed = next_write_batch(&mut write_rx).await;
assert!(resumed.iter().any(|write| write.channel == 1));
assert!(resumed.iter().any(|write| write.channel == 2));
assert!(!resumed.iter().any(|write| write.channel == 2 && write.value == -5.0));
```

Add `repeated_scene_finish_uses_active_ownership_when_incoming_scope_changed`: start the same `initial` two-target fade, then send `reduced` with `FinishActiveTargets`. After readiness, assert one completion batch contains both original stored exact values:

```rust
assert!(writes.contains(&Lv1ParameterWrite {
    group: 0,
    channel: 1,
    parameter: Lv1WriteParameter::FaderDb,
    value: -10.0,
}));
assert!(writes.contains(&Lv1ParameterWrite {
    group: 0,
    channel: 2,
    parameter: Lv1WriteParameter::FaderDb,
    value: -5.0,
}));
```

This protects enabled finishing when copy/paste changes a scene's configured scope during an active fade.

Use fake snapshots with monotonically increasing ping boundaries matching the published sequences. Interact only through the fade mailbox, fake LV1 mailbox, and `AppEventBus`; do not inspect `EngineState` directly.

- [ ] **Step 2: Run the new actor test and verify failure**

Run: `cargo nextest run -p advanced-show-control repeated_scene_override`

Expected: compilation fails because `SameSceneRecallBehavior` and the command field do not exist.

- [ ] **Step 3: Define and export the explicit command behavior**

In `fade/commands.rs` add:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSceneRecallBehavior {
    FinishActiveTargets,
    OverrideMatchingTargets,
}
```

Add `same_scene_behavior: SameSceneRecallBehavior` to `FadeCommand::RecallSceneFade`. Re-export it from `fade/mod.rs`:

```rust
pub use commands::{FadeCommand, SameSceneRecallBehavior};
```

At every current direct call site, including `src-tauri/src/scenes/actor.rs:505-509`, explicitly pass `SameSceneRecallBehavior::FinishActiveTargets` to preserve current behavior and keep Task 2 independently compilable. Task 3 replaces only the scenes actor's temporary fixed value with settings-selected dispatch. Do not add hidden handle convenience methods.

- [ ] **Step 4: Route the mode through the fade actor without changing zero-duration behavior**

Destructure `same_scene_behavior` in `run_engine` and pass it to `handle_recall_scene_fade`:

```rust
async fn handle_recall_scene_fade(
    runtime_generation: &RuntimeGeneration,
    lv1: &Lv1ActorHandle,
    state: &mut EngineState,
    config: FadeConfig,
    same_scene_behavior: SameSceneRecallBehavior,
    expected_generation: Option<u64>,
) -> Result<RecallSceneFadeOutcome, AppCommandError>
```

Leave the empty-target and zero-duration branches before mode selection. Replace unconditional finish selection with:

```rust
let finishing_target_count = match same_scene_behavior {
    SameSceneRecallBehavior::FinishActiveTargets => {
        state.finish_scene_on_next_tick(&config.scene)
    }
    SameSceneRecallBehavior::OverrideMatchingTargets => 0,
};
```

Before mutating targets, establish whether this is actually a repeated active-scene recall and count the active keys the incoming command will replace. Count active targets rather than incoming targets so duplicate incoming keys cannot inflate diagnostics, and include overlapping keys currently owned by another scene because the existing replacement loop removes them too:

```rust
let scene_owns_active_targets = state
    .channels
    .iter()
    .any(|active| active.scene == config.scene);
let overriding_target_count = if same_scene_behavior
    == SameSceneRecallBehavior::OverrideMatchingTargets
    && scene_owns_active_targets
{
    state
        .channels
        .iter()
        .filter(|active| config.targets.iter().any(|target| active.key == target.key()))
        .count()
} else {
    0
};
```

Run the existing target-key replacement loop whenever `finishing_target_count == 0`. Extend `RecallSceneFadeOutcome` with `Overriding { target_count: usize }`; return it only when an exact-scene target was active and at least one active key was actually replaced. Otherwise return `Started`. Active exact-scene targets omitted from the incoming config remain untouched.

- [ ] **Step 5: Add the distinct disabled-mode operational log test**

Add `repeated_scene_override_logs_one_current_value_outcome` beside the finishing log test. Start `Chorus` owning channel 1 and `Verse` owning channel 2, then send an override-mode `Verse` config containing both keys. This proves the count includes every active key actually replaced while the exact repeated scene owns at least one active target. Assert exactly:

```rust
CapturedWarnEvent {
    level: Some("INFO".to_string()),
    event: Some("fade_same_scene_overriding".to_string()),
    message: Some(
        "Repeated scene recall is overriding active fade targets from their current values for 17: Verse (2 targets)"
            .to_string(),
    ),
    scene_index: Some("17".to_string()),
    scene_name: Some("Verse".to_string()),
    target_count: Some("2".to_string()),
    ..Default::default()
}
```

Also assert the two initial commands emit exactly two `fade_started` logs and the override command emits no additional `fade_started` log.

- [ ] **Step 6: Emit the mode-specific outcome without duplicate logs**

Handle the new outcome beside existing `Started` and `Finishing`:

```rust
RecallSceneFadeOutcome::Overriding { target_count } => tracing::info!(
    event = "fade_same_scene_overriding",
    scene_index,
    scene_name = %scene_name,
    target_count,
    "Repeated scene recall is overriding active fade targets from their current values for {}: {} ({} targets)",
    scene_index,
    scene_name,
    target_count
),
```

Do not emit a second `fade_started` for `Finishing` or `Overriding`. Both outcomes still fan out the existing `FadeStarted` fact and reset the readiness barrier.

- [ ] **Step 7: Run focused fade actor tests**

Run: `cargo nextest run -p advanced-show-control fade::actor::tests`

Expected: finish, full-duration override, omitted-target continuation, overlap, exact-write, readiness, manual override, abort, disconnect, generation, timeout, and logging actor tests pass.

- [ ] **Step 8: Run broader fade tests, lint, and standalone probe checks**

Run: `cargo nextest run -p advanced-show-control fade && cargo clippy -p advanced-show-control --all-targets -- -D warnings && cargo check --manifest-path src-tauri/dev-tools/Cargo.toml --all-targets`

Expected: all fade tests pass, clippy reports no warnings, and the standalone `lv1-probe` crate compiles with its explicit behavior field.

- [ ] **Step 9: Commit explicit fade behavior**

```bash
git add src-tauri/src/fade src-tauri/src/scenes/actor.rs src-tauri/dev-tools/src/bin/lv1-probe.rs src-tauri/tests/fade_engine.rs src-tauri/tests/runtime_bus.rs
git commit -m "feat: add same-scene fade override mode"
```

---

### Task 3: Apply Ordered Settings To The Recall Gate And Mode Dispatch

**Files:**
- Modify: `src-tauri/src/scenes/state.rs:1-86,351-422,562-645`
- Modify: `src-tauri/src/scenes/actor.rs:5-19,66-127,117-315,418-564,684-860,1900-1980,2114-2370`
- Modify: `src-tauri/src/lifecycle/mod.rs:9-18,81-109,282-307,357-425,730-756,1200-1215`
- Modify: `src-tauri/src/show/actor.rs:634,735,958,1004,1071,1222`

**Interfaces:**
- Consumes: `AppSettings::{same_scene_recall_enabled, same_scene_recall_threshold_ms}` from Task 1.
- Consumes: `SameSceneRecallBehavior` from Task 2.
- Produces: `build_scenes_actor(generation, runtime_generation, event_bus, events, settings_handle, initial_settings)` where `events` is subscribed before snapshot acquisition.
- Produces: configurable `ScenesState::accepts(current_scene, same_scene_repeat_delay)`.
- Produces: settings facts applied by the scenes actor before later ordered LV1 scene facts.

- [ ] **Step 1: Write failing pure gate tests for custom, exact-boundary, and zero thresholds**

Change `ScenesState::accepts_at` tests to pass an explicit delay and add:

```rust
#[test]
fn configurable_repeat_delay_accepts_exact_boundary() {
    let mut state = ScenesState::default();
    let scene = scene(1, "Intro");
    let start = Instant::now();
    let delay = Duration::from_millis(1_200);

    assert!(!state.accepts_at(&scene, start, delay));
    assert!(state.accepts_at(&scene, start + RECALL_ARMING_DELAY, delay));
    assert!(!state.accepts_at(
        &scene,
        start + RECALL_ARMING_DELAY + delay - Duration::from_millis(1),
        delay,
    ));
    assert!(state.accepts_at(
        &scene,
        start + RECALL_ARMING_DELAY + delay,
        delay,
    ));
}

#[test]
fn zero_repeat_delay_accepts_immediate_repeat_after_arming() {
    let mut state = ScenesState::default();
    let scene = scene(1, "Intro");
    let start = Instant::now();

    assert!(!state.accepts_at(&scene, start, Duration::ZERO));
    assert!(state.accepts_at(
        &scene,
        start + RECALL_ARMING_DELAY,
        Duration::ZERO,
    ));
    assert!(state.accepts_at(
        &scene,
        start + RECALL_ARMING_DELAY,
        Duration::ZERO,
    ));
}
```

Update the baseline echo test to use a non-default custom delay and prove the same supplied delay controls that branch too.

- [ ] **Step 2: Run gate tests and verify failure**

Run: `cargo nextest run -p advanced-show-control scenes::state::tests`

Expected: compilation fails because `accepts_at` does not accept a delay.

- [ ] **Step 3: Parameterize only the same-identity repeat comparisons**

Remove `SAME_SCENE_REPEAT_DELAY` and change signatures:

```rust
pub(crate) fn accepts(
    &mut self,
    current_scene: &SceneState,
    same_scene_repeat_delay: Duration,
) -> bool {
    self.accepts_at(current_scene, Instant::now(), same_scene_repeat_delay)
}

pub(crate) fn accepts_at(
    &mut self,
    current_scene: &SceneState,
    now: Instant,
    same_scene_repeat_delay: Duration,
) -> bool
```

Pass `same_scene_repeat_delay` into `decide_armed` and use it in both existing `<` comparisons. Do not alter `RECALL_ARMING_DELAY` or `SCENE_LIST_EDIT_SUPPRESSION_WINDOW`.

Update the `RecallGate` and `decide_armed` comments so they refer to the supplied same-scene repeat delay rather than the removed `SAME_SCENE_REPEAT_DELAY` constant. Update the scenes actor timing comment from a fixed `500 ms repeat delay` to a configurable repeat delay with a 500 ms default; leave the other fixed timing comments unchanged.

- [ ] **Step 4: Run pure gate tests**

Run: `cargo nextest run -p advanced-show-control scenes::state::tests`

Expected: all scene-state tests pass, including exact-boundary, zero, baseline, and scene-list-edit behavior.

- [ ] **Step 5: Add failing scenes actor tests for initial settings and live updates**

Change the fake fade observation channel from `FadeConfig` to:

```rust
type ObservedFadeCommand = (FadeConfig, SameSceneRecallBehavior);
```

Update `fake_fade_handle`, `next_fade_command`, and every existing caller to use `ObservedFadeCommand`. Destructure `(config, behavior)` before existing `FadeConfig` field assertions, and assert `FinishActiveTargets` in existing tests that preserve default behavior.

Add an initial-settings test using `intro_scene_document()`, which explicitly enables fader scope and supplies a stored value for its scoped channel. Do not build this test from a newly aligned/default scene config because current defaults intentionally have empty scope. Build the actor with:

```rust
AppSettings {
    same_scene_recall_enabled: false,
    same_scene_recall_threshold_ms: 1_200,
    ..Default::default()
}
```

After arming, publish one accepted `Intro`, receive the fake fade command, and assert:

```rust
assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);
```

Then advance 900 ms, publish the same identity, advance through the 25 ms settle delay, and assert no second command. Advance until more than 1,200 ms has elapsed from the accepted trigger, publish again, settle, and assert a second override command. Keep exact-boundary coverage in the pure state test because actor acceptance intentionally occurs after the independent settle delay.

Add a second actor test that starts with defaults, publishes:

```rust
event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
    settings: AppSettings {
        same_scene_recall_enabled: false,
        same_scene_recall_threshold_ms: 1_000,
        ..Default::default()
    },
}));
```

before the next `SceneChanged`, then verifies the later command uses `OverrideMatchingTargets` and the 1,000 ms delay.

Add `settings_update_preserves_scene_settings_clipboard_and_pasted_recall_config`:

Install a document whose destination is the fake LV1's `Intro` and whose source has distinct settings:

```rust
let source_id = uuid::Uuid::from_u128(0x22222222222242228222222222222222);
let destination_id = intro_internal_scene_id();
let mut destination_document = intro_scene_document();
let destination = destination_document.scene_configs.remove(0);
let document = SceneDocument {
    scene_configs: vec![
        SceneConfig {
            internal_scene_id: source_id,
            scene_index: Some(2),
            scene_name: "Source".to_string(),
            duration_ms: 7_777,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 2,
                fader_db: Some(-3.0),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            scoped_channels: vec![ChannelRef { group: 0, channel: 2 }],
            scope_toggles: SceneScopeToggles { faders: true, pan: false },
        },
        destination,
    ],
    selected_scene_internal_id: None,
};
```

Install the document through `ReplaceSceneDocument`, then:

```rust
let (reply, copy_rx) = oneshot::channel();
handle.send(ScenesCommand::CopySceneSettings {
    source_internal_scene_id: source_id,
    reply: Some(reply),
}).await.unwrap();
copy_rx.await.unwrap().unwrap();

event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
    settings: AppSettings {
        same_scene_recall_enabled: false,
        same_scene_recall_threshold_ms: 1_000,
        ..Default::default()
    },
}));
yield_to_actor().await;

let (reply, paste_rx) = oneshot::channel();
handle.send(ScenesCommand::PasteSceneSettings {
    destination_internal_scene_id: destination_id,
    reply: Some(reply),
}).await.unwrap();
assert!(paste_rx.await.unwrap().unwrap().changed);
```

Arm recall tracking, publish `intro_scene()`, and destructure the observed command:

```rust
let (config, behavior) = next_fade_command(&mut fade_rx).await;
assert_eq!(behavior, SameSceneRecallBehavior::OverrideMatchingTargets);
assert_eq!(config.duration_ms, 7_777);
assert_eq!(config.targets.len(), 1);
assert_eq!(config.targets[0].target, -3.0);
```

Successful paste after the settings event proves the fact did not replace `ScenesState` or clear its actor-owned clipboard.

This is an actor-mailbox/`AppEventBus` test. Do not inspect the private clipboard field directly.

- [ ] **Step 6: Run the new actor tests and verify failure**

Run: `cargo nextest run -p advanced-show-control scenes::actor::tests`

Expected: compilation fails because scenes actor construction and processing do not accept settings or dispatch behavior.

- [ ] **Step 7: Give each scenes actor an initial settings snapshot**

Add `initial_settings: AppSettings` to `ScenesTask` and `build_scenes_actor`:

```rust
pub fn build_scenes_actor(
    generation: u64,
    runtime_generation: RuntimeGeneration,
    event_bus: AppEventBus,
    events: tokio::sync::broadcast::Receiver<AppEvent>,
    settings_handle: SettingsHandle,
    initial_settings: AppSettings,
) -> (ScenesHandle, ScenesTask, ScenesPeers)
```

Store the supplied `events` receiver and app-lifetime `SettingsHandle` in `ScenesTask`; do not call `event_bus.subscribe()` inside this constructor. This makes subscription ordering explicit at every call site while retaining a recovery path for missed settings facts.

Destructure it in `run_scenes_actor` as mutable local state:

```rust
let mut settings = initial_settings;
```

Keep settings as a local recall-policy snapshot beside `recall_state`; do not add settings to `ScenesState`, replace the scene document, clear `scene_settings_clipboard`, or publish `ScenesEvent::StateChanged` when settings change.

In both current event-receive branches (`src-tauri/src/scenes/actor.rs:213-237` while a scene is pending and `:279-312` while idle), consume settings facts:

```rust
Ok(AppEvent::Settings(SettingsEvent::StateChanged {
    settings: updated_settings,
})) => {
    settings = updated_settings;
}
```

Add a narrow lag-recovery function that constructs the mailbox command explicitly:

```rust
async fn refresh_settings_after_lag(settings_handle: &SettingsHandle) -> Option<AppSettings> {
    let (reply, rx) = oneshot::channel();
    if settings_handle
        .send(SettingsCommand::GetSettings { reply })
        .await
        .is_err()
    {
        tracing::error!(
            event = "scene_recall_settings_unavailable",
            "Scene recall automation stopped because current settings are unavailable"
        );
        return None;
    }
    match rx.await {
        Ok(settings) => Some(settings),
        Err(_) => {
            tracing::error!(
                event = "scene_recall_settings_unavailable",
                "Scene recall automation stopped because current settings are unavailable"
            );
            None
        }
    }
}
```

In both `RecvError::Lagged` branches, keep `log_lagged_subscriber`, then replace the local snapshot from this helper. If it returns `None`, break the actor loop. Do not process a pending or future scene observation with the pre-lag snapshot. Also call the helper immediately before processing every settled `PendingSceneObservation`; replace the local snapshot and pass it to validation. This defends against a ready settle timer winning `tokio::select!` before a queued settings event. Do not use biased selection; if this boundary refresh fails, log the existing `scene_recall_settings_unavailable` ERROR and stop before any fade command.

Pass `&settings` into `process_scene_observation`. Compute the delay once per observation:

```rust
let same_scene_repeat_delay =
    std::time::Duration::from_millis(settings.same_scene_recall_threshold_ms);
if !recall_state.accepts(&observation.scene, same_scene_repeat_delay) {
    // existing diagnostic and return
}
```

- [ ] **Step 8: Dispatch one explicit behavior after full validation**

Immediately before sending `FadeCommand::RecallSceneFade`, derive:

```rust
let same_scene_behavior = if settings.same_scene_recall_enabled {
    SameSceneRecallBehavior::FinishActiveTargets
} else {
    SameSceneRecallBehavior::OverrideMatchingTargets
};
```

Include it in the command. Do not move this command before fresh state, lockout, exact identity, topology, target, or generation checks.

- [ ] **Step 9: Load current settings before connected actor construction**

Add a private lifecycle helper that constructs the explicit settings command:

```rust
async fn settings_snapshot(&self) -> Result<crate::settings::AppSettings, String> {
    let (reply, rx) = oneshot::channel();
    self.settings
        .send(SettingsCommand::GetSettings { reply })
        .await
        .map_err(|_| "Settings are unavailable".to_string())?;
    rx.await
        .map_err(|_| "Settings reply channel is closed".to_string())
}
```

In `connect_to_identity`, create `let scene_events = event_bus.subscribe();` **before** awaiting `settings_snapshot()`. Pass `scene_events`, `self.settings.clone()`, and the snapshot into `build_connected_runtime` and then `build_scenes_actor`. In the test-only `finish_connect_transaction_inner`, subscribe and fetch settings at the start of the function, before `install_runtime_transaction`; pass the same app-lifetime handle and do not introduce an error path that leaves LV1/fade handles installed without a scenes actor. Reuse the helper in `frontend_ready` instead of duplicating the mailbox exchange.

Update every direct constructor and wrapper, including `build_scenes_actor_with_pending_scene_observer` and `build_and_spawn_scene_recall_fader_with_document`. Each ordinary test call creates `let events = event_bus.subscribe();` immediately before construction and passes a live test `SettingsHandle` plus `AppSettings::default()`; add a settings-handle-and-receiver-taking helper for policy ordering tests. Update all show actor and lifecycle test call sites the same way unless the test explicitly exercises custom settings.

Add `queued_settings_update_after_snapshot_supersedes_initial_policy` with this ordering:

```rust
let event_bus = AppEventBus::default();
let events = event_bus.subscribe();
let initial_settings = AppSettings::default();
event_bus.publish(AppEvent::Settings(SettingsEvent::StateChanged {
    settings: AppSettings {
        same_scene_recall_enabled: false,
        same_scene_recall_threshold_ms: 1_000,
        ..Default::default()
    },
}));

let (handle, task, peers) = build_scenes_actor(
    1,
    runtime_generation.clone(),
    event_bus.clone(),
    events,
    settings_handle,
    initial_settings,
);
peers.set_peers(lv1, fade);
task.spawn();
```

Install `intro_scene_document()`, arm the actor, and publish `intro_scene()`. Assert the first observed command uses `OverrideMatchingTargets`; advance 700 ms, publish and settle another observation of the same identity, and assert no command. Advance until more than 1,000 ms has elapsed from the accepted trigger, publish and settle that same identity again, and assert an override command. This reproduces the snapshot/construction race and proves the queued fact wins without conflating actor settle timing with the pure exact-boundary rule.

Add `lagged_settings_events_refresh_before_recall`: use an `AppEventBus` with a small test capacity, subscribe the scenes receiver, retain an enabled snapshot, replace settings through a real spawned settings actor with finishing disabled, and publish enough unrelated facts before spawning scenes to force `RecvError::Lagged`. After actor startup and lag recovery, publish an eligible configured scene observation and assert `OverrideMatchingTargets`. Add a companion with a closed fake settings mailbox and assert no fade command is sent after lag and one `ERROR` event named `scene_recall_settings_unavailable` carries the complete fail-closed message, proving recovery is both safe and visible.

- [ ] **Step 10: Run scenes and lifecycle tests**

Run: `cargo nextest run -p advanced-show-control scenes lifecycle show::actor::tests`

Expected: actor settings ordering, queued construction-race handling, configurable threshold, clipboard continuity, pasted recall configuration, validation boundaries, lifecycle connection, and show integration tests pass.

- [ ] **Step 11: Run all Rust tests, formatting, and lint**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo nextest run --workspace`

Expected: all commands pass.

- [ ] **Step 12: Commit settings-driven recall policy**

```bash
git add src-tauri/src/scenes src-tauri/src/lifecycle/mod.rs src-tauri/src/show/actor.rs
git commit -m "feat: configure same-scene recall policy"
```

---

### Task 4: Add Settings Tab Controls

**Files:**
- Modify: `ui/src/components/StepperControl.tsx:1-45`
- Modify: `ui/src/components/SettingsTab.tsx:96-185`
- Modify: `ui/src/components/SettingsTab.test.tsx:15-118`
- Modify: `ui/src/components/SettingsTab.stories.tsx:1-32`

**Interfaces:**
- Consumes: `AppSettings.sameSceneRecallEnabled` and `sameSceneRecallThresholdMs` from Task 1.
- Produces: `StepperControl.step?: number` defaulting to `1`.
- Produces: `StepperControl.formatValue?: (value: number) => string` defaulting to numeric display.
- Produces: full-object settings replacements for toggle and threshold interactions.

- [ ] **Step 1: Add failing frontend interaction tests**

Add these tests to `SettingsTab.test.tsx`:

```tsx
it("updates same-scene finishing while replacing the full settings object", () => {
  renderWithAppProviders(<SettingsTab />, {
    appState: disconnectedAppViewState,
  });

  fireEvent.click(screen.getByLabelText("Same scene recall finishing"));

  expect(replaceAppSettings).toHaveBeenCalledWith({
    ...disconnectedAppViewState.settings,
    sameSceneRecallEnabled: false,
  });
});

it("updates the same-scene threshold in 100ms increments", () => {
  renderWithAppProviders(<SettingsTab />, {
    appState: disconnectedAppViewState,
  });

  expect(screen.getByLabelText("Same scene recall threshold")).toHaveValue(
    "500 ms",
  );
  fireEvent.click(
    screen.getByRole("button", {
      name: "Increase Same scene recall threshold",
    }),
  );

  expect(replaceAppSettings).toHaveBeenCalledWith({
    ...disconnectedAppViewState.settings,
    sameSceneRecallThresholdMs: 600,
  });
});
```

Add a bounded case using projected values `0` and `5000`:

```tsx
it.each([
  [0, "Decrease Same scene recall threshold"],
  [5000, "Increase Same scene recall threshold"],
] as const)("keeps same-scene threshold %i within bounds", (value, buttonName) => {
  renderWithAppProviders(<SettingsTab />, {
    appState: {
      ...disconnectedAppViewState,
      settings: {
        ...disconnectedAppViewState.settings,
        sameSceneRecallEnabled: false,
        sameSceneRecallThresholdMs: value,
      },
    },
  });

  fireEvent.click(screen.getByRole("button", { name: buttonName }));
  expect(replaceAppSettings).toHaveBeenCalledWith({
    ...disconnectedAppViewState.settings,
    sameSceneRecallEnabled: false,
    sameSceneRecallThresholdMs: value,
  });
});
```

This also proves the threshold control remains available when `sameSceneRecallEnabled` is false. The existing backend no-op behavior handles a same-value bounded replacement.

- [ ] **Step 2: Run the Settings tab tests and verify failure**

Run: `npm --prefix ui run test -- SettingsTab.test.tsx`

Expected: tests fail because the controls do not exist.

- [ ] **Step 3: Extend `StepperControl` without changing existing callers**

Update props and stepping:

```tsx
export function StepperControl(props: {
  label: string;
  min: number;
  max: number;
  step?: number;
  value: number;
  formatValue?: (value: number) => string;
  onChange: (value: number) => void;
}) {
  function step(direction: 1 | -1) {
    const increment = props.step ?? 1;
    props.onChange(
      Math.min(
        props.max,
        Math.max(props.min, props.value + direction * increment),
      ),
    );
  }

  const displayValue = props.formatValue
    ? props.formatValue(props.value)
    : props.value;
```

Set the read-only input's `value` to `displayValue`. Do not alter styling or the existing sensitivity control's behavior.

- [ ] **Step 4: Add both General settings rows**

Add these rows near the other operational settings:

```tsx
<SettingRow
  help="When enabled, an accepted repeated recall completes that scene's active fade targets. When disabled, matching fades continue from their current values over the full scene duration."
  label="Same scene recall finishing"
  onHelpChange={setActiveHelp}
>
  <ToggleControl
    label="Same scene recall finishing"
    checked={settings.sameSceneRecallEnabled}
    onChange={(checked) =>
      update((current) => ({
        ...current,
        sameSceneRecallEnabled: checked,
      }))
    }
  />
</SettingRow>
<SettingRow
  help="Suppress repeated identical LV1 scene notifications below this threshold. Other scene recall timing and safety gates are unchanged."
  label="Same scene recall threshold"
  onHelpChange={setActiveHelp}
>
  <StepperControl
    label="Same scene recall threshold"
    min={0}
    max={5000}
    step={100}
    value={settings.sameSceneRecallThresholdMs}
    formatValue={(value) => `${value} ms`}
    onChange={(value) =>
      update((current) => ({
        ...current,
        sameSceneRecallThresholdMs: value,
      }))
    }
  />
</SettingRow>
```

Do not disable the threshold based on the toggle.

- [ ] **Step 5: Run frontend unit, format, lint, and type checks**

Run: `npm --prefix ui run test -- SettingsTab.test.tsx && npm --prefix ui run format:check && npm --prefix ui run lint && npm --prefix ui run typecheck`

Expected: all commands pass.

- [ ] **Step 6: Run Storybook interaction tests**

First import `expect`, `userEvent`, and `within` from `storybook/test`, then add a required interaction assertion to the default story:

```tsx
export const Default: Story = {
  render: () => <InteractiveSettingsTab />,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const finishing = canvas.getByLabelText("Same scene recall finishing");
    const threshold = canvas.getByLabelText("Same scene recall threshold");

    await expect(finishing).toHaveAttribute("aria-pressed", "true");
    await expect(threshold).toHaveValue("500 ms");
    await userEvent.click(finishing);
    await expect(finishing).toHaveAttribute("aria-pressed", "false");
    await userEvent.click(
      canvas.getByRole("button", {
        name: "Increase Same scene recall threshold",
      }),
    );
    await expect(threshold).toHaveValue("600 ms");
  },
};
```

This proves the threshold remains editable while finishing is disabled.

Run: `npm --prefix ui run test:storybook`

Expected: all Storybook browser tests pass, including the Settings defaults and disabled-finishing threshold interaction.

- [ ] **Step 7: Commit the Settings UI**

```bash
git add ui/src/components/StepperControl.tsx ui/src/components/SettingsTab.tsx ui/src/components/SettingsTab.test.tsx ui/src/components/SettingsTab.stories.tsx
git commit -m "feat: add same-scene recall controls"
```

---

### Task 5: Make Hardware Smoke Settings Deterministic

**Files:**
- Modify: `src-tauri/src/ui/debug.rs:39-70`
- Modify: `ui/src/debug/main.tsx:1-32,48-55,250-320`

**Interfaces:**
- Consumes: production `replace_app_settings` and projected `AppViewState.settings` from Tasks 1 and 3.
- Produces: debug smoke setup that explicitly restores enabled finishing and a 500 ms threshold.
- Produces: hardware case `same-scene-override` while keeping `same-scene-finish` deterministic.

- [ ] **Step 1: Register the production settings command in the debug app**

Add this command to the debug `generate_handler!` list; do not create a debug-only settings mutation path:

```rust
crate::ui::commands::settings::replace_app_settings,
```

The smoke app must exercise the same full-object replacement adapter as the normal UI.

- [ ] **Step 2: Add a full-object smoke settings helper**

Import `AppSettings` and add:

```ts
async function setSameSceneSettings(
  sameSceneRecallEnabled: boolean,
  sameSceneRecallThresholdMs: number,
) {
  const current = state?.settings;
  if (!current) throw new Error("projected settings are unavailable");
  const settings: AppSettings = {
    ...current,
    sameSceneRecallEnabled,
    sameSceneRecallThresholdMs,
  };

  await invoke("replace_app_settings", { settings });
  await waitFor(
    () =>
      state?.settings.sameSceneRecallEnabled === sameSceneRecallEnabled &&
      state.settings.sameSceneRecallThresholdMs ===
        sameSceneRecallThresholdMs,
    "projected same-scene recall settings",
  );
}
```

Add `same-scene-override` to the smoke test list immediately after `same-scene-finish`.

- [ ] **Step 3: Make the existing finish smoke independent of persisted debug settings**

Before `same-scene-finish` setup, call:

```ts
await setSameSceneSettings(true, 500);
```

In its `finally`, restore both the 1,000 ms scene duration and `setSameSceneSettings(true, 500)`. This prevents an earlier failed or interrupted smoke run from changing the expected enabled-mode result.

- [ ] **Step 4: Add disabled-mode full-duration hardware smoke coverage**

Add a production-command-driven test:

```ts
await test("same-scene-override", async () => {
  try {
    await setSameSceneSettings(false, 500);
    await reset(sceneA, targetA);
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneB,
      durationMs: sameSceneDurationMs,
    });
    await invoke("recall_scene", { internalSceneId: sceneB });
    await waitFor(async () => {
      const liveGain = await gain();
      return liveGain >= targetA + sameSceneMovementThresholdDb && liveGain < targetB - tolerance;
    }, "same-scene override movement before repeat");

    const repeatedAt = Date.now();
    await invoke("recall_scene", { internalSceneId: sceneB });
    await sleep(1_000);
    if (Math.abs((await gain()) - targetB) <= tolerance) {
      throw new Error("disabled same-scene finishing completed immediately");
    }
    await waitFor(
      async () => Math.abs((await gain()) - targetB) <= tolerance,
      "same-scene override completion",
      sameSceneDurationMs + 5_000,
    );
    if (Date.now() - repeatedAt < sameSceneDurationMs) {
      throw new Error("same-scene override did not use the full configured duration");
    }
  } finally {
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneB,
      durationMs: 1_000,
    });
    await setSameSceneSettings(true, 500);
  }
});
```

- [ ] **Step 5: Verify debug frontend and Tauri command registration compile**

Run: `npm --prefix ui run format:check && npm --prefix ui run lint && npm --prefix ui run typecheck && cargo check -p advanced-show-control --all-targets`

Expected: all commands pass, including the debug Tauri binary and smoke TypeScript entrypoint.

- [ ] **Step 6: Commit deterministic hardware smoke coverage**

```bash
git add src-tauri/src/ui/debug.rs ui/src/debug/main.tsx
git commit -m "test: smoke same-scene recall settings"
```

---

### Task 6: Update Architecture, Manual, And Visual Baselines

**Files:**
- Modify: `docs/architecture.md:317-332`
- Modify: `site/docs/settings.md:1-36`
- Modify: `ui/tests/visual/storybook.visual.spec.ts-snapshots/settings-settingstab--default.png`
- Modify: `ui/tests/visual/storybook.visual.spec.ts-snapshots/app-appshell--settings-tab.png`
- Modify: `site/docs/assets/screenshots/settings.png`

**Interfaces:**
- Consumes: final behavior and exact UI labels from Tasks 1 through 5.
- Produces: revision-matched internal architecture, public operator guidance, and screenshots.

- [ ] **Step 1: Update architecture behavior**

Revise the Scene-Owned Repeated Recall section to state:

```markdown
The application-wide same-scene recall threshold controls only suppression of repeated identical scene observations and defaults to 500 ms. The 25 ms settle delay, connection-generation arming window, scene-list-edit suppression, and fresh-state timeout remain independent.

After normal recall validation, enabled same-scene finishing rewrites active targets owned by the exact scene for completion after readiness. When finishing is disabled, matching target keys are replaced with full-duration timelines from their current interpolated or live values, using the same overlap path as a different-scene recall. Both paths reset and obey the connection-wide two-ping readiness barrier.
```

Keep the exact identity, generation, lockout, manual override, disconnect, and timeout safety requirements intact.

- [ ] **Step 2: Document both active settings for operators**

Add an Active in v2 section before Extensive diagnostics:

```markdown
### Same-scene recall

**Same scene recall finishing** is on by default. When it is on, recalling an app-managed scene again while that exact scene still owns active fade targets completes those targets after LV1's post-recall readiness check. When it is off, matching fades continue from their current values over the full configured scene duration instead of completing immediately.

**Same scene recall threshold** defaults to `500 ms` and accepts values from `0 ms` through `5000 ms` in `100 ms` steps. Repeated identical LV1 scene notifications below the threshold are ignored. A notification at the threshold is eligible for normal recall validation. The threshold does not change connection arming, scene-list-edit suppression, exact scene matching, SAFE, or disconnect behavior.

Use these settings with **Extensive diagnostics** when investigating unexpected immediate fade completion. If disabling finishing removes the jump, same-scene finishing was selected. Increase the threshold to test whether delayed duplicate LV1 scene notifications are being accepted. Rehearse any changed value before show use.
```

Update the introduction so active same-scene behavior is included, and do not list either setting under Not active in v2.

Keep scene copy/paste guidance in `site/docs/scenes.md` unchanged; it is a regression constraint, not part of the Settings documentation change.

- [ ] **Step 3: Prove the current visual baselines detect the intentional UI change**

Run: `make visual-test`

Expected: FAIL only for `settings-settingstab--default.png` and `app-appshell--settings-tab.png`. The merged clipboard baselines `scenes-selected-scene-selectedsceneactions--clipboard-available.png`, `--clipboard-unavailable.png`, and `--unlinked-destination.png` must remain unchanged. Investigate any unrelated difference before updating snapshots.

- [ ] **Step 4: Regenerate Docker-compatible visual baselines**

Run: `make visual-update`

Expected: Playwright updates the affected Settings screenshots using the CI-compatible Docker image.

Run `git diff --name-only` immediately afterward and confirm only the two Settings snapshots changed. Do not retain regenerated scene clipboard baselines or unrelated screenshots.

- [ ] **Step 5: Re-run visual tests**

Run: `make visual-test`

Expected: all Storybook visual comparisons pass.

- [ ] **Step 6: Copy the stable Settings screenshot into the public manual**

Run:

```bash
cp "ui/tests/visual/storybook.visual.spec.ts-snapshots/settings-settingstab--default.png" "site/docs/assets/screenshots/settings.png"
```

Expected: `site/docs/assets/screenshots/settings.png` matches the updated default Settings story.

- [ ] **Step 7: Build the public documentation strictly**

Run: `make docs-build`

Expected: `zensical build --clean --strict --config-file site/zensical.toml` succeeds with valid links and image references.

- [ ] **Step 8: Commit docs and visual baselines**

```bash
git add docs/architecture.md site/docs/settings.md site/docs/assets/screenshots/settings.png \
  ui/tests/visual/storybook.visual.spec.ts-snapshots/settings-settingstab--default.png \
  ui/tests/visual/storybook.visual.spec.ts-snapshots/app-appshell--settings-tab.png
git commit -m "docs: explain same-scene recall settings"
```

---

### Task 7: Complete Verification And Hardware Handoff

**Files:**
- Verify only; modify implementation files only to fix failures attributable to this feature.

**Interfaces:**
- Consumes: all implementation, tests, docs, and snapshots from Tasks 1 through 6.
- Produces: CI-style verification evidence and explicit hardware-testing status.

- [ ] **Step 1: Run the standard non-visual verification suite**

Run: `make check`

Expected: Rust formatting, clippy, nextest, build, frontend formatting, lint, typecheck, build, Vitest, and Storybook tests all pass.

- [ ] **Step 2: Re-run Docker visual verification**

Run: `make visual-test`

Expected: all visual regression tests pass against committed baselines.

- [ ] **Step 3: Compile the standalone LV1 probe workspace**

Run: `cargo check --manifest-path src-tauri/dev-tools/Cargo.toml --all-targets`

Expected: the standalone `lv1-probe` crate compiles with no missing fade command fields.

- [ ] **Step 4: Re-run strict documentation build**

Run: `make docs-build`

Expected: the documentation site builds cleanly with strict validation.

- [ ] **Step 5: Inspect repository state and final diff**

Run:

```bash
git status --short
git diff --check
git log --oneline -10
```

Expected: no uncommitted implementation changes, no whitespace errors, and focused feature commits only. Do not alter unrelated user or agent changes.

- [ ] **Step 6: Run hardware smoke only when an LV1-compatible target is available**

Run: `make smoke`

Then read: `logs/debug-smoke-report.txt`

Expected when hardware is available: the report states the full suite passed, including `same-scene-finish: PASS` and `same-scene-override: PASS`. A successful shell exit without a passing report is not sufficient.

After smoke, use the normal application Settings UI with Extensive diagnostics enabled to exercise this diagnostic matrix against naturally observed hardware notifications:

| Finishing | Threshold | Expected accepted repeat behavior |
| --- | ---: | --- |
| On | 500 ms | Exact-scene active targets finish after two readiness pings. |
| Off | 500 ms | Matching timelines continue from current values for the full duration. |
| On | Above observed duplicate delay | The delayed duplicate is suppressed; no finishing event occurs. |
| Off | Above observed duplicate delay | The delayed duplicate is suppressed; no override event occurs. |

Use the extensive diagnostic file to confirm that the expected repeated `/Notify/CurSceneIndex` and `/Notify/Scene/Name` pair actually arrived and was followed by `scene_recall_skipped` rather than `fade_same_scene_finishing` or `fade_same_scene_overriding` during each manual threshold trial. Without those observations, natural fade completion alone does not prove threshold suppression. If hardware is unavailable, record that smoke and manual diagnosis were not run; do not claim LV1 duplicate timing is verified.

- [ ] **Step 7: Request final code review**

Review the complete diff against `docs/superpowers/specs/2026-07-18-same-scene-recall-settings-design.md`, with findings prioritized around:

- Any path that can finish or write targets without full recall validation.
- Settings event versus fade command ordering.
- Current-value replacement accidentally rewinding or sending exact targets early.
- Readiness, generation, lockout, manual override, abort, disconnect, or timeout regressions.
- Missing exact-boundary or threshold-zero coverage.

Expected: no unresolved safety-critical findings before integration.
