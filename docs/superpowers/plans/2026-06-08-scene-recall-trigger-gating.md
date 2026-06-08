# Scene Recall Trigger Gating Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent connect/reconnect scene sync and LV1 duplicate same-scene recall bursts from triggering unintended fade automation.

**Architecture:** Keep `Lv1Actor` as the factual LV1 state source and implement trigger policy inside `SceneRecallFader`. Each scene recall fader generation starts in `Priming`, arms after `2000 ms`, and then applies a `500 ms` minimum delay for same-scene repeat recall events.

**Tech Stack:** Rust, Tokio async tasks, Tauri backend crate tests, existing `AppEventBus`, `AppCommandBus`, `ShellState`, and `FadeEngineHandle` test harnesses.

---

## File Structure

- Modify `src-tauri/src/scene_recall_fader.rs`: add recall trigger gate state and tests in the existing module.
- Do not modify `src/lv1/state.rs`: `Lv1Actor` should continue emitting `SceneChanged` facts.
- Do not modify `src/fade/engine.rs`: same-scene `RecallSceneFade` execution semantics stay unchanged.

---

### Task 1: Add Failing Priming Test

**Files:**
- Modify: `src-tauri/src/scene_recall_fader.rs`

- [ ] **Step 1: Write the failing test**

Add this test inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/scene_recall_fader.rs`, near the other scene recall automation tests:

```rust
#[tokio::test(start_paused = true)]
async fn first_scene_observation_after_connect_primes_without_starting_fade() {
    let event_bus = AppEventBus::default();
    let command_bus = AppCommandBus::new(event_bus.clone());
    let state = ShellState::default();
    let generation = configure_intro_recall(&state).await;
    let (fade_tx, mut fade_rx) = mpsc::channel(4);
    command_bus
        .set_fade(Some(FadeEngineHandle::new(fade_tx)))
        .await;

    let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
    command_bus.set_lv1(Some(lv1)).await;

    let handle =
        spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());
    release_lv1.send(()).unwrap();

    tokio::time::advance(Duration::from_millis(100)).await;
    tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
        .await
        .expect_err("initial scene sync should prime without sending fade commands");

    let snapshot = state.snapshot().await;
    assert!(
        !snapshot
            .logs
            .iter()
            .any(|log| log.message == "Auto fade start requested for scene 1: Intro")
    );

    handle.abort();
    server.await.unwrap();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri first_scene_observation_after_connect_primes_without_starting_fade
```

Expected: FAIL because the current `SceneRecallFader` treats the first `SceneChanged` event as actionable and sends `FadeCommand::RecallSceneFade`.

- [ ] **Step 3: Commit is not allowed yet**

Do not commit after a red test. Continue to Task 2 so the production change is covered by the failing test.

---

### Task 2: Implement Minimal Priming Gate

**Files:**
- Modify: `src-tauri/src/scene_recall_fader.rs`

- [ ] **Step 1: Add trigger gate constants and state**

Near the top of `src-tauri/src/scene_recall_fader.rs`, after the existing imports, add:

```rust
use lv1_scene_fade_utility::lv1::types::SceneState;
use std::time::{Duration, Instant};

const RECALL_ARMING_DELAY: Duration = Duration::from_millis(2_000);
const SAME_SCENE_REPEAT_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, PartialEq, Eq)]
struct RecallSceneIdentity {
    index: i32,
    name: String,
}

impl From<&SceneState> for RecallSceneIdentity {
    fn from(scene: &SceneState) -> Self {
        Self {
            index: scene.index,
            name: scene.name.clone(),
        }
    }
}

#[derive(Debug)]
enum RecallTriggerGate {
    Priming {
        baseline: Option<RecallSceneIdentity>,
        arm_after: Option<Instant>,
    },
    Armed {
        last_identity: RecallSceneIdentity,
        last_identity_at: Instant,
    },
}

impl RecallTriggerGate {
    fn new() -> Self {
        Self::Priming {
            baseline: None,
            arm_after: None,
        }
    }

    fn accept(&mut self, scene: &SceneState, now: Instant) -> bool {
        let identity = RecallSceneIdentity::from(scene);
        match self {
            Self::Priming {
                baseline,
                arm_after,
            } => {
                if let Some(deadline) = *arm_after {
                    if now >= deadline {
                        let last_identity = baseline.clone().unwrap_or_else(|| identity.clone());
                        let last_identity_at = deadline;
                        *self = Self::Armed {
                            last_identity,
                            last_identity_at,
                        };
                        return self.accept(scene, now);
                    }
                }

                *baseline = Some(identity);
                *arm_after = Some(now + RECALL_ARMING_DELAY);
                false
            }
            Self::Armed {
                last_identity,
                last_identity_at,
            } => {
                if *last_identity == identity && now.duration_since(*last_identity_at) < SAME_SCENE_REPEAT_DELAY {
                    return false;
                }

                *last_identity = identity;
                *last_identity_at = now;
                true
            }
        }
    }
}
```

- [ ] **Step 2: Apply the gate before LV1 state fetch**

In `spawn_scene_recall_fader`, create the gate before the loop and check it immediately after the generation check:

```rust
let mut recall_gate = RecallTriggerGate::new();

tokio::spawn(async move {
    loop {
        match events.recv().await {
            Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
                if !state.is_generation_current(generation).await {
                    continue;
                }

                if !recall_gate.accept(&scene, Instant::now()) {
                    continue;
                }

                let snapshot = match command_bus.get_lv1_state().await {
                    // keep existing body unchanged
                };
```

Do not move the existing generation check below the gate.

- [ ] **Step 3: Run priming test to verify it passes**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri first_scene_observation_after_connect_primes_without_starting_fade
```

Expected: PASS.

- [ ] **Step 4: Run existing scene recall tests to find intended breakage**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri scene_recall
```

Expected: Some existing tests may fail because they assumed the first scene observation starts a fade. Do not weaken the priming behavior; update tests in later tasks so explicit post-arming recall events are used for actionable fade cases.

---

### Task 3: Update Existing Start Test for Post-Arming Recall

**Files:**
- Modify: `src-tauri/src/scene_recall_fader.rs`

- [ ] **Step 1: Update the existing valid recall test**

Change `valid_scene_recall_starts_scene_fade_without_global_abort` to use paused time and publish an explicit recall after arming.

Use this function signature:

```rust
#[tokio::test(start_paused = true)]
async fn valid_scene_recall_starts_scene_fade_without_global_abort() {
```

After `release_lv1.send(()).unwrap();`, add:

```rust
tokio::time::advance(RECALL_ARMING_DELAY).await;
event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
```

Leave the existing assertion that the first received fade command is `RecallSceneFade`.

- [ ] **Step 2: Run the updated test**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri valid_scene_recall_starts_scene_fade_without_global_abort
```

Expected: PASS.

- [ ] **Step 3: Commit priming behavior**

Run:

```bash
git add src-tauri/src/scene_recall_fader.rs
git commit -m "fix: prime scene recall automation after connect"
```

---

### Task 4: Add Same-Scene Duplicate Suppression Tests

**Files:**
- Modify: `src-tauri/src/scene_recall_fader.rs`

- [ ] **Step 1: Write duplicate burst test**

Add this test in the same test module:

```rust
#[tokio::test(start_paused = true)]
async fn duplicate_same_scene_notifications_inside_repeat_delay_send_one_fade_command() {
    let event_bus = AppEventBus::default();
    let command_bus = AppCommandBus::new(event_bus.clone());
    let state = ShellState::default();
    let generation = configure_intro_recall(&state).await;
    let (fade_tx, mut fade_rx) = mpsc::channel(4);
    command_bus
        .set_fade(Some(FadeEngineHandle::new(fade_tx)))
        .await;

    let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
    command_bus.set_lv1(Some(lv1)).await;

    let handle =
        spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());
    release_lv1.send(()).unwrap();
    tokio::time::advance(RECALL_ARMING_DELAY).await;

    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
    tokio::time::advance(Duration::from_millis(100)).await;
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
    tokio::time::advance(Duration::from_millis(100)).await;
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

    let start = tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
        .await
        .expect("first actionable recall should send a fade command")
        .expect("fade command channel should be open");
    match start {
        FadeCommand::RecallSceneFade { reply, .. } => {
            let _ = reply.send(Ok(()));
        }
        other => panic!("expected RecallSceneFade, got {other:?}"),
    }

    tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
        .await
        .expect_err("duplicate same-scene notifications should be suppressed");

    handle.abort();
    server.await.unwrap();
}
```

- [ ] **Step 2: Write repeat-after-delay test**

Add this test:

```rust
#[tokio::test(start_paused = true)]
async fn same_scene_repeat_after_repeat_delay_is_actionable() {
    let event_bus = AppEventBus::default();
    let command_bus = AppCommandBus::new(event_bus.clone());
    let state = ShellState::default();
    let generation = configure_intro_recall(&state).await;
    let (fade_tx, mut fade_rx) = mpsc::channel(4);
    command_bus
        .set_fade(Some(FadeEngineHandle::new(fade_tx)))
        .await;

    let (lv1, release_lv1, server) = spawn_fake_lv1_with_intro(event_bus.clone()).await;
    command_bus.set_lv1(Some(lv1)).await;

    let handle =
        spawn_scene_recall_fader(state.clone(), generation, command_bus, event_bus.clone());
    release_lv1.send(()).unwrap();
    tokio::time::advance(RECALL_ARMING_DELAY).await;

    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
    let first = tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
        .await
        .expect("first recall should send a fade command")
        .expect("fade command channel should be open");
    if let FadeCommand::RecallSceneFade { reply, .. } = first {
        let _ = reply.send(Ok(()));
    } else {
        panic!("expected first RecallSceneFade");
    }

    tokio::time::advance(SAME_SCENE_REPEAT_DELAY).await;
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

    let second = tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
        .await
        .expect("same-scene repeat after delay should send a fade command")
        .expect("fade command channel should be open");
    if let FadeCommand::RecallSceneFade { reply, .. } = second {
        let _ = reply.send(Ok(()));
    } else {
        panic!("expected second RecallSceneFade");
    }

    handle.abort();
    server.await.unwrap();
}
```

- [ ] **Step 3: Run tests and confirm behavior**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri duplicate_same_scene_notifications_inside_repeat_delay_send_one_fade_command
cargo test -p lv1-scene-fade-utility-tauri same_scene_repeat_after_repeat_delay_is_actionable
```

Expected: PASS if Task 2 implementation is correct.

- [ ] **Step 4: Commit duplicate suppression tests**

Run:

```bash
git add src-tauri/src/scene_recall_fader.rs
git commit -m "test: cover scene recall duplicate suppression"
```

---

### Task 5: Add Reconnect Generation Test

**Files:**
- Modify: `src-tauri/src/scene_recall_fader.rs`

- [ ] **Step 1: Write reconnect priming test**

Add this test:

```rust
#[tokio::test(start_paused = true)]
async fn reconnect_generation_primes_again_without_carrying_repeat_history() {
    let event_bus = AppEventBus::default();
    let command_bus = AppCommandBus::new(event_bus.clone());
    let state = ShellState::default();
    let first_generation = configure_intro_recall(&state).await;
    let (fade_tx, mut fade_rx) = mpsc::channel(4);
    command_bus
        .set_fade(Some(FadeEngineHandle::new(fade_tx)))
        .await;

    let first_handle = spawn_scene_recall_fader(
        state.clone(),
        first_generation,
        command_bus.clone(),
        event_bus.clone(),
    );
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
    tokio::time::advance(RECALL_ARMING_DELAY).await;
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));

    let first = tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
        .await
        .expect("armed first generation should send fade")
        .expect("fade command channel should be open");
    if let FadeCommand::RecallSceneFade { reply, .. } = first {
        let _ = reply.send(Ok(()));
    } else {
        panic!("expected first generation RecallSceneFade");
    }

    let _ = state.disconnect().await;
    first_handle.abort();

    let second_generation = configure_intro_recall(&state).await;
    let second_handle = spawn_scene_recall_fader(
        state.clone(),
        second_generation,
        command_bus.clone(),
        event_bus.clone(),
    );
    event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
    tokio::time::advance(Duration::from_millis(100)).await;

    tokio::time::timeout(Duration::from_millis(1), fade_rx.recv())
        .await
        .expect_err("first scene observation in new generation should prime again");

    second_handle.abort();
}
```

- [ ] **Step 2: Run reconnect test**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri reconnect_generation_primes_again_without_carrying_repeat_history
```

Expected: PASS.

- [ ] **Step 3: Commit reconnect coverage**

Run:

```bash
git add src-tauri/src/scene_recall_fader.rs
git commit -m "test: cover recall priming after reconnect"
```

---

### Task 6: Run Targeted Verification and Adjust Existing Tests

**Files:**
- Modify only `src-tauri/src/scene_recall_fader.rs` if existing tests need post-arming event updates.

- [ ] **Step 1: Run all scene recall tests**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri scene_recall
```

Expected: PASS.

- [ ] **Step 2: If an existing test expects immediate first-event automation, update it**

For tests that should exercise actionable recall, use this pattern after spawning the fader:

```rust
event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
tokio::time::advance(RECALL_ARMING_DELAY).await;
event_bus.publish(AppEvent::Lv1(Lv1Event::SceneChanged(intro_scene())));
```

For tests that should exercise unavailable LV1 state or lockout blocking, decide whether they are testing recall preparation or priming. If they are testing recall preparation, make them post-arming tests with `#[tokio::test(start_paused = true)]` and the same pattern above.

- [ ] **Step 3: Re-run all scene recall tests**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri scene_recall
```

Expected: PASS with no panics or timeouts.

- [ ] **Step 4: Commit any compatibility updates**

If Step 2 changed tests, run:

```bash
git add src-tauri/src/scene_recall_fader.rs
git commit -m "test: update scene recall tests for trigger gating"
```

If Step 2 made no changes, do not create an empty commit.

---

### Task 7: Final Verification

**Files:**
- No code changes expected.

- [ ] **Step 1: Run focused Rust tests**

Run:

```bash
cargo test -p lv1-scene-fade-utility-tauri scene_recall
```

Expected: PASS.

- [ ] **Step 2: Run workspace Rust tests**

Run:

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 3: Check worktree state**

Run:

```bash
git status --short
```

Expected: no unstaged implementation changes. Unrelated user changes, if any, must not be staged or modified.

---

## Self-Review

- Spec coverage: Tasks cover initial priming, `2000 ms` arming, `500 ms` same-scene repeat delay, reconnect generation reset, keeping `Lv1Actor` unchanged, and preserving fade engine semantics.
- Placeholder scan: No placeholder implementation steps remain; code snippets and commands are included.
- Type consistency: Plan uses existing `SceneState`, `Lv1Event::SceneChanged`, `FadeEngineHandle`, `FadeCommand`, `AppEventBus`, `AppCommandBus`, `ShellState`, `RECALL_ARMING_DELAY`, and `SAME_SCENE_REPEAT_DELAY` consistently.
