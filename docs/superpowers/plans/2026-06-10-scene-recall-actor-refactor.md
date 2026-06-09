# Scene Recall Actor Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the scene recall actor loop so event settling and recall processing are easier to audit without changing behavior.

**Architecture:** Keep the refactor confined to `src/scene_recall/actor.rs`. Extract the settled-scene recall validation into a helper and replace the pending scene tuple/deadline pair with a named struct.

**Tech Stack:** Rust, Tokio actor loop, paused-time scene recall tests.

---

## File Structure

- Modify `src/scene_recall/actor.rs`: introduce named settle delay, pending observation struct, and helper for processing settled scene observations.
- Do not change `Lv1Actor`, `SceneRecallState`, policy logic, or test behavior.

---

### Task 1: Refactor Actor Settling And Processing

**Files:**
- Modify: `src/scene_recall/actor.rs`

- [ ] **Step 1: Run current tests as the refactor safety net**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall -- --nocapture
```

Expected: PASS. These tests are the behavior lock for the refactor.

- [ ] **Step 2: Add named settle delay and pending observation struct**

Near the top of `src/scene_recall/actor.rs`, add:

```rust
const SCENE_CHANGED_SETTLE_DELAY: std::time::Duration = std::time::Duration::from_millis(25);

struct PendingSceneObservation {
    scene: crate::lv1::types::SceneState,
    seen_at: tokio::time::Instant,
    settle_after: tokio::time::Instant,
}

impl PendingSceneObservation {
    fn new(scene: crate::lv1::types::SceneState, now: tokio::time::Instant) -> Self {
        Self {
            scene,
            seen_at: now,
            settle_after: now + SCENE_CHANGED_SETTLE_DELAY,
        }
    }
}
```

Replace `pending_scene` and `pending_settle_after` with:

```rust
let mut pending_scene: Option<PendingSceneObservation> = None;
```

- [ ] **Step 3: Extract settled recall processing helper**

Add this helper before `scene_label`:

```rust
async fn process_scene_observation(
    generation: u64,
    command_bus: &AppCommandBus,
    event_bus: &AppEventBus,
    recall_state: &mut SceneRecallState,
    duration_zero_logged: &mut HashSet<String>,
    observation: PendingSceneObservation,
) {
    let now = tokio::time::Instant::now();
    if recall_state.is_scene_list_edit_suppressed(observation.seen_at)
        || recall_state.is_scene_list_edit_suppressed(now)
    {
        return;
    }
    if !recall_state.accepts(&observation.scene) {
        return;
    }

    if command_bus.get_generation().await != generation {
        return;
    }

    let lv1_snapshot = match fresh_lv1_snapshot(command_bus, &observation.scene).await {
        Ok(snapshot) => snapshot,
        Err(err) => {
            if command_bus.get_generation().await != generation {
                return;
            }
            event_bus.publish(AppEvent::SceneRecall(
                crate::scene_recall::events::SceneRecallEvent::Blocked {
                    scene_label: scene_label(&observation.scene),
                    reason: format!("LV1 state is unavailable: {err}"),
                },
            ));
            return;
        }
    };

    let scene_id = format!("{}::{}", observation.scene.index, observation.scene.name);
    let scene_config = match command_bus.get_scene_config(scene_id.clone()).await {
        Ok(scene_config) => scene_config,
        Err(err) => {
            if command_bus.get_generation().await != generation {
                return;
            }
            event_bus.publish(AppEvent::SceneRecall(
                crate::scene_recall::events::SceneRecallEvent::Blocked {
                    scene_label: scene_label(&observation.scene),
                    reason: format!("failed to fetch scene config: {err}"),
                },
            ));
            return;
        }
    };

    let lockout = match command_bus.get_lockout().await {
        Ok(lockout) => lockout,
        Err(err) => {
            if command_bus.get_generation().await != generation {
                return;
            }
            event_bus.publish(AppEvent::SceneRecall(
                crate::scene_recall::events::SceneRecallEvent::Blocked {
                    scene_label: scene_label(&observation.scene),
                    reason: format!("failed to fetch lockout: {err}"),
                },
            ));
            return;
        }
    };

    match decide_scene_recall(RecallPolicyInput {
        recalled_scene: observation.scene.clone(),
        lv1_snapshot,
        lockout,
        scene_config,
    }) {
        RecallPolicyDecision::Start(fade_config) => {
            let scene_label = scene_label(&observation.scene);
            if command_bus.get_generation().await != generation {
                return;
            }
            event_bus.publish(AppEvent::SceneRecall(
                crate::scene_recall::events::SceneRecallEvent::Ready {
                    scene_label: scene_label.clone(),
                    target_count: fade_config.targets.len(),
                },
            ));
            event_bus.publish(AppEvent::SceneRecall(
                crate::scene_recall::events::SceneRecallEvent::StartRequested {
                    scene_label: scene_label.clone(),
                },
            ));
            if command_bus.get_generation().await != generation {
                return;
            }
            if command_bus.start_fade(fade_config).await.is_err() {
                if command_bus.get_generation().await != generation {
                    return;
                }
                event_bus.publish(AppEvent::SceneRecall(
                    crate::scene_recall::events::SceneRecallEvent::Blocked {
                        scene_label,
                        reason: "failed to start fade".to_string(),
                    },
                ));
            }
        }
        RecallPolicyDecision::Skip { reason } => {
            if command_bus.get_generation().await != generation {
                return;
            }
            if reason != "duration is 0" || duration_zero_logged.insert(scene_id) {
                event_bus.publish(AppEvent::SceneRecall(
                    crate::scene_recall::events::SceneRecallEvent::Skipped {
                        scene_label: scene_label(&observation.scene),
                        reason,
                    },
                ));
            }
        }
        RecallPolicyDecision::Blocked { reason } => {
            if command_bus.get_generation().await != generation {
                return;
            }
            event_bus.publish(AppEvent::SceneRecall(
                crate::scene_recall::events::SceneRecallEvent::Blocked {
                    scene_label: scene_label(&observation.scene),
                    reason,
                },
            ));
        }
    }
}
```

This is a move of existing logic only. Do not change validation order or generation checks.

- [ ] **Step 4: Simplify actor loop to call helper**

Replace the pending branch with:

```rust
if let Some(deadline) = pending_scene.as_ref().map(|pending| pending.settle_after) {
    tokio::select! {
        event = events.recv() => {
            match event {
                Ok(AppEvent::Lv1(Lv1Event::SceneListChanged(scene_list))) => {
                    recall_state.observe_scene_list(scene_list, tokio::time::Instant::now());
                }
                Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
                    pending_scene = Some(PendingSceneObservation::new(scene, tokio::time::Instant::now()));
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    log_lagged_subscriber("scene-recall", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
        _ = tokio::time::sleep_until(deadline) => {
            if let Some(observation) = pending_scene.take() {
                process_scene_observation(
                    generation,
                    &command_bus,
                    &event_bus,
                    &mut recall_state,
                    &mut duration_zero_logged,
                    observation,
                ).await;
            }
        }
    }
    continue;
}
```

Replace the non-pending `SceneChanged` arm with:

```rust
Ok(AppEvent::Lv1(Lv1Event::SceneChanged(scene))) => {
    pending_scene = Some(PendingSceneObservation::new(scene, tokio::time::Instant::now()));
}
```

- [ ] **Step 5: Run targeted tests**

Run:

```bash
perl -e 'alarm shift; exec @ARGV' 20 cargo test -p advanced-show-control scene_recall -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Run formatting and clippy**

Run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: both PASS. If formatting fails, run `cargo fmt --all`, then rerun both commands.

- [ ] **Step 7: Commit refactor**

Run:

```bash
git status --short
git diff -- src/scene_recall/actor.rs
git add src/scene_recall/actor.rs docs/superpowers/plans/2026-06-10-scene-recall-actor-refactor.md
git commit -m "refactor: clarify scene recall actor flow"
```

Expected: commit succeeds with hooks passing.

---

## Self-Review Notes

- Spec coverage: refactor extracts actor flow only, preserves tests and behavior, and keeps safety checks in same order.
- Placeholder scan: no deferred implementation items.
- Type consistency: uses existing `SceneRecallState`, `AppCommandBus`, `AppEventBus`, `RecallPolicyDecision`, and `PendingSceneObservation` introduced in Task 1.
