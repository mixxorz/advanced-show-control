# Scene-Owned Overlapping Fades Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow validated scene fades to overlap per fader, finish only same-scene active faders on repeat recall, remove global finish-now, and always send exact stored target dB at channel completion.

**Architecture:** `FadeEngine` owns overlap decisions. Scene recall automation sends one validated scene-recall fade command; the engine atomically either finishes channels owned by that exact scene or starts/replaces only the incoming scene's scoped faders. `Abort All`, disconnect, exact scene validation, generation guards, and manual override safety remain intact.

**Tech Stack:** Rust, Tokio actor loops and channels, Tauri commands, React/TypeScript UI, existing `AppEventBus` and `AppCommandBus`.

---

## File Structure

- `src/fade/types.rs`: add `FadeSceneIdentity`, attach it to `FadeConfig`, replace `StartFade` with `RecallSceneFade`, remove `FinishNow`, add `ChannelCompleted` if needed by app state.
- `src/fade/tick.rs`: add scene ownership to `ActiveChannel`; add `exact_final_send()` so final sends bypass interpolation and min-delta suppression.
- `src/fade/engine.rs`: keep active channels keyed by `(group, channel)`; implement same-scene finish vs overlapping start atomically.
- `tests/fade_engine.rs`: cover overlap, replacement, same-scene finish, exact final target send, abort/disconnect, and manual override.
- `src/runtime/commands.rs`: route the scene-recall fade command and remove `finish_fade_now`.
- `src-tauri/src/scene_recall_fader.rs`: remove pre-start abort and call the new atomic fade command.
- `src-tauri/src/commands.rs`, `src-tauri/src/main.rs`: remove Tauri `finish_fade_now`.
- `ui/src/App.tsx`, `ui/src/components/Header.tsx`: remove the global Finish Now button and prop.
- `PROJECT.md`, `PHASES.md`, `docs/architecture.md`, `IDEAS.md`: update current behavior and remove implemented future ideas.

## Task 1: Add Scene Identity To Fade Types

**Files:**
- Modify: `src/fade/types.rs`
- Modify: `src/fade/tick.rs`
- Test: `src/fade/tick.rs`

- [ ] **Step 1: Write the failing tick ownership test**

Add this to `src/fade/tick.rs` inside `mod tests`:

```rust
#[test]
fn active_channel_records_scene_identity() {
    let scene = crate::fade::types::FadeSceneIdentity {
        index: 7,
        name: "Bridge".to_string(),
    };
    let ch = ActiveChannel::new(
        scene.clone(),
        0,
        3,
        -20.0,
        -10.0,
        FadeCurve::Linear,
        Duration::from_millis(4000),
        Instant::now(),
    );
    assert_eq!(ch.scene, scene);
    assert_eq!(ch.group, 0);
    assert_eq!(ch.channel, 3);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p lv1-scene-fade-utility fade::tick::tests::active_channel_records_scene_identity`

Expected: FAIL because `FadeSceneIdentity` and the new `ActiveChannel::new` signature do not exist.

- [ ] **Step 3: Update fade types**

In `src/fade/types.rs`, add:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FadeSceneIdentity {
    pub index: i32,
    pub name: String,
}
```

Change `FadeConfig` to:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct FadeConfig {
    pub scene: FadeSceneIdentity,
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}
```

Change `FadeCommand` to:

```rust
#[derive(Debug)]
pub enum FadeCommand {
    RecallSceneFade {
        config: FadeConfig,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    AbortAll {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
}
```

Add to `FadeEvent`:

```rust
ChannelCompleted { group: i32, channel: i32 },
```

- [ ] **Step 4: Update active channel type**

In `src/fade/tick.rs`, import `FadeSceneIdentity`, add `pub(crate) scene: FadeSceneIdentity` to `ActiveChannel`, change `ActiveChannel::new` to take `scene` first, and add:

```rust
pub(crate) fn exact_final_send(&mut self) -> f64 {
    self.expected_db = self.target_db;
    self.target_db
}
```

Update `make_channel` in tick tests to pass:

```rust
FadeSceneIdentity {
    index: 1,
    name: "Intro".to_string(),
},
```

- [ ] **Step 5: Update compile errors in literals**

For every `FadeConfig { ... }` literal, add:

```rust
scene: FadeSceneIdentity {
    index: 1,
    name: "Intro".to_string(),
},
```

Where code matches `FadeCommand::StartFade`, rename it to `FadeCommand::RecallSceneFade`. Remove handling for `FadeCommand::FinishNow`.

- [ ] **Step 6: Run targeted tests**

Run: `cargo test -p lv1-scene-fade-utility fade::tick`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/fade/types.rs src/fade/tick.rs src/fade/engine.rs src/runtime/commands.rs src-tauri/src/scene_recall_fader.rs tests/fade_engine.rs
git commit -m "feat: add scene identity to fades"
```

## Task 2: Implement Overlap And Exact Final Sends

**Files:**
- Modify: `src/fade/engine.rs`
- Test: `tests/fade_engine.rs`

- [ ] **Step 1: Add test helpers**

In `tests/fade_engine.rs`, update the import and add helpers near the top:

```rust
use lv1_scene_fade_utility::fade::types::{FadeConfig, FadeEvent, FadeSceneIdentity, FadeTarget};

fn scene(index: i32, name: &str) -> FadeSceneIdentity {
    FadeSceneIdentity { index, name: name.to_string() }
}

fn fade_config(scene: FadeSceneIdentity, targets: Vec<FadeTarget>, duration_ms: u64) -> FadeConfig {
    FadeConfig { scene, targets, duration_ms, curve: FadeCurve::Linear }
}
```

- [ ] **Step 2: Write failing overlap test**

Add `different_scene_fade_does_not_cancel_unrelated_channel` to `tests/fade_engine.rs`. It should start scene `1: Intro` on channel `0` for `30_000ms`, then start scene `2: Verse` on channel `1` for `500ms`, then assert no global `FadeCompleted` is emitted within `800ms` because Intro is still active.

Use the existing `spawn_runtime_for_test`, `wait_for_app_fade_event`, `TcpListener`, `spawn_actor`, and `lv1_frame` patterns already in the file.

- [ ] **Step 3: Run the failing overlap test**

Run: `cargo test -p lv1-scene-fade-utility --test fade_engine different_scene_fade_does_not_cancel_unrelated_channel`

Expected: FAIL because the second start clears all active channels today.

- [ ] **Step 4: Replace clear-all start behavior with per-channel replacement**

In `src/fade/engine.rs`, inside the `RecallSceneFade` start path, remove `state.cancel_all_in_place()` and replace per incoming target:

```rust
state
    .channels
    .retain(|ch| !(ch.group == target.group && ch.channel == target.channel));
state.channels.push(ActiveChannel::new(
    config.scene.clone(),
    target.group,
    target.channel,
    start_db,
    target.target_db,
    config.curve,
    duration,
    now,
));
```

- [ ] **Step 5: Send exact target on natural completion**

In the tick loop, process `is_done(now)` before `next_send(now)`:

```rust
if ch.is_done(now) {
    let target_db = ch.exact_final_send();
    let _ = command_bus.set_gain(ch.group, ch.channel, target_db).await;
    completed_events.push(FadeEvent::ChannelCompleted { group: ch.group, channel: ch.channel });
    done_indices.push(i);
    continue;
}
```

After removals, fan out collected `ChannelCompleted` events. Emit `FadeCompleted` only when `state.channels` is empty.

- [ ] **Step 6: Run overlap test**

Run: `cargo test -p lv1-scene-fade-utility --test fade_engine different_scene_fade_does_not_cancel_unrelated_channel`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/fade/engine.rs tests/fade_engine.rs
git commit -m "feat: overlap scene fades by channel"
```

## Task 3: Implement Same-Scene Repeat Finish

**Files:**
- Modify: `src/fade/engine.rs`
- Test: `tests/fade_engine.rs`

- [ ] **Step 1: Write failing same-scene finish test**

Add `recalling_same_scene_finishes_only_that_scene_channels` to `tests/fade_engine.rs`. It should start Intro channel `0` long, start Verse channel `1` long, then recall Intro again. Assert `ChannelCompleted { group: 0, channel: 0 }` is emitted and no global `FadeCompleted` arrives within `500ms` because Verse remains active.

- [ ] **Step 2: Run the failing test**

Run: `cargo test -p lv1-scene-fade-utility --test fade_engine recalling_same_scene_finishes_only_that_scene_channels`

Expected: FAIL because same-scene recall restarts today.

- [ ] **Step 3: Add scene-active and scene-finish helpers**

In `src/fade/engine.rs`, add to `EngineState`:

```rust
fn has_active_scene(&self, scene: &FadeSceneIdentity) -> bool {
    self.channels.iter().any(|ch| &ch.scene == scene)
}
```

Add an async helper in the actor module:

```rust
async fn finish_scene_channels(
    state: &mut EngineState,
    command_bus: &AppCommandBus,
    scene: &FadeSceneIdentity,
) {
    let mut completed = Vec::new();
    for ch in &mut state.channels {
        if &ch.scene == scene {
            let target_db = ch.exact_final_send();
            let _ = command_bus.set_gain(ch.group, ch.channel, target_db).await;
            completed.push((ch.group, ch.channel));
        }
    }
    state.channels.retain(|ch| &ch.scene != scene);
    for (group, channel) in completed {
        state.fan_out(FadeEvent::ChannelCompleted { group, channel });
    }
}
```

- [ ] **Step 4: Make recall command atomic**

At the start of the `RecallSceneFade` command branch:

```rust
if state.has_active_scene(&config.scene) {
    finish_scene_channels(&mut state, &command_bus, &config.scene).await;
    if !state.is_active() {
        tick_interval = None;
        state.fan_out(FadeEvent::FadeCompleted);
    }
    let _ = reply.send(Ok(()));
    continue;
}
```

- [ ] **Step 5: Run same-scene and overlap tests**

Run: `cargo test -p lv1-scene-fade-utility --test fade_engine different_scene_fade_does_not_cancel_unrelated_channel recalling_same_scene_finishes_only_that_scene_channels`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/fade/engine.rs tests/fade_engine.rs
git commit -m "feat: finish repeated scene recalls"
```

## Task 4: Update Scene Recall Automation And Remove Global Finish Command

**Files:**
- Modify: `src/runtime/commands.rs`
- Modify: `src-tauri/src/scene_recall_fader.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/components/Header.tsx`
- Test: `src-tauri/src/scene_recall_fader.rs`

- [ ] **Step 1: Update failing scene recall tests**

In `src-tauri/src/scene_recall_fader.rs`, change `valid_scene_recall_aborts_existing_fade_then_starts_new_fade` so it expects a single `FadeCommand::RecallSceneFade { config, reply }` and no preceding `AbortAll`. Assert `config.scene.index == 1`, `config.scene.name == "Intro"`, and the existing target/duration/curve values.

In `stale_generation_after_start_log_does_not_start_fade`, update the helper task so it no longer expects `AbortAll`; it should expect no fade command after the generation changes.

- [ ] **Step 2: Run scene recall tests to verify failure**

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_recall_fader`

Expected: FAIL because automation still sends `AbortAll` before start.

- [ ] **Step 3: Route atomic recall command through command bus**

In `src/runtime/commands.rs`, keep the public method name `start_fade` if preferred, but route to `fade.start_fade(config)` where `FadeEngineHandle::start_fade` now sends `RecallSceneFade`. Delete `finish_fade_now` entirely.

- [ ] **Step 4: Remove pre-start abort from scene recall fader**

In `start_scene_recall_fade_with_hook`, delete the log block that says `Previous fade abort requested before auto fade...` and delete the `command_bus.abort_all_fades().await` call. Keep generation checks before logging start and immediately before `command_bus.start_fade(request.fade_config).await`.

When constructing `SceneRecallFadeRequest.fade_config`, include:

```rust
scene: FadeSceneIdentity {
    index: scene.index,
    name: scene.name.clone(),
},
```

- [ ] **Step 5: Remove global finish command from Tauri and UI**

Delete `finish_fade_now` from `src-tauri/src/commands.rs` and from the `generate_handler!` list in `src-tauri/src/main.rs`.

In `ui/src/components/Header.tsx`, remove `onFinishNow` from props and delete the `Finish Now` button.

In `ui/src/App.tsx`, remove the `onFinishNow={() => runVoidCommand("finish_fade_now", ...)}` prop.

- [ ] **Step 6: Run targeted backend and frontend checks**

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_recall_fader`

Expected: PASS.

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src/runtime/commands.rs src-tauri/src/scene_recall_fader.rs src-tauri/src/commands.rs src-tauri/src/main.rs ui/src/App.tsx ui/src/components/Header.tsx
git commit -m "fix: route scene recall fades atomically"
```

## Task 5: Update App State, Docs, And Final Verification

**Files:**
- Modify: `src-tauri/src/app_state/events.rs`
- Modify: `src-tauri/src/app_state/events_tests.rs`
- Modify: `PROJECT.md`
- Modify: `PHASES.md`
- Modify: `docs/architecture.md`
- Modify: `IDEAS.md`

- [ ] **Step 1: Handle `ChannelCompleted` in app state**

In `src-tauri/src/app_state/events.rs`, add a match arm:

```rust
FadeEvent::ChannelCompleted { group, channel } => {
    inner.push_log(
        LogSource::Fade,
        LogSeverity::Info,
        format!("Fade channel completed: group {group}, channel {channel}"),
    );
}
```

Do not set `fade_state` to idle for `ChannelCompleted`; only `FadeCompleted`, `FadeAborted`, and disconnect-derived events should clear global running state.

- [ ] **Step 2: Add app-state event test**

In `src-tauri/src/app_state/events_tests.rs`, add:

```rust
#[tokio::test]
async fn channel_completed_logs_without_clearing_running_state() {
    let state = ShellState::default();
    let started = state.apply_fade_event(&FadeEvent::FadeStarted).await;
    assert_eq!(started.fade_state, AppFadeState::Running);

    let completed = state
        .apply_fade_event(&FadeEvent::ChannelCompleted { group: 0, channel: 2 })
        .await;

    assert_eq!(completed.fade_state, AppFadeState::Running);
    assert!(completed.logs.iter().any(|log| {
        log.message == "Fade channel completed: group 0, channel 2"
    }));
}
```

- [ ] **Step 3: Update docs**

Edit docs with these concrete changes:

- `IDEAS.md`: remove the overlap idea and the stronger final-state guarantee idea, or replace them with a note that retry/verification remains deferred if hardware proves one exact final send is insufficient.
- `PHASES.md`: change Phase 7 overlap policy from cancel previous fade to scene-owned overlapping fades; remove `Finish Now` from MVP lists.
- `PROJECT.md`: change fade engine step 4 to say exact target send is the final channel send; remove global Finish Now from control lists.
- `docs/architecture.md`: update command flow so `SceneRecallFader` starts validated scene recall fades without aborting first; repeat same-scene recall finishes that scene's owned channels.

- [ ] **Step 4: Run targeted checks**

Run: `cargo test -p lv1-scene-fade-utility --test fade_engine`

Expected: PASS.

Run: `cargo test -p lv1-scene-fade-utility-tauri scene_recall_fader`

Expected: PASS.

Run: `cargo test -p lv1-scene-fade-utility-tauri app_state::events_tests`

Expected: PASS.

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 5: Run broad verification**

Run: `cargo test --workspace`

Expected: PASS.

Run: `npm run build`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/app_state/events.rs src-tauri/src/app_state/events_tests.rs PROJECT.md PHASES.md docs/architecture.md IDEAS.md
git commit -m "docs: update fade overlap behavior"
```

## Self-Review Notes

- Spec coverage: overlap, same-scene scoped finish, exact final target send, removed global finish, manual override, abort/disconnect, generation guard preservation, UI removal, and docs updates are covered.
- Placeholder scan: no `TBD`, `TODO`, or unspecified implementation steps remain.
- Type consistency: the plan uses `FadeSceneIdentity`, `FadeConfig.scene`, `FadeCommand::RecallSceneFade`, and `FadeEvent::ChannelCompleted` consistently across tasks.
