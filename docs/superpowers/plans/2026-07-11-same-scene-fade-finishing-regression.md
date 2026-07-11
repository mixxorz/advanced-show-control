# Same-Scene Fade Finishing Regression Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore exact-scene ownership so a repeated validated scene recall finishes only that scene's active targets at exact stored values after the post-recall ping barrier releases.

**Architecture:** `ActiveTarget` retains its `FadeSceneIdentity` and exposes a pure instant-completion timeline rewrite. `EngineState` applies that rewrite only to targets owned by an exact scene identity, while `FadeEngine` keeps fresh-state and generation checks before resetting the existing readiness barrier. The normal scheduler remains the only final-write and completion path.

**Tech Stack:** Rust, Tokio actors and paused time, `AppEventBus`, `tracing`, Cargo nextest, React/TypeScript debug smoke runner, Tauri commands, LV1 hardware smoke app.

## Global Constraints

- Treat this as restoration of behavior introduced by commit `376642f` and removed by commit `7ec29ec`; do not broaden scene identity matching.
- Exact scene identity remains LV1 scene index plus scene name.
- Blocked, skipped, disabled, stale, unsafe, or unavailable recalls must not rewrite active targets or reset readiness.
- Same-scene final writes must use the existing scheduler after two qualifying same-generation pings; do not add a direct or deferred write path.
- Targets owned by other scenes may pause behind the connection-wide barrier but must not be finished, replaced, or removed.
- Different-scene recalls continue to replace only matching `FadeTargetKey` values.
- Manual override, Abort All, disconnect, generation change, readiness timeout, and actor shutdown must prevent stale final writes.
- Zero-duration recall behavior remains unchanged.
- Use pure unit tests for timeline/state transitions and actor tests through mailboxes, `AppEventBus`, fake LV1 commands, and tracing capture.
- Use production Tauri commands for the smoke workflow; debug-only commands are limited to deterministic setup and live observation.
- Run `make smoke` and inspect `logs/debug-smoke-report.txt`; shell exit alone is not proof of success.

---

## File Map

- `src-tauri/src/fade/tick.rs`: retain exact scene ownership on each active target and provide the pure instant-completion rewrite.
- `src-tauri/src/fade/state.rs`: find and rewrite all targets owned by one exact scene without touching other owners.
- `src-tauri/src/fade/actor.rs`: select same-scene finishing after fresh-state/generation validation, reset readiness, emit one operational log, and test observable writes and cancellation behavior.
- `ui/src/debug/main.tsx`: add the live `same-scene-finish` smoke workflow.
- `docs/architecture.md`: document active-target scene ownership and readiness-gated repeated-recall finishing.

### Task 1: Restore Active Target Scene Ownership

**Files:**
- Modify: `src-tauri/src/fade/tick.rs:5-178`
- Modify: `src-tauri/src/fade/state.rs:157-182`
- Modify: `src-tauri/src/fade/actor.rs` test and production `ActiveTargetInit` literals

**Interfaces:**
- Consumes: existing `FadeSceneIdentity` from `crate::fade::types`.
- Produces: `ActiveTarget::scene: FadeSceneIdentity`.
- Produces: `ActiveTargetInit::scene: FadeSceneIdentity`.
- Produces: `ActiveTarget::finish_on_next_tick(&mut self)`.

- [ ] **Step 1: Add failing pure ownership and rewrite tests**

Import `FadeSceneIdentity` in `fade/tick.rs`, add a `scene(index, name)` test helper, pass it through `make_channel`, and add:

```rust
#[test]
fn instant_finish_preserves_target_ownership_and_exact_value() {
    let mut target = make_channel(-20.0, -10.0, 4_000);
    let owner = target.scene.clone();
    let key = target.key;
    let generation = target.expected_generation;

    target.finish_on_next_tick();

    assert!(target.is_done(Instant::now()));
    assert_eq!(target.exact_final_send(), -10.0);
    assert_eq!(target.scene, owner);
    assert_eq!(target.key, key);
    assert_eq!(target.expected_generation, generation);
}

#[test]
fn instant_finish_remains_done_after_paused_target_resumes() {
    let now = Instant::now();
    let mut target = make_channel(-20.0, -10.0, 4_000);
    target.pause(now);

    target.finish_on_next_tick();
    target.resume(now + Duration::from_secs(2));

    assert!(target.is_done(now + Duration::from_secs(2)));
    assert_eq!(target.exact_final_send(), -10.0);
}
```

Initialize the test helper with an exact owner:

```rust
scene: FadeSceneIdentity {
    index: 17,
    name: "Verse".to_string(),
},
expected_generation: Some(4),
```

- [ ] **Step 2: Run the pure tests and verify RED**

Run: `cargo nextest run -p advanced-show-control 'instant_finish'`

Expected: compilation fails because `ActiveTarget` has no `scene` field and no `finish_on_next_tick` method.

- [ ] **Step 3: Add ownership and minimal instant-completion behavior**

Update imports and both target structs:

```rust
use crate::fade::types::{FadeParameter, FadeSceneIdentity, FadeTargetKey};

pub(crate) struct ActiveTarget {
    pub(crate) scene: FadeSceneIdentity,
    // existing fields remain unchanged
}

pub(crate) struct ActiveTargetInit {
    pub(crate) scene: FadeSceneIdentity,
    // existing fields remain unchanged
}
```

Copy ownership in `ActiveTarget::new` and add the pure rewrite:

```rust
scene: init.scene,
```

```rust
pub(crate) fn finish_on_next_tick(&mut self) {
    self.duration = Duration::ZERO;
}
```

Do not send writes, mutate `target_value`, clear `paused_since`, or alter `expected_generation` in this method.

- [ ] **Step 4: Update every `ActiveTargetInit` literal**

For production creation in `handle_recall_scene_fade`, use:

```rust
scene: config.scene.clone(),
```

For test literals, use the scene identity already supplied to that test's `fade_config`, or a stable fixture identity when the test does not care about ownership:

```rust
scene: FadeSceneIdentity {
    index: 17,
    name: "Verse".to_string(),
},
```

Update imports in `fade/state.rs`, `fade/actor.rs` tests, and any compiler-identified target literal without changing test behavior.

- [ ] **Step 5: Run target and state tests and verify GREEN**

Run: `cargo nextest run -p advanced-show-control 'fade::tick::tests|fade::state::tests'`

Expected: all target interpolation, override, pause, ownership, instant-finish, and readiness state tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/fade/tick.rs src-tauri/src/fade/state.rs src-tauri/src/fade/actor.rs
git commit -m "fix: restore fade target scene ownership"
```

### Task 2: Rewrite One Scene's Timelines in Engine State

**Files:**
- Modify: `src-tauri/src/fade/state.rs:35-155`
- Test: `src-tauri/src/fade/state.rs:157-290`

**Interfaces:**
- Consumes: `ActiveTarget::scene` and `ActiveTarget::finish_on_next_tick()` from Task 1.
- Produces: `EngineState::finish_scene_on_next_tick(&mut self, scene: &FadeSceneIdentity) -> usize`.

- [ ] **Step 1: Write failing pure scene-scoping tests**

Change the state test helper to accept scene, channel, and target value, then add:

```rust
#[test]
fn finish_scene_rewrites_only_exact_scene_owner() {
    let now = Instant::now();
    let scene_a = FadeSceneIdentity { index: 17, name: "Verse".to_string() };
    let same_index_wrong_name = FadeSceneIdentity { index: 17, name: "Verse Copy".to_string() };
    let scene_b = FadeSceneIdentity { index: 18, name: "Chorus".to_string() };
    let mut state = EngineState::new(AppEventBus::default(), 4);
    state.channels.push(active_target(now, scene_a.clone(), 1, -10.0));
    state.channels.push(active_target(now, scene_a.clone(), 2, -12.0));
    state.channels.push(active_target(now, same_index_wrong_name, 3, -14.0));
    state.channels.push(active_target(now, scene_b, 4, -16.0));

    assert_eq!(state.finish_scene_on_next_tick(&scene_a), 2);
    assert!(state.channels[0].is_done(now));
    assert!(state.channels[1].is_done(now));
    assert!(!state.channels[2].is_done(now));
    assert!(!state.channels[3].is_done(now));
    assert_eq!(state.channels[0].target_value, -10.0);
    assert_eq!(state.channels[1].target_value, -12.0);
}

#[test]
fn finish_scene_returns_zero_without_mutating_unowned_targets() {
    let now = Instant::now();
    let mut state = EngineState::new(AppEventBus::default(), 4);
    state.channels.push(active_target(
        now,
        FadeSceneIdentity { index: 18, name: "Chorus".to_string() },
        1,
        -10.0,
    ));

    let count = state.finish_scene_on_next_tick(&FadeSceneIdentity {
        index: 17,
        name: "Verse".to_string(),
    });

    assert_eq!(count, 0);
    assert!(!state.channels[0].is_done(now));
}
```

- [ ] **Step 2: Run state tests and verify RED**

Run: `cargo nextest run -p advanced-show-control 'finish_scene'`

Expected: compilation fails because `EngineState::finish_scene_on_next_tick` does not exist.

- [ ] **Step 3: Implement the scene-scoped state transition**

Import `FadeSceneIdentity` and add:

```rust
pub(super) fn finish_scene_on_next_tick(&mut self, scene: &FadeSceneIdentity) -> usize {
    let mut count = 0;
    for target in &mut self.channels {
        if &target.scene == scene {
            target.finish_on_next_tick();
            count += 1;
        }
    }
    count
}
```

Do not remove targets, publish events, change readiness, or inspect incoming parameter keys here.

- [ ] **Step 4: Run state tests and verify GREEN**

Run: `cargo nextest run -p advanced-show-control 'fade::state::tests'`

Expected: all state tests pass, including exact index-and-name scoping and unchanged readiness behavior.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/fade/state.rs
git commit -m "fix: rewrite exact scene fade timelines"
```

### Task 3: Route Repeated Recalls Through Readiness-Gated Completion

**Files:**
- Modify: `src-tauri/src/fade/actor.rs:75-141`
- Modify: `src-tauri/src/fade/actor.rs:364-503`
- Test: `src-tauri/src/fade/actor.rs`

**Interfaces:**
- Consumes: `EngineState::finish_scene_on_next_tick(&FadeSceneIdentity) -> usize` from Task 2.
- Produces: private `RecallSceneFadeOutcome::{Started, Finishing { target_count: usize }}`.
- Preserves: `FadeCommand::RecallSceneFade`, `FadeEvent`, and the frontend projection contract.

- [ ] **Step 1: Strengthen the repeated-scene actor regression test**

Add these helpers beside `assert_no_write`:

```rust
fn publish_ping(event_bus: &AppEventBus, generation: u64, sequence: u64) {
    event_bus.publish(AppEvent::Lv1 {
        generation,
        event: Lv1Event::PingReceived { sequence },
    });
}

async fn next_write_batch(
    write_rx: &mut tokio::sync::mpsc::Receiver<Vec<Lv1ParameterWrite>>,
) -> Vec<Lv1ParameterWrite> {
    tokio::time::timeout(Duration::from_secs(1), write_rx.recv())
        .await
        .expect("fade write should arrive")
        .expect("LV1 write channel should remain open")
}
```

Replace the misleading `recalling_same_scene_finishes_only_that_scene_channels` assertion or add a focused sibling using `spawn_runtime_for_ping_gate_test`. Supply snapshots with increasing ping boundaries for Scene A, Scene B, and repeated Scene A. Define `let scene_b_exact_target = 0.0;`, start Scene A on channel 1 and Scene B toward that target on channel 2, release their prior readiness gates, repeat Scene A, and assert:

```rust
assert_no_write(&mut write_rx).await;
publish_ping(&event_bus, 7, repeated_recall_boundary + 1);
assert_no_write(&mut write_rx).await;
publish_ping(&event_bus, 7, repeated_recall_boundary + 2);

let writes = next_write_batch(&mut write_rx).await;
assert!(writes.contains(&Lv1ParameterWrite {
    group: 0,
    channel: 1,
    parameter: Lv1WriteParameter::FaderDb,
    value: -10.0,
}));
assert!(!writes.iter().any(|write| {
    write.channel == 2 && (write.value - scene_b_exact_target).abs() < 1e-10
}));

tokio::time::advance(Duration::from_millis(200)).await;
let resumed_writes = next_write_batch(&mut write_rx).await;
assert!(resumed_writes.iter().any(|write| write.channel == 2));
```

Observe `ChannelCompleted` for Scene A's target. Assert that `FadeCompleted` is not emitted while Scene B remains active. Use paused Tokio time and existing ping/write helpers rather than sleeping in real time.

- [ ] **Step 2: Add failing parameter-family and manual-override actor cases**

Add one table-driven repeated-scene test with fader, pan, balance, and width targets. After two pings, assert exact `Lv1WriteParameter` and target values for all four.

Add a gated override case using `start_fade_for_generation`, direct event publication, and the helpers defined in Step 1:

```rust
start_fade_for_generation(&engine, repeated_scene_a, Some(7))
    .await
    .expect("repeated Scene A recall should validate");
event_bus.publish(AppEvent::Lv1 {
    generation: 7,
    event: Lv1Event::FaderChanged {
        group: 0,
        channel: 1,
        gain_db: manual_value,
    },
});
publish_ping(&event_bus, 7, boundary + 1);
publish_ping(&event_bus, 7, boundary + 2);
tokio::time::advance(Duration::from_millis(200)).await;
while let Ok(writes) = write_rx.try_recv() {
    assert!(!writes.iter().any(|write| {
        write.group == 0
            && write.channel == 1
            && write.parameter == Lv1WriteParameter::FaderDb
    }));
}
```

Retain an unrelated scene target and assert it resumes after release. Existing timeout, disconnect, Abort All, and generation-change tests continue to prove global cancellation; include a repeated-scene target in one cancellation case so the regression path is covered.

- [ ] **Step 3: Run focused actor tests and verify RED**

Run: `cargo nextest run -p advanced-show-control 'same_scene|repeated_scene'`

Expected: tests fail because the repeated recall creates replacement timed targets and does not finish all Scene A-owned targets.

- [ ] **Step 4: Add an explicit command outcome**

Near `handle_recall_scene_fade`, define:

```rust
enum RecallSceneFadeOutcome {
    Started,
    Finishing { target_count: usize },
}
```

Change the handler return type to:

```rust
) -> Result<RecallSceneFadeOutcome, AppCommandError>
```

Return `Started` for empty-target, zero-duration, and normal new/overlapping recall paths so existing command semantics remain unchanged.

- [ ] **Step 5: Select same-scene finishing after fresh validation**

After the zero-duration branch and before replacement target creation, run:

```rust
let finishing_target_count = state.finish_scene_on_next_tick(&config.scene);
let outcome = if finishing_target_count > 0 {
    RecallSceneFadeOutcome::Finishing {
        target_count: finishing_target_count,
    }
} else {
    for target in &config.targets {
        let active_start_value = state
            .channels
            .iter()
            .find(|channel| channel.key == target.key())
            .map(|channel| {
                if channel.is_done(now) {
                    channel.target_value
                } else {
                    channel.value_at(now)
                }
            });
        let snapshot_start_value = snapshot
            .channels
            .iter()
            .find(|channel| channel.group == target.group && channel.channel == target.channel)
            .and_then(|channel| live_value_for_snapshot(channel, target));
        let start_value = if state.is_waiting_for_readiness() {
            snapshot_start_value.or(active_start_value)
        } else {
            active_start_value.or(snapshot_start_value)
        }
        .unwrap_or(target.target);

        state.channels.retain(|channel| channel.key != target.key());
        state.channels.push(ActiveTarget::new(ActiveTargetInit {
            scene: config.scene.clone(),
            key: target.key(),
            group: target.group,
            channel: target.channel,
            start_value,
            target_value: target.target,
            curve: config.curve,
            duration,
            started_at: now,
            expected_generation,
        }));
    }
    RecallSceneFadeOutcome::Started
};
```

Keep the existing common `start_or_reset_readiness` call after this branch. It must use the fresh snapshot's `ping_sequence`, exact `config.scene`, current `now`, and expected generation for both outcomes. Return `Ok(outcome)` after logging the readiness barrier at `DEBUG`.

- [ ] **Step 6: Emit one operational log without duplicating the fact**

In `run_engine`, match the successful outcome. Preserve `FadeEvent::FadeStarted` and running projection state, but choose one `INFO` log:

```rust
match outcome {
    RecallSceneFadeOutcome::Started => tracing::info!(
        event = "fade_started",
        scene_index,
        scene_name = %scene_name,
        duration_ms,
        target_count,
        "Fade started for {}: {} ({} targets, {} ms)",
        scene_index,
        scene_name,
        target_count,
        duration_ms
    ),
    RecallSceneFadeOutcome::Finishing { target_count } => tracing::info!(
        event = "fade_same_scene_finishing",
        scene_index,
        scene_name = %scene_name,
        target_count,
        "Repeated scene recall is finishing active fade targets for {}: {} ({} targets)",
        scene_index,
        scene_name,
        target_count
    ),
}
```

Do not add the same `INFO` event in `scenes` or emit a second `fade_started` log for the finishing outcome.

- [ ] **Step 7: Add a tracing assertion for the finishing fact**

Extend `CapturedWarnEvent` with `target_count: Option<String>`, record it in the visitor alongside `scene_index`, and use `CapturedWarnEvents` with `tracing::subscriber::with_default` to assert one event with:

```rust
event == "fade_same_scene_finishing"
level == tracing::Level::INFO
message == "Repeated scene recall is finishing active fade targets for 17: Verse (2 targets)"
```

Assert diagnostic fields `scene_index = 17`, `scene_name = "Verse"`, and `target_count = 2`. Assert no second `fade_same_scene_finishing` or `fade_started` `INFO` for the repeated command.

- [ ] **Step 8: Run fade and scenes actor tests and verify GREEN**

Run: `cargo nextest run -p advanced-show-control 'fade|scene_recall'`

Expected: all fade target, readiness, overlap, override, cancellation, same-scene, and scenes validation tests pass. Existing blocked/skipped/disabled/stale scenes tests prove that invalid recalls never reach the fade actor.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/fade/actor.rs
git commit -m "fix: finish repeated scene fades safely"
```

### Task 4: Add Live Same-Scene Smoke Coverage

**Files:**
- Modify: `ui/src/debug/main.tsx:7-170`

**Interfaces:**
- Consumes: production `recall_scene` and `set_scene_duration_ms` Tauri commands.
- Consumes: debug-only `debug_smoke_set_channel_gain` and `debug_smoke_get_channel_gain` for deterministic setup and observation.
- Produces: authoritative report line `TEST same-scene-finish PASS ...`.

- [ ] **Step 1: Register the smoke case**

Add `"same-scene-finish"` after `"fade-completes"` in the `tests` array. Add fixed timing values beside the existing smoke constants:

```ts
const sameSceneDurationMs = 6_000;
const sameSceneMovementThresholdDb = 2;
const sameSceneFinishTimeoutMs = 3_000;
```

The 3-second completion bound is shorter than the 6-second configured duration but allows two approximately 200 ms LV1 pings plus host scheduling tolerance.

- [ ] **Step 2: Add a deadline-aware wait helper**

Change `waitFor` to accept an optional timeout while preserving all current callers:

```ts
async function waitFor<T>(
  check: () => T | Promise<T>,
  labelText: string,
  waitTimeoutMs = timeoutMs,
): Promise<NonNullable<T>> {
  const deadline = Date.now() + waitTimeoutMs;
  while (Date.now() < deadline) {
    const value = await check();
    if (value) return value as NonNullable<T>;
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${labelText}`);
}
```

- [ ] **Step 3: Add the repeated-recall hardware workflow**

After `fade-completes`, add:

```ts
await test("same-scene-finish", async () => {
  try {
    await reset(sceneA, targetA);
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneB,
      durationMs: sameSceneDurationMs,
    });
    await invoke("recall_scene", { internalSceneId: sceneB });
    await waitFor(
      async () => {
        const liveGain = await gain();
        return liveGain >= targetA + sameSceneMovementThresholdDb &&
          liveGain < targetB - tolerance;
      },
      "same-scene fade movement before target",
    );

    const repeatedAt = Date.now();
    await invoke("recall_scene", { internalSceneId: sceneB });
    await waitFor(
      async () => Math.abs((await gain()) - targetB) <= tolerance,
      "same-scene exact finish",
      sameSceneFinishTimeoutMs,
    );
    await waitFor(
      () => state?.fadeState === "idle",
      "same-scene projected fade completion",
      sameSceneFinishTimeoutMs,
    );
    if (Date.now() - repeatedAt >= sameSceneDurationMs) {
      throw new Error("same-scene recall restarted the full fade duration");
    }
  } finally {
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneB,
      durationMs: 1_000,
    });
  }
});
```

- [ ] **Step 4: Run frontend static verification**

Run: `npm --prefix ui run format:check`

Expected: Prettier exits successfully.

Run: `npm --prefix ui run lint`

Expected: ESLint exits successfully.

Run: `npm --prefix ui run typecheck`

Expected: TypeScript exits successfully.

- [ ] **Step 5: Run the hardware smoke suite**

Run: `make smoke`

Expected: the debug Tauri app completes against an LV1-compatible target.

- [ ] **Step 6: Inspect the authoritative smoke report**

Read: `logs/debug-smoke-report.txt`

Require both lines:

```text
TEST same-scene-finish PASS
SUITE PASS
```

The test line may include elapsed milliseconds after `PASS`. If hardware is unavailable or either line is absent, record the smoke as blocked or failed; do not infer success from the shell exit.

- [ ] **Step 7: Commit**

```bash
git add ui/src/debug/main.tsx
git commit -m "test: smoke repeated scene finishing"
```

### Task 5: Document and Verify the Restored Contract

**Files:**
- Modify: `docs/architecture.md:293-310`
- Verify: full workspace and smoke report

**Interfaces:**
- Documents: exact active-target scene ownership, same-scene timeline rewriting, and readiness-gated exact completion.

- [ ] **Step 1: Update fade architecture responsibilities**

Add a focused subsection under `fade`:

```markdown
#### Scene-Owned Repeated Recall

Every timed active target retains the exact LV1 scene index and scene name that created it. After normal recall validation, recalling that exact scene while it owns active targets rewrites only those targets for completion on the next eligible scheduler tick. The repeated recall resets the connection-wide post-recall readiness barrier; no final parameter write occurs until two qualifying same-generation pings release the barrier. The normal scheduler then sends each stored target value exactly and removes the completed target. Targets owned by other scenes remain active and resume after the shared readiness pause.
```

- [ ] **Step 2: Run formatting**

Run: `make fmt`

Expected: Rust and frontend formatting complete successfully with no remaining differences.

- [ ] **Step 3: Run targeted Rust verification**

Run: `cargo nextest run -p advanced-show-control 'fade|scene_recall'`

Expected: all selected tests pass.

- [ ] **Step 4: Run broad CI-style verification**

Run: `make check`

Expected: Rust formatting, Clippy, tests, and build plus frontend formatting, lint, typecheck, tests, Storybook tests, and build all pass.

- [ ] **Step 5: Re-run smoke after final formatting and inspect its report**

Run: `make smoke`

Read: `logs/debug-smoke-report.txt`

Expected report evidence:

```text
TEST same-scene-finish PASS
SUITE PASS
```

- [ ] **Step 6: Inspect final safety invariants and diff**

Run: `git status --short` and `git diff --check`.

Confirm from the diff and tests that:

- target ownership is exact index plus name;
- same-scene selection happens after fresh LV1 state and generation checks;
- zero-duration recalls retain their existing earlier branch;
- same-scene targets write only after readiness release;
- unrelated scene targets remain active;
- override and lifecycle cancellation prevent later final writes;
- final values use the normal exact completion path;
- only one user-facing finishing log is emitted.

- [ ] **Step 7: Commit documentation**

```bash
git add docs/architecture.md
git commit -m "docs: describe repeated scene fade finishing"
```
