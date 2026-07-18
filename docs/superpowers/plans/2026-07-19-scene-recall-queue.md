# Scene Recall Queue Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Queue every ASC-originated LV1 scene recall in one bounded FIFO and dispatch the next request only after exact scene observation and the existing two-ping readiness condition.

**Architecture:** `ScenesTask` owns an eight-request runtime FIFO, validates at admission and dispatch, and tracks the exact post-dispatch scene observation. It transfers the original five-second deadline to `FadeEngine`, which remains the sole owner of ping readiness and reports a delayed completion result back to `scenes`. `show` exposes lockout through a latest-value watch channel, while LV1 supplies a monotonic scene-observation boundary.

**Tech Stack:** Rust 2024, Tokio actors/channels/time, Tauri 2, `tracing`, `cargo nextest`, React/TypeScript debug smoke runner.

## Global Constraints

- Load and follow `test-driven-development`, `touching-safety-critical-code`, `maintaining-module-boundaries`, `writing-tests`, and `adding-logging` before implementation; load `make-sure-it-works` and `verification-before-completion` before final verification.
- LV1 remains the source of truth for scene creation, scene identity, and scene recall.
- Keep one runtime-only FIFO in `scenes`; never persist, coalesce, reorder, or silently drop requests.
- Capacity is exactly eight total requests: one in flight plus at most seven waiting.
- A successful caller reply means `/Set/CurSceneIndex` was accepted by the LV1 writer queue, not merely queued in ASC.
- Require exact scene index and name from a post-dispatch settled observation, followed by exactly two newer same-generation pings.
- Start one absolute five-second deadline at successful LV1 dispatch; do not restart it at scene observation.
- Validate before admission and again from fresh LV1 state and latest show lockout immediately before dispatch.
- Preserve generation guards, lockout, scene-list edit suppression, exact identity, manual override, overlap, same-scene, zero-duration, Abort All, and disconnect behavior.
- Blocked or invalid queued recalls must not abort or alter an existing fade.
- A pre-observation timeout clears recall intent without aborting fades; a post-observation ping timeout preserves issue #35 and aborts paused fades.
- Use actor tests through mailboxes, `AppEventBus`, fake peer channels, and tracing capture. Do not inspect or mutate actor internals for side-effecting behavior.
- Add no new crate or npm dependency.
- Final verification must include `make smoke` and direct inspection of `logs/debug-smoke-report.txt`.

---

## File Structure

- `src-tauri/src/lv1/types.rs`: public runtime-only scene observation and recall-dispatch boundary types.
- `src-tauri/src/lv1/state.rs`: monotonic scene observation sequence owned by the LV1 mirror.
- `src-tauri/src/lv1/commands.rs`, `events.rs`, `actor.rs`, `mod.rs`: carry sequence boundaries through the LV1 mailbox and event bus.
- `src-tauri/src/show/lockout.rs`: focused latest-value lockout reader owned by the show domain.
- `src-tauri/src/show/actor.rs`, `mod.rs`: publish lockout changes to the watch channel.
- `src-tauri/src/lifecycle/mod.rs`, `src-tauri/src/ui/mod.rs`, `src-tauri/src/ui/debug.rs`: pass the show lockout reader into each connected scenes actor.
- `src-tauri/src/fade/commands.rs`: readiness request, delayed result, cancellation reason, and readiness-only command contracts.
- `src-tauri/src/fade/state.rs`, `actor.rs`, `mod.rs`: single barrier implementation with absolute deadlines and delayed completion.
- `src-tauri/src/scenes/recall_queue.rs`: private queue entries, in-flight phase, capacity, deadlines, and cancellation helpers.
- `src-tauri/src/scenes/actor.rs`, `commands.rs`, `mod.rs`: admission, dispatch, exact observation, readiness handoff, progression, cancellation, and Abort All.
- `src-tauri/src/runtime/errors.rs`: typed queue-full and recall-canceled command failures.
- `src-tauri/src/ui/commands/fade.rs`: route Abort All through `ScenesCommand::AbortAll`.
- `src-tauri/src/cue_lists/actor.rs`: deferred-reply and auto-next actor coverage.
- `ui/src/debug/main.tsx`: rapid direct-recall smoke scenario using production commands.
- `docs/architecture.md`: finalized ownership, data flow, deadline, and Abort All behavior.

---

### Task 1: Add LV1 Scene Observation Boundaries

**Files:**
- Modify: `src-tauri/src/lv1/types.rs`
- Modify: `src-tauri/src/lv1/state.rs`
- Modify: `src-tauri/src/lv1/commands.rs`
- Modify: `src-tauri/src/lv1/events.rs`
- Modify: `src-tauri/src/lv1/actor.rs`
- Modify: `src-tauri/src/lv1/mod.rs`
- Modify: `src-tauri/src/projector/cache.rs`
- Modify: `src-tauri/src/runtime/events.rs`
- Modify: `src-tauri/dev-tools/src/bin/lv1-probe.rs`
- Modify: `src-tauri/tests/runtime_bus.rs`
- Modify: `src-tauri/tests/lv1_actor.rs`
- Modify: all Rust test fixtures constructing or matching `Lv1Event::SceneChanged`

**Interfaces:**
- Produces: `SceneObservation { sequence: u64, scene: SceneState }`.
- Produces: `RecallSceneDispatch { scene_observation_sequence: u64 }`.
- Changes: `Lv1Event::SceneChanged(SceneObservation)`.
- Changes: `Lv1Command::RecallScene.reply` to `Option<oneshot::Sender<Result<RecallSceneDispatch, Lv1ActorError>>>`.

- [ ] **Step 1: Write failing state and actor tests for monotonic observation sequences**

Extend `lv1::state::tests::actor_publishes_scene_changes_to_event_bus` and add a second complete scene pair:

```rust
async fn recv_scene_observation(
    rx: &mut tokio::sync::broadcast::Receiver<AppEvent>,
) -> SceneObservation {
    loop {
        if let AppEvent::Lv1 {
            event: Lv1Event::SceneChanged(observation),
            ..
        } = rx.recv().await.unwrap()
        {
            return observation;
        }
    }
}

let first = recv_scene_observation(&mut rx).await;
assert_eq!(first.sequence, 1);
assert_eq!(first.scene, SceneState {
    index: 3,
    name: "Bridge".to_string(),
});

handle_message(&mut state, &OscMessage {
    address: "/Notify/CurSceneIndex".to_string(),
    args: vec![OscArg::Int(4)],
});
handle_message(&mut state, &OscMessage {
    address: "/Notify/Scene/Name".to_string(),
    args: vec![OscArg::String("Chorus".to_string())],
});

let second = recv_scene_observation(&mut rx).await;
assert_eq!(second.sequence, 2);
assert_eq!(state.snapshot().scene, Some(second.scene));
```

Add an LV1 actor integration assertion after one observed scene and one `RecallScene` command:

```rust
let (reply, rx) = oneshot::channel();
handle
    .send(Lv1Command::RecallScene {
        scene_index: 1,
        reply: Some(reply),
    })
    .await
    .unwrap();
let dispatch = rx.await.unwrap().unwrap();
assert_eq!(dispatch.scene_observation_sequence, 1);
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
cargo nextest run -p advanced-show-control lv1::state::tests::actor_publishes_scene_changes_to_event_bus
cargo nextest run -p advanced-show-control actor_parses_and_emits_scene_changed
```

Expected: compile failures because `SceneObservation`, `RecallSceneDispatch`, and the new reply type do not exist.

- [ ] **Step 3: Implement sequence ownership and dispatch boundary types**

Add to `lv1/types.rs` and re-export from `lv1/mod.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SceneObservation {
    pub sequence: u64,
    pub scene: SceneState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecallSceneDispatch {
    pub scene_observation_sequence: u64,
}
```

Add `scene_observation_sequence: u64` to `ActorState`, initialize it to zero, reset it to zero when a new TCP connection is accepted, and centralize complete-scene publication:

```rust
fn observe_scene(state: &mut ActorState, scene: SceneState) {
    state.scene_observation_sequence = state.scene_observation_sequence.saturating_add(1);
    state.scene = Some(scene.clone());
    state.fan_out(Lv1Event::SceneChanged(SceneObservation {
        sequence: state.scene_observation_sequence,
        scene,
    }));
}
```

Return the current boundary only after recall bytes enter the writer queue:

```rust
let result = enqueue_writer_bytes(&writer_tx, bytes)
    .map_err(|_| Lv1ActorError::CommandSendFailed)
    .map(|()| RecallSceneDispatch {
        scene_observation_sequence: state.scene_observation_sequence,
    });
```

- [ ] **Step 4: Update consumers mechanically without changing behavior**

Replace event patterns such as:

```rust
Lv1Event::SceneChanged(scene)
```

with either:

```rust
Lv1Event::SceneChanged(SceneObservation { scene, .. })
```

or, where sequence is asserted:

```rust
Lv1Event::SceneChanged(SceneObservation { sequence, scene })
```

The projector and probe continue consuming `scene` only. Test publishers must construct explicit observations with stable sequences, for example:

```rust
Lv1Event::SceneChanged(SceneObservation {
    sequence: 1,
    scene: intro_scene(),
})
```

Do not add the sequence to persisted or frontend projection types.

- [ ] **Step 5: Run LV1, runtime bus, projector, and workspace compile checks**

Run:

```bash
cargo nextest run -p advanced-show-control lv1
cargo nextest run -p advanced-show-control runtime_bus
cargo nextest run -p advanced-show-control projector
cargo check --workspace --all-targets
```

Expected: all pass.

- [ ] **Step 6: Commit the LV1 boundary**

```bash
git add src-tauri/src/lv1 src-tauri/src/projector/cache.rs src-tauri/src/runtime/events.rs src-tauri/src/scenes src-tauri/src/fade src-tauri/src/show src-tauri/src/lifecycle/mod.rs src-tauri/src/cue_lists src-tauri/src/ui src-tauri/src/lib.rs src-tauri/tests src-tauri/dev-tools/src/bin/lv1-probe.rs
git commit -m "feat: sequence LV1 scene observations"
```

Stage only files changed to adapt `SceneChanged`; omit any unrelated paths shown by `git status`.

---

### Task 2: Expose Fresh Show Lockout Through a Watch Reader

**Files:**
- Create: `src-tauri/src/show/lockout.rs`
- Modify: `src-tauri/src/show/mod.rs`
- Modify: `src-tauri/src/show/actor.rs`
- Modify: `src-tauri/src/show/handle.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/scenes/actor.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/ui/debug.rs`

**Interfaces:**
- Produces: `ShowLockoutReader::current(&self) -> bool`.
- Produces: `ShowLockoutReader::changed(&mut self) -> Result<bool, watch::error::RecvError>`.
- Changes: `build_show_actor(event_bus: AppEventBus) -> (ShowStateHandle, ShowActorTask, ShowActorPeers, ShowLockoutReader)`.
- Changes: `AppLifecycle::new(event_bus: AppEventBus, show: ShowStateHandle, show_peers: ShowActorPeers, lockout: ShowLockoutReader, settings: SettingsHandle)` and connected-runtime construction pass a clone into `build_scenes_actor`.

- [ ] **Step 1: Write failing show actor tests for latest-value lockout**

Add to `show/handle.rs` tests:

```rust
#[tokio::test]
async fn lockout_reader_tracks_the_latest_show_owned_value() {
    let event_bus = AppEventBus::default();
    let (show, task, _peers, mut lockout) = build_show_actor(event_bus);
    task.spawn();

    assert!(!lockout.current());
    let (reply, rx) = oneshot::channel();
    show.send(ShowCommand::SetLockout {
        enabled: true,
        reply: Some(reply),
    })
    .await
    .unwrap();
    rx.await.unwrap();

    assert!(lockout.changed().await.unwrap());
    assert!(lockout.current());
}
```

Add a file-replacement test proving imported lockout also updates the reader.

- [ ] **Step 2: Run the test and verify RED**

Run: `cargo nextest run -p advanced-show-control lockout_reader_tracks_the_latest_show_owned_value`

Expected: compile failure because `ShowLockoutReader` and the fourth builder return do not exist.

- [ ] **Step 3: Implement the show-domain reader**

Create `show/lockout.rs`:

```rust
use tokio::sync::watch;

#[derive(Clone)]
pub struct ShowLockoutReader {
    receiver: watch::Receiver<bool>,
}

impl ShowLockoutReader {
    pub(super) fn new(receiver: watch::Receiver<bool>) -> Self {
        Self { receiver }
    }

    pub fn current(&self) -> bool {
        *self.receiver.borrow()
    }

    pub async fn changed(&mut self) -> Result<bool, watch::error::RecvError> {
        self.receiver.changed().await?;
        Ok(self.current())
    }
}
```

Create the channel from the initial `ShowState::lockout()` in `build_show_actor_with_state`, store the sender in `ShowActorTask`, and synchronize after every handled show command:

```rust
fn publish_lockout_if_changed(lockout_tx: &watch::Sender<bool>, state: &ShowState) {
    lockout_tx.send_if_modified(|current| {
        let next = state.lockout();
        let changed = *current != next;
        *current = next;
        changed
    });
}
```

Call it after `handle_command(command, &mut state, &event_bus, &peers).await`, including commands that replace or reset the show.

- [ ] **Step 4: Wire the reader without adding a reverse show mailbox dependency**

Update production/debug setup:

```rust
let (show, show_task, show_peers, lockout) = build_show_actor(event_bus.clone());
let lifecycle = AppLifecycle::new(
    event_bus,
    show.clone(),
    show_peers,
    lockout,
    settings.clone(),
);
```

Store the reader on `AppLifecycle`, clone it into `build_connected_runtime`, and add it as a `ScenesTask` constructor field. For now, use `lockout.current()` wherever explicit recall validation currently uses `recall_state.lockout()`. Do not send `ShowCommand::GetLockout` from `scenes`.

Also pass `lockout.current()` into settled recall policy, remove the cached lockout field and setters from `ScenesState`, and remove `ShowEvent` lockout subscription branches from `ScenesTask`. The watch reader becomes the one latest-value source for both explicit dispatch and observation-driven fade policy.

- [ ] **Step 5: Run show, lifecycle, scenes, and setup tests**

Run:

```bash
cargo nextest run -p advanced-show-control show
cargo nextest run -p advanced-show-control lifecycle
cargo nextest run -p advanced-show-control scenes
cargo check --workspace --all-targets
```

Expected: all pass.

- [ ] **Step 6: Commit the lockout reader**

```bash
git add src-tauri/src/show src-tauri/src/lifecycle/mod.rs src-tauri/src/scenes/actor.rs src-tauri/src/ui/mod.rs src-tauri/src/ui/debug.rs
git commit -m "feat: expose fresh show lockout state"
```

---

### Task 3: Extend Fade Readiness With Absolute Deadlines and Completion

**Files:**
- Modify: `src-tauri/src/fade/commands.rs`
- Modify: `src-tauri/src/fade/state.rs`
- Modify: `src-tauri/src/fade/actor.rs`
- Modify: `src-tauri/src/fade/mod.rs`
- Modify: `src-tauri/src/scenes/actor.rs` for detached non-ASC requests

**Interfaces:**
- Produces: `RecallReadinessRequest { deadline, completion }`.
- Produces: `RecallReadinessError::{TimedOut { generation, scene_index, scene_name, observed_ping_count }, Cancelled(RecallReadinessCancellation)}`.
- Produces: `RecallReadinessCancellation::{Aborted, Disconnected, GenerationChanged, Superseded, ActorStopped}`.
- Adds: `FadeCommand::WaitForRecallReadiness { scene, expected_generation, readiness, reply }`.
- Changes: `FadeCommand::RecallSceneFade` gains `readiness: RecallReadinessRequest`.

- [ ] **Step 1: Write failing Fade actor tests for delayed completion**

Add mailbox/event-bus tests beside the existing post-recall ping tests:

```rust
#[tokio::test(start_paused = true)]
async fn readiness_only_command_completes_after_two_newer_pings() {
    let (event_bus, engine, mut writes) = spawn_runtime_for_ping_gate_test(vec![
        connected_snapshot(10, vec![]),
    ]).await;
    let (completion, mut completed) = oneshot::channel();
    let (reply, accepted) = oneshot::channel();

    engine.send(FadeCommand::WaitForRecallReadiness {
        scene: FadeSceneIdentity { index: 2, name: "Verse".to_string() },
        expected_generation: 7,
        readiness: RecallReadinessRequest {
            deadline: Instant::now() + Duration::from_secs(5),
            completion: Some(completion),
        },
        reply: Some(reply),
    }).await.unwrap();

    assert_eq!(accepted.await.unwrap(), Ok(()));
    publish_ping(&event_bus, 7, 11);
    assert!(completed.try_recv().is_err());
    publish_ping(&event_bus, 7, 12);
    assert_eq!(completed.await.unwrap(), Ok(()));
    assert!(writes.try_recv().is_err());
}
```

Add tests for:

```rust
assert_eq!(
    completed.await.unwrap(),
    Err(RecallReadinessError::Cancelled(
        RecallReadinessCancellation::Superseded,
    )),
);
```

when a newer recall resets the barrier, and `TimedOut` when the supplied absolute deadline expires. Start the timeout test with a deadline only 200 ms away to prove Fade does not restart five seconds.

- [ ] **Step 2: Run focused Fade tests and verify RED**

Run: `cargo nextest run -p advanced-show-control fade::actor::tests::readiness`

Expected: compile failures for the new readiness contract and command.

- [ ] **Step 3: Define the public readiness command contract**

Add to `fade/commands.rs` and re-export from `fade/mod.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallReadinessCancellation {
    Aborted,
    Disconnected,
    GenerationChanged,
    Superseded,
    ActorStopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecallReadinessError {
    TimedOut {
        generation: u64,
        scene_index: i32,
        scene_name: String,
        observed_ping_count: u8,
    },
    Cancelled(RecallReadinessCancellation),
}

#[derive(Debug)]
pub struct RecallReadinessRequest {
    pub deadline: Instant,
    pub completion: Option<oneshot::Sender<Result<(), RecallReadinessError>>>,
}

impl RecallReadinessRequest {
    pub fn detached(deadline: Instant) -> Self {
        Self { deadline, completion: None }
    }
}
```

- [ ] **Step 4: Make the existing barrier own and resolve completion**

Store `completion` and the caller-supplied `deadline` in `ReadinessBarrier`. Change `start_or_reset_readiness` to cancel any replaced completion with `Superseded`, pause targets, and use the supplied deadline verbatim:

```rust
if let Some(previous) = self.readiness_barrier.take()
    && let Some(completion) = previous.completion
{
    let _ = completion.send(Err(RecallReadinessError::Cancelled(
        RecallReadinessCancellation::Superseded,
    )));
}
```

On the second qualifying ping, take the barrier, resume channels, and send `Ok(())`. On timeout, send `TimedOut` before clearing targets. Change cancellation helpers to accept an explicit `RecallReadinessCancellation` and resolve the sender once.

Add `completion_owned: bool` to the private `ReadinessTimeoutContext` returned to
the actor so logging can distinguish ASC-owned and detached barriers. Remove the
unconditional `clear_readiness_barrier()` call from `complete_fade`; target
completion and connection-wide readiness are independent lifecycles, especially
when manual override removes the final target while the queue still awaits two
pings.

- [ ] **Step 5: Handle readiness-only, zero-duration, and shutdown paths**

Implement `WaitForRecallReadiness` by validating generation, getting a fresh LV1 snapshot for the ping boundary, and starting the same barrier without creating targets.

For `RecallSceneFade`:

- Keep existing zero-duration writes before starting the barrier.
- Start/reset readiness for empty or zero-duration configurations when a completion owner exists.
- Do not call `complete_fade` merely because no targets exist while a readiness barrier is active.
- Preserve detached issue #35 behavior for normal LV1-originated timed recalls.

Resolve cancellations as follows:

```rust
AbortAll => Aborted
matching LV1 disconnect => Disconnected
active generation change => GenerationChanged
command channel closure / actor exit => ActorStopped
new validated recall => Superseded
```

Suppress the existing user-facing timeout warning only when the timed-out barrier carried an ASC completion sender; `scenes` will own that combined message later. Keep the diagnostic fields and `FadeAborted` fact.

- [ ] **Step 6: Adapt current scenes fade sends with detached readiness**

Until queue ownership is added, construct:

```rust
readiness: RecallReadinessRequest::detached(
    Instant::now() + Duration::from_secs(5),
),
```

at the settled scene observation. This preserves existing behavior and keeps the workspace compiling.

- [ ] **Step 7: Run targeted and regression tests**

Run:

```bash
cargo nextest run -p advanced-show-control fade
cargo nextest run -p advanced-show-control scenes
cargo nextest run -p advanced-show-control lv1_actor
```

Expected: all pass, including existing manual override, overlap, same-scene, timeout, disconnect, and generation tests.

- [ ] **Step 8: Commit Fade readiness completion**

```bash
git add src-tauri/src/fade src-tauri/src/scenes/actor.rs
git commit -m "feat: expose fade recall readiness completion"
```

---

### Task 4: Add Bounded Recall Admission and Dispatch

**Files:**
- Create: `src-tauri/src/scenes/recall_queue.rs`
- Modify: `src-tauri/src/scenes/mod.rs`
- Modify: `src-tauri/src/scenes/commands.rs`
- Modify: `src-tauri/src/scenes/actor.rs`
- Modify: `src-tauri/src/runtime/errors.rs`

**Interfaces:**
- Produces: private `RecallQueue`, `QueuedRecall`, `InFlightRecall`, and `InFlightPhase`.
- Produces: `RECALL_QUEUE_CAPACITY: usize = 8` and `RECALL_COMPLETION_TIMEOUT: Duration = 5s`.
- Adds: `AppCommandError::RecallQueueFull` and `AppCommandError::RecallCanceled(String)`.
- Changes: `ScenesCommand::RecallScene` admission retains its caller reply until actual LV1 dispatch.

- [ ] **Step 1: Write failing scenes actor tests for dispatch and pending replies**

Add actor tests through the scenes mailbox and fake LV1 channel. Introduce these test-only fixtures in `scenes/actor.rs`:

```rust
struct ObservedLv1Recall {
    scene_index: i32,
    reply: oneshot::Sender<Result<RecallSceneDispatch, Lv1ActorError>>,
}

impl ObservedLv1Recall {
    fn reply(self, result: Result<RecallSceneDispatch, Lv1ActorError>) {
        let _ = self.reply.send(result);
    }
}

struct RecallQueueFixture {
    event_bus: AppEventBus,
    handle: ScenesHandle,
    show: ShowStateHandle,
    snapshot: watch::Sender<Lv1StateSnapshot>,
    lv1_recalls: mpsc::Receiver<ObservedLv1Recall>,
}

impl RecallQueueFixture {
    async fn connected_with_scenes(scene_configs: Vec<SceneConfig>) -> Self {
        let event_bus = AppEventBus::default();
        let (show, show_task, _show_peers, lockout) = build_show_actor(event_bus.clone());
        show_task.spawn();

        let initial_snapshot = Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: scene_configs
                .iter()
                .map(|scene| SceneListEntry {
                    index: scene.scene_index.unwrap(),
                    name: scene.scene_name.clone(),
                })
                .collect(),
            channels: vec![],
            ping_sequence: 10,
        };
        let (snapshot, snapshot_rx) = watch::channel(initial_snapshot);
        let (recall_tx, lv1_recalls) = mpsc::channel(8);
        let (lv1_tx, mut lv1_rx) = mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    Lv1Command::GetState { reply } => {
                        let _ = reply.send(snapshot_rx.borrow().clone());
                    }
                    Lv1Command::RecallScene {
                        scene_index,
                        reply: Some(reply),
                    } => {
                        recall_tx
                            .send(ObservedLv1Recall { scene_index, reply })
                            .await
                            .unwrap();
                    }
                    Lv1Command::RecallScene { reply: None, .. } => {
                        panic!("queued recalls require an LV1 reply");
                    }
                    Lv1Command::WriteBatch(_) => {}
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let runtime_generation = RuntimeGeneration::new();
        runtime_generation.set(1).await;
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        let (fade, fade_task, fade_peers) = build_engine(
            runtime_generation.clone(),
            event_bus.clone(),
            1,
        );
        fade_peers.set_lv1(lv1.clone());
        fade_task.spawn();
        let (handle, task, peers) = build_scenes_actor(
            1,
            runtime_generation,
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(AppSettings::default()),
            AppSettings::default(),
            lockout,
        );
        peers.set_peers(lv1, fade);
        task.spawn();
        install_scene_document(
            &handle,
            SceneDocument {
                scene_configs,
                selected_scene_internal_id: None,
            },
        )
        .await;

        Self {
            event_bus,
            handle,
            show,
            snapshot,
            lv1_recalls,
        }
    }

    async fn send_recall(
        &self,
        internal_scene_id: Uuid,
    ) -> oneshot::Receiver<Result<RecallSceneResult, AppCommandError>> {
        let (reply, rx) = oneshot::channel();
        self.handle
            .send(ScenesCommand::RecallScene {
                internal_scene_id,
                reply,
            })
            .await
            .unwrap();
        rx
    }

    async fn next_lv1_recall(&mut self) -> ObservedLv1Recall {
        self.lv1_recalls.recv().await.expect("expected LV1 recall")
    }

    fn try_next_lv1_recall(&mut self) -> Option<ObservedLv1Recall> {
        self.lv1_recalls.try_recv().ok()
    }

    fn set_snapshot(&self, snapshot: Lv1StateSnapshot) {
        self.snapshot.send_replace(snapshot);
    }

    fn publish_scene_observation(&self, generation: u64, sequence: u64, scene: SceneState) {
        self.event_bus.publish(AppEvent::Lv1 {
            generation,
            event: Lv1Event::SceneChanged(SceneObservation { sequence, scene }),
        });
    }

    fn publish_ping(&self, generation: u64, sequence: u64) {
        self.event_bus.publish(AppEvent::Lv1 {
            generation,
            event: Lv1Event::PingReceived { sequence },
        });
    }
}

fn queue_scene(index: i32, name: &str) -> SceneConfig {
    SceneConfig {
        internal_scene_id: Uuid::from_u128(index as u128),
        scene_index: Some(index),
        scene_name: name.to_string(),
        duration_ms: 1_000,
        channel_configs: vec![],
        scoped_channels: vec![],
        scope_toggles: SceneScopeToggles::default(),
    }
}
```

Use the fixture in the first core test:

```rust
#[tokio::test]
async fn first_recall_dispatches_immediately_and_second_reply_stays_pending() {
    let mut fixture = RecallQueueFixture::connected_with_scenes(vec![
        queue_scene(1, "Intro"),
        queue_scene(2, "Verse"),
    ]).await;
    let first = fixture.send_recall(Uuid::from_u128(1)).await;
    let mut second = fixture.send_recall(Uuid::from_u128(2)).await;

    let first_dispatch = fixture.next_lv1_recall().await;
    assert_eq!(first_dispatch.scene_index, 1);
    first_dispatch.reply(Ok(RecallSceneDispatch {
        scene_observation_sequence: 10,
    }));
    assert_eq!(first.await.unwrap().unwrap().lv1_scene_index, 1);

    assert!(second.try_recv().is_err());
    assert!(fixture.try_next_lv1_recall().is_none());
}
```

Add a capacity test that admits one in-flight and seven waiting requests, then asserts the ninth reply is:

```rust
Err(AppCommandError::RecallQueueFull)
```

Add an admission test proving lockout, disconnect, and exact identity mismatch never consume capacity or send `Lv1Command::RecallScene`.

- [ ] **Step 2: Run scenes tests and verify RED**

Run: `cargo nextest run -p advanced-show-control scenes::actor::tests::recall_queue`

Expected: the second request currently dispatches immediately, and `RecallQueueFull` does not exist.

- [ ] **Step 3: Define the private queue state**

Create `scenes/recall_queue.rs` with these concrete shapes:

```rust
pub(super) const RECALL_QUEUE_CAPACITY: usize = 8;
pub(super) const RECALL_COMPLETION_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) struct QueuedRecall {
    pub request_id: Uuid,
    pub internal_scene_id: Uuid,
    pub reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
}

pub(super) enum InFlightPhase {
    AwaitingObservation {
        dispatch_sequence: u64,
        deadline: Instant,
    },
    AwaitingReadiness {
        deadline: Instant,
    },
}

pub(super) struct InFlightRecall {
    pub request_id: Uuid,
    pub generation: u64,
    pub result: RecallSceneResult,
    pub phase: InFlightPhase,
}

#[derive(Default)]
pub(super) struct RecallQueue {
    pub in_flight: Option<InFlightRecall>,
    pub waiting: VecDeque<QueuedRecall>,
}
```

Give `RecallQueue` focused methods for `len`, `is_full`, admission, taking the next waiting entry, setting in-flight, and draining pending reply senders with a supplied error. Keep validation and actor I/O out of this file.

- [ ] **Step 4: Replace synchronous explicit recall handling with admission and drain**

Change the `RecallScene` command branch to:

1. Obtain fresh LV1 state.
2. Validate against `lockout.current()` and a current `SceneDocument`.
3. Reject full capacity visibly.
4. Push a distinct `QueuedRecall` with `Uuid::new_v4()`.
5. If no item is in flight, call an async `dispatch_next_recall` helper.

`dispatch_next_recall` must re-fetch LV1 state and re-run `validate_recall_scene_request` before sending. On success:

```rust
let deadline = Instant::now() + RECALL_COMPLETION_TIMEOUT;
queue.in_flight = Some(InFlightRecall {
    request_id,
    generation,
    result: result.clone(),
    phase: InFlightPhase::AwaitingObservation {
        dispatch_sequence: dispatch.scene_observation_sequence,
        deadline,
    },
});
let _ = reply.send(Ok(result));
```

On revalidation failure, reply to only that entry and continue to the next FIFO item. On LV1 recall send failure, fail the current entry and cancel all later entries because the command path is unavailable.

- [ ] **Step 5: Preserve existing owning-layer logs**

Move `scene_recall_requested`, `scene_recall_blocked`, and `scene_recall_command_sent` logging into admission/dispatch helpers. Add one queue-full warning:

```rust
tracing::warn!(
    event = "scene_recall_queue_full",
    internal_scene_id = %internal_scene_id,
    capacity = RECALL_QUEUE_CAPACITY,
    "Scene recall blocked because the recall queue is full"
);
```

Do not log the same failure from cue lists or Tauri adapters.

- [ ] **Step 6: Run queue core tests and scenes regressions**

Run:

```bash
cargo nextest run -p advanced-show-control scenes::actor::tests::recall_queue
cargo nextest run -p advanced-show-control scenes
```

Expected: the first request replies after LV1 dispatch, later requests remain pending, capacity is exact, and existing scene mutations/observations still pass.

- [ ] **Step 7: Commit bounded queue admission**

```bash
git add src-tauri/src/scenes src-tauri/src/runtime/errors.rs
git commit -m "feat: queue ASC scene recall requests"
```

---

### Task 5: Release FIFO Through Exact Observation and Fade Readiness

**Files:**
- Modify: `src-tauri/src/scenes/recall_queue.rs`
- Modify: `src-tauri/src/scenes/actor.rs`
- Modify: `src-tauri/src/scenes/commands.rs` test fixtures
- Modify: `src-tauri/src/fade/actor.rs` only if integration exposes a missing outcome edge

**Interfaces:**
- Consumes: `SceneObservation.sequence`, `RecallReadinessRequest`, and `RecallReadinessError` from Tasks 1 and 3.
- Produces: an internal scenes readiness-completion channel carrying `{ request_id, generation, result }`.
- Changes: `PendingSceneObservation` stores event generation and observation sequence.

- [ ] **Step 1: Write failing actor tests for exact observation and two-ping progression**

Add tests proving:

```rust
fixture.publish_scene_observation(generation, dispatch_sequence, exact_scene.clone()); // equal boundary
assert!(fixture.try_next_lv1_recall().is_none());

fixture.publish_scene_observation(generation - 1, dispatch_sequence + 1, exact_scene.clone()); // stale
assert!(fixture.try_next_lv1_recall().is_none());

fixture.publish_scene_observation(generation, dispatch_sequence + 1, SceneState {
    index: exact_scene.index,
    name: "Wrong".to_string(),
});
assert!(fixture.try_next_lv1_recall().is_none());

fixture.publish_ping(generation, 11); // before exact observation
fixture.publish_scene_observation(generation, dispatch_sequence + 2, exact_scene);
fixture.publish_ping(generation, 12); // first post-observation ping
assert!(fixture.try_next_lv1_recall().is_none());
fixture.publish_ping(generation, 13); // second post-observation ping
assert_eq!(fixture.next_lv1_recall().await.scene_index, 2);
```

Add a repeated same-scene test with two distinct queued requests and two distinct newer observation sequences. Add disabled, empty-scope, and zero-duration cases; zero-duration must retain immediate writes while queue progression still waits for two pings.

- [ ] **Step 2: Run the focused tests and verify RED**

Run: `cargo nextest run -p advanced-show-control scenes::actor::tests::recall_queue`

Expected: no second dispatch occurs because exact observation/readiness completion is not wired yet.

- [ ] **Step 3: Carry observation metadata through the existing settle path**

Change the pending type to:

```rust
struct PendingSceneObservation {
    generation: u64,
    sequence: u64,
    scene: SceneState,
    seen_at: Instant,
    settle_after: Instant,
}
```

Construct it only from `Lv1Event::SceneChanged(SceneObservation { sequence, scene })`. Keep the 25 ms replacement/settle behavior unchanged.

At settle time, derive an optional ASC context only when all of these hold:

```rust
observation.generation == in_flight.generation
    && observation.sequence > dispatch_sequence
    && observation.scene.index == in_flight.result.lv1_scene_index
    && observation.scene.name == in_flight.result.scene.scene_name
```

The normal fresh LV1 snapshot check must still confirm exact identity before handoff.
For an exact ASC observation, run that fresh check even when normal repeat/edit
policy will skip starting a fade; queue readiness cannot bypass fresh-state
validation. Before changing to `AwaitingReadiness`, compare `Instant::now()` to
the original deadline. If it has already expired, use the pre-observation
timeout path and do not send any Fade command.

- [ ] **Step 4: Attach one delayed completion to normal recall policy**

Create a oneshot completion for the matching in-flight request and spawn only a forwarding task into an actor-owned `mpsc::Sender<RecallReadinessCompletion>`:

```rust
let (completion, completed) = oneshot::channel();
let completion_tx = readiness_completion_tx.clone();
tokio::spawn(async move {
    let result = completed.await.unwrap_or_else(|_| {
        Err(RecallReadinessError::Cancelled(
            RecallReadinessCancellation::ActorStopped,
        ))
    });
    let _ = completion_tx.send(RecallReadinessCompletion {
        request_id,
        generation,
        result,
    }).await;
});
```

Pass `RecallReadinessRequest { deadline, completion: Some(completion) }` into `FadeCommand::RecallSceneFade` for every `RecallPolicyDecision::Start`, including zero duration. For `Skip`, `Blocked`, edit suppression, repeat suppression, or another no-configuration path belonging to the exact ASC observation, send `FadeCommand::WaitForRecallReadiness` with the same request and deadline.

Normal LV1-originated observations continue using detached readiness only when they start a timed fade.

- [ ] **Step 5: Progress only from matching readiness success**

Add a readiness-completion branch to both scenes actor select paths. Ignore a result unless request ID and generation match the current `AwaitingReadiness` item. On `Ok(())`, clear in-flight and call `dispatch_next_recall`.

The transfer changes the phase but never the deadline:

```rust
in_flight.phase = InFlightPhase::AwaitingReadiness { deadline };
```

Do not dispatch from a scene event, first ping, mismatched completion, or stale completion.

- [ ] **Step 6: Run core FIFO, Fade, and same-scene regressions**

Run:

```bash
cargo nextest run -p advanced-show-control scenes::actor::tests::recall_queue
cargo nextest run -p advanced-show-control fade
cargo nextest run -p advanced-show-control same_scene
```

Expected: all pass.

- [ ] **Step 7: Commit exact readiness progression**

```bash
git add src-tauri/src/scenes src-tauri/src/fade/actor.rs
git commit -m "feat: release recall queue after LV1 readiness"
```

---

### Task 6: Add Queue Cancellation, Abort All, and Owning Logs

**Files:**
- Modify: `src-tauri/src/scenes/actor.rs`
- Modify: `src-tauri/src/scenes/commands.rs`
- Modify: `src-tauri/src/scenes/recall_queue.rs`
- Modify: `src-tauri/src/ui/commands/fade.rs`
- Modify: `src-tauri/src/runtime/errors.rs`
- Modify: `src-tauri/src/fade/actor.rs` for timeout-log ownership only

**Interfaces:**
- Adds: `ScenesCommand::AbortAll { reply: oneshot::Sender<Result<(), AppCommandError>> }`.
- Consumes: mutable `ShowLockoutReader::changed()` branch.
- Produces: one cancellation helper taking a stable reason and resolving every waiting caller once.

- [ ] **Step 1: Write failing cancellation and logging actor tests**

Use paused Tokio time and `TracingCapture` to cover:

- Five seconds without exact observation clears all queued replies and does not send `FadeCommand::AbortAll`.
- Fade `TimedOut` completion clears all queued replies and yields one combined warning.
- Matching disconnect, active generation change, lockout activation, event-bus lag/closure, peer closure, and shutdown clear all intent.
- Stale disconnect/generation/readiness outcomes do not affect the current queue.
- Abort All clears queue intent before forwarding exactly one Fade abort.
- A blocked revalidation failure still affects only that request and does not abort Fade.
- Failed LV1 dispatch clears later intent but leaves an existing fade untouched.

Assert exact owning message examples:

```rust
assert_eq!(
    capture.matching("scene_recall_queue_cancelled", Level::WARN)[0]
        .message.as_deref(),
    Some("Queued scene recalls were canceled because LV1 recall readiness was lost"),
);
assert!(capture.matching("fade_post_recall_ping_timeout", Level::WARN).is_empty());
```

- [ ] **Step 2: Run cancellation tests and verify RED**

Run: `cargo nextest run -p advanced-show-control scenes::actor::tests::recall_queue_cancellation`

Expected: timeout/cancellation branches and `ScenesCommand::AbortAll` do not exist.

- [ ] **Step 3: Implement one idempotent cancellation path**

Add `fn cancel_recall_queue(queue: &mut RecallQueue, reason: &str, emit_log: bool)` that:

1. Takes and drops the in-flight item.
2. Sends `AppCommandError::RecallCanceled(reason.clone())` to every waiting caller.
3. Leaves an already-dispatched caller alone because its reply was already sent.
4. Logs once only when at least one in-flight/waiting item existed.
5. Invalidates stale delayed completions by removing the request ID.

Use it for:

```text
pre-observation timeout
post-observation Fade error
matching LV1 disconnect
active generation change
lockout transition to true
event-bus lag or closure
LV1/Fade peer command closure
Scenes shutdown or mailbox closure
```

On pre-observation timeout, do not send a Fade abort. On post-observation `TimedOut`, describe both paused fade abortion and queued recall cancellation in the one scenes-owned warning.

- [ ] **Step 4: Route Abort All through scenes**

Implement:

```rust
ScenesCommand::AbortAll { reply } => {
    cancel_recall_queue(&mut recall_queue, "Abort All was requested", true);
    let (fade_reply, fade_result) = oneshot::channel();
    let result = match peers.handles().fade
        .send(FadeCommand::AbortAll { reply: Some(fade_reply) })
        .await
    {
        Ok(()) => fade_result
            .await
            .map_err(|_| AppCommandError::ReplyChannelClosed)
            .and_then(|result| result),
        Err(_) => Err(AppCommandError::FadeUnavailable),
    };
    let _ = reply.send(result);
}
```

Keep the actor code explicit rather than adding a hidden handle helper. Change `abort_all_fades` to obtain `current_scene_recall_fader()` and send this command. Preserve the frontend Tauri command name.

- [ ] **Step 5: Add lockout and timeout branches to the actor loop**

Select on `lockout.changed()` in both pending-scene and ordinary loop states. A transition to `true` cancels queue intent; a transition to `false` only updates the latest value. Treat watch closure as loss of safe state.

Also re-read `lockout.current()` immediately before every dispatch and exact
observation handoff. If it is true, cancel queue intent before normal fade policy
can race the watch notification. Any actor-loop exit caused by settings refresh
failure, event-bus closure, command mailbox closure, or explicit shutdown must
run the same cancellation helper before returning.

Build an optional sleep future from the in-flight absolute deadline, as the existing Fade actor does. When the phase is `AwaitingObservation`, expiry is owned by scenes. Once phase is `AwaitingReadiness`, Fade owns that same deadline and its delayed result owns progression/cancellation.

- [ ] **Step 6: Run safety, logging, Fade, and adapter tests**

Run:

```bash
cargo nextest run -p advanced-show-control scenes
cargo nextest run -p advanced-show-control fade
cargo nextest run -p advanced-show-control commands::tests
cargo nextest run -p advanced-show-control lifecycle
```

Expected: all pass with no duplicate timeout/cancellation warnings.

- [ ] **Step 7: Commit safety cancellation and Abort All**

```bash
git add src-tauri/src/scenes src-tauri/src/fade/actor.rs src-tauri/src/runtime/errors.rs src-tauri/src/ui/commands/fade.rs
git commit -m "feat: cancel queued recalls safely"
```

---

### Task 7: Prove Cue Auto-Next, Extend Smoke, Document, and Verify

**Files:**
- Modify: `src-tauri/src/cue_lists/actor.rs`
- Modify: `ui/src/debug/main.tsx`
- Modify: `docs/architecture.md`

**Interfaces:**
- Consumes: unchanged `ScenesCommand::RecallScene` reply contract.
- Produces: no new production interface.

- [ ] **Step 1: Write the cue-list deferred-reply regression test**

Extend the existing cue recall actor test so the fake scenes task retains the reply:

```rust
let (dispatch_seen, dispatched) = oneshot::channel();
let (release_reply, release) = oneshot::channel();
let fake = tokio::spawn(async move {
    let ScenesCommand::RecallScene { reply, .. } = scene_rx.recv().await.unwrap() else {
        panic!("expected RecallScene");
    };
    let _ = dispatch_seen.send(());
    release.await.unwrap();
    let _ = reply.send(Ok(RecallSceneResult {
        scene: scene_config(scene_id),
        lv1_scene_index: 1,
    }));
});

dispatched.await.unwrap();
assert_eq!(current_document(&handle).await.cued_cue_entry_id, Some(first_entry.id));
let _ = release_reply.send(());
assert_eq!(recall_result_rx.await.unwrap().unwrap().next_cued_entry_id, Some(second_entry.id));
```

Add a failure variant proving `RecallCanceled` does not auto-next.

- [ ] **Step 2: Run cue-list tests and verify behavior**

Run: `cargo nextest run -p advanced-show-control cue_lists::actor::tests::recall_cued_cue`

Expected: pass without production cue-list changes; if it fails, fix only reply timing assumptions in `recall_cued_cue`.

- [ ] **Step 3: Add a rapid production-command smoke scenario**

Add `"rapid-scene-recall-queue"` after `"scene-recall"` in the smoke test list. Use zero-duration settings to keep the test short while preserving immediate-write behavior:

```ts
await test("rapid-scene-recall-queue", async () => {
  try {
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneA,
      durationMs: 0,
    });
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneB,
      durationMs: 0,
    });

    await invoke("recall_scene", { internalSceneId: sceneB });
    const dispatchOrder: string[] = [];
    const second = invoke("recall_scene", { internalSceneId: sceneA }).then(() => {
      dispatchOrder.push("Smoke A");
    });
    await sleep(10);
    const third = invoke("recall_scene", { internalSceneId: sceneB }).then(() => {
      dispatchOrder.push("Smoke B");
    });

    await Promise.all([second, third]);
    if (dispatchOrder.join(",") !== "Smoke A,Smoke B") {
      throw new Error(`recall dispatch order was ${dispatchOrder.join(",")}`);
    }
    await waitScene("Smoke B");
  } finally {
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneA,
      durationMs: 1_000,
    });
    await invoke("set_scene_duration_ms", {
      internalSceneId: sceneB,
      durationMs: 1_000,
    });
  }
});
```

Do not add a debug-only recall path; this must invoke the production `recall_scene` command.

- [ ] **Step 4: Update architecture documentation**

Update `docs/architecture.md` with exact statements:

- `scenes` owns a bounded eight-request ASC recall FIFO and retains replies until dispatch.
- LV1 scene observations carry a connection-local sequence used only for post-dispatch boundaries.
- `show` exposes lockout through a latest-value reader; no reverse `scenes -> show` mailbox dependency is added.
- One five-second deadline transfers from scenes observation wait to Fade readiness.
- Every ASC recall, including no-fade and zero-duration recalls, waits before releasing the next request.
- Abort All clears queue intent through scenes and target state through Fade.

Update the peer/ownership tables and post-recall readiness section without duplicating the implementation spec.

- [ ] **Step 5: Run targeted formatting and tests**

Run:

```bash
cargo fmt --all -- --check
cargo nextest run -p advanced-show-control scenes
cargo nextest run -p advanced-show-control cue_lists
cargo nextest run -p advanced-show-control fade
cargo nextest run -p advanced-show-control lv1_actor
npm --prefix ui run format:check
npm --prefix ui run typecheck
```

Expected: all pass.

- [ ] **Step 6: Commit cue, smoke, and architecture coverage**

```bash
git add src-tauri/src/cue_lists/actor.rs ui/src/debug/main.tsx docs/architecture.md
git commit -m "test: cover queued scene recall workflow"
```

- [ ] **Step 7: Run full non-hardware verification**

Run: `make check`

Expected: formatting, Clippy, Rust/UI tests, builds, typecheck, and Storybook tests all pass.

- [ ] **Step 8: Run required hardware smoke verification**

Run: `make smoke`

After the process exits, open and read `logs/debug-smoke-report.txt`. The authoritative result must contain:

```text
TEST rapid-scene-recall-queue PASS
SUITE PASS
```

If hardware or the debug environment is unavailable, record the exact failure and do not claim smoke success. If the report exposes an implementation defect, add a failing automated regression where practical, fix it, rerun the smallest affected checks, rerun `make check`, and rerun `make smoke`.

- [ ] **Step 9: Inspect final repository state**

Run:

```bash
git status --short
git log --oneline -10
```

Expected: no uncommitted implementation changes and a sequence of focused commits for LV1 boundaries, lockout state, Fade readiness, queue behavior, safety cancellation, and workflow coverage.
