# Monotonic Runtime Generation During Disconnect Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent stale disconnect work from rolling runtime generation backward or aborting a newer connected runtime.

**Architecture:** Keep `disconnect_current_runtime()` as the narrow public interface. Move generation targeting into private `AppLifecycle` operations, remove arbitrary production generation assignment, and make superseded disconnect cleanup an idempotent no-op.

**Tech Stack:** Rust, Tokio actors and synchronization, Tauri lifecycle state, `AppEventBus`, `tracing`, cargo-nextest.

## Global Constraints

- Production runtime generation mutation must be monotonic.
- A disconnect request may affect only the generation active when the request began.
- Stale disconnect work must not abort handles or clear peers owned by a newer generation.
- Normal disconnects must abort handles before publishing `Lv1Event::Disconnected`, then clear peers, advance generation once, and publish `ActiveGenerationChanged`.
- A superseded disconnect returns `ShowCommandResult { changed: false }` without a user-facing success log or disconnected fact.
- Keep generation comparison and runtime-handle ownership private to `AppLifecycle`.
- Preserve lockout, exact scene identity, generation guards, fade abort, manual override, and LV1 write safety.
- Rust behavior uses lifecycle/actor-style tests through lifecycle operations, actor mailboxes, `AppEventBus`, and tracing capture; use explicit gates rather than sleeps.
- Do not address settings persistence (#54) or cue-list reconnect identity (#66) in this change.

---

## File Map

- Modify `src-tauri/src/lifecycle/mod.rs`: add deterministic test coordination, remove rollback from disconnect finalization, bind cleanup to the requested generation, and add lifecycle/actor regression tests.
- Modify `src-tauri/src/runtime/generation.rs`: make arbitrary generation assignment test-only so production code can only advance generation.
- No architecture or frontend files change because the public runtime shape and projected state remain unchanged.

### Task 1: Remove Runtime Generation Rollback From Disconnect Finalization

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs:48-51, 160-203, 286-295, 447-466, 770-2060`
- Modify: `src-tauri/src/runtime/generation.rs:15-18`
- Test: `src-tauri/src/lifecycle/mod.rs` inline `tests` module

**Test style:** Lifecycle/actor-style test using public lifecycle operations, fake actor mailboxes, `AppEventBus`, and two `oneshot` gates. The gate pauses the existing disconnect after generation `N` has been captured and its old handles have been aborted, but before the stale generation write/final cleanup.

**Interfaces:**
- Consumes: `AppLifecycle::disconnect_current_runtime()`, `AppLifecycle::begin_connecting()`, `AppLifecycle::install_runtime_transaction()`, `AppLifecycle::install_accepted_scene_recall_fader()`, and existing fake actor handles.
- Produces: a test-only `BeforeDisconnectGenerationFinalizationHook`; production `RuntimeGeneration` no longer exposes `set`; normal disconnect behavior is otherwise unchanged.

- [ ] **Step 1: Add the deterministic test-only finalization hook**

Add the hook type next to `BeforeSceneRecallStartHook`:

```rust
#[cfg(test)]
type BeforeDisconnectGenerationFinalizationHook =
    Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;
```

Add test-only shared hook state to `AppLifecycle`:

```rust
#[derive(Clone)]
pub struct AppLifecycle {
    inner: Arc<Mutex<LifecycleInner>>,
    event_bus: AppEventBus,
    show: ShowStateHandle,
    show_peers: ShowActorPeers,
    lockout: ShowLockoutReader,
    cue_lists: CueListsHandle,
    cue_lists_peers: CueListsPeers,
    settings: SettingsHandle,
    #[cfg(test)]
    before_disconnect_generation_finalization:
        Arc<Mutex<Option<BeforeDisconnectGenerationFinalizationHook>>>,
}
```

Initialize it in `AppLifecycle::new`:

```rust
#[cfg(test)]
before_disconnect_generation_finalization: Arc::new(Mutex::new(None)),
```

Add private test helpers inside `impl AppLifecycle`:

```rust
#[cfg(test)]
async fn set_before_disconnect_generation_finalization(
    &self,
    hook: BeforeDisconnectGenerationFinalizationHook,
) {
    *self.before_disconnect_generation_finalization.lock().await = Some(hook);
}

#[cfg(test)]
async fn run_before_disconnect_generation_finalization(&self) {
    let hook = self
        .before_disconnect_generation_finalization
        .lock()
        .await
        .take();
    if let Some(hook) = hook {
        hook().await;
    }
}
```

Invoke the hook in `abort_runtime_handles_without_advancing_generation` immediately before the existing `runtime_generation.set(generation).await` call:

```rust
self.show_peers.clear_lv1(generation);
#[cfg(test)]
self.run_before_disconnect_generation_finalization().await;
runtime_generation.set(generation).await;
```

This is test coordination only; it must compile out of production builds.

- [ ] **Step 2: Write the failing rollback-interleaving regression test**

Add this lifecycle test near `disconnect_current_runtime_publishes_active_generation_disconnect`:

```rust
#[tokio::test(flavor = "current_thread")]
async fn stale_disconnect_finalization_preserves_newer_runtime() {
    let event_bus = AppEventBus::default();
    let lifecycle = lifecycle_for_test(event_bus.clone());

    let original_generation = lifecycle.begin_connecting().await.unwrap();
    let old_lv1 = fake_lv1_handle(connected_snapshot());
    let (old_fade_tx, _old_fade_rx) = mpsc::channel(1);
    lifecycle
        .install_runtime_transaction(
            original_generation,
            RuntimeHandles::with_runtime_targets(
                old_lv1,
                FadeEngineHandle::new(old_fade_tx),
            ),
        )
        .await
        .expect("original runtime should install");

    let (cleanup_reached_tx, cleanup_reached_rx) = oneshot::channel();
    let (resume_cleanup_tx, resume_cleanup_rx) = oneshot::channel();
    lifecycle
        .set_before_disconnect_generation_finalization(Box::new(move || {
            Box::pin(async move {
                cleanup_reached_tx
                    .send(())
                    .expect("disconnect should announce the finalization gate");
                resume_cleanup_rx
                    .await
                    .expect("disconnect finalization should be released");
            })
        }))
        .await;

    let lifecycle_for_disconnect = lifecycle.clone();
    let disconnect = tokio::spawn(async move {
        lifecycle_for_disconnect
            .disconnect_current_runtime()
            .await
            .expect("disconnect command should complete")
    });

    cleanup_reached_rx
        .await
        .expect("disconnect should pause before generation finalization");

    let newer_generation = lifecycle.begin_connecting().await.unwrap();
    let newer_lv1 = fake_lv1_handle(connected_snapshot());
    let (newer_fade_tx, _newer_fade_rx) = mpsc::channel(1);
    lifecycle
        .install_runtime_transaction(
            newer_generation,
            RuntimeHandles::with_runtime_targets(
                newer_lv1,
                FadeEngineHandle::new(newer_fade_tx),
            ),
        )
        .await
        .expect("newer runtime should install");

    let (newer_scenes, newer_scenes_task, _newer_scenes_peers) = build_scenes_actor(
        newer_generation,
        lifecycle.current_runtime_generation().await,
        event_bus.clone(),
        event_bus.subscribe(),
        lifecycle.settings.clone(),
        lifecycle.settings_snapshot().await.unwrap(),
        lifecycle.lockout.clone(),
    );
    newer_scenes_task.spawn();
    assert!(
        lifecycle
            .install_accepted_scene_recall_fader(newer_generation, newer_scenes)
            .await
    );

    resume_cleanup_tx
        .send(())
        .expect("disconnect finalization should resume");
    let result = disconnect.await.expect("disconnect task should join");

    assert!(result.changed, "the original runtime was disconnected");
    assert_eq!(lifecycle.active_generation().await, newer_generation);

    let current_lv1 = lifecycle
        .current_lv1()
        .await
        .expect("newer LV1 handle should survive stale finalization");
    let (state_reply, state_rx) = oneshot::channel();
    current_lv1
        .send(Lv1Command::GetState { reply: state_reply })
        .await
        .expect("newer LV1 mailbox should remain open");
    assert_eq!(
        state_rx.await.expect("newer LV1 should reply").connection,
        ConnectionStatus::Connected
    );
    assert!(lifecycle.current_fade().await.is_some());

    let current_scenes = lifecycle
        .current_scene_recall_fader()
        .await
        .expect("newer scenes handle should survive stale finalization");
    let (scenes_reply, scenes_rx) = oneshot::channel();
    current_scenes
        .send(ScenesCommand::InitialProjectionState {
            reply: scenes_reply,
        })
        .await
        .expect("newer scenes mailbox should remain open");
    scenes_rx
        .await
        .expect("newer scenes actor should reply after stale finalization");

    assert!(lifecycle.show_peers.scenes().is_some());
    assert!(lifecycle.cue_lists_peers.scenes().is_some());
}
```

- [ ] **Step 3: Run the regression test and confirm the old code fails**

Run:

```bash
cargo nextest run -p advanced-show-control stale_disconnect_finalization_preserves_newer_runtime --no-capture
```

Expected: FAIL because the stale `RuntimeGeneration::set(original_generation)` allows `clear_runtime_transaction(original_generation)` to abort the newly installed runtime. At least one newer handle assertion should report that the handle is unavailable or its mailbox is closed.

- [ ] **Step 4: Remove the stale generation write**

Replace `abort_runtime_handles_without_advancing_generation` with cleanup that captures the generation only for peer ownership and never assigns it back:

```rust
pub async fn abort_runtime_handles_without_advancing_generation(&self) {
    let generation = {
        let mut inner = self.inner.lock().await;
        inner.handles.abort_all();
        inner.runtime_handles_generation = None;
        inner.generation.current().await
    };
    self.show_peers.clear_lv1(generation);
    #[cfg(test)]
    self.run_before_disconnect_generation_finalization().await;
}
```

Restrict arbitrary generation assignment to test builds in `src-tauri/src/runtime/generation.rs`:

```rust
#[cfg(test)]
pub(crate) async fn set(&self, generation: u64) {
    *self.current.lock().await = generation;
}
```

Do not add a compare-and-set or monotonic setter to production. Production mutation remains `advance` only.

- [ ] **Step 5: Run the targeted test and lifecycle suite**

Run:

```bash
cargo nextest run -p advanced-show-control stale_disconnect_finalization_preserves_newer_runtime --no-capture
cargo nextest run -p advanced-show-control lifecycle
```

Expected: PASS. The newer runtime remains installed after the old disconnect resumes, and all existing lifecycle tests pass.

- [ ] **Step 6: Format, inspect, and commit Task 1**

Run:

```bash
cargo fmt --all
cargo fmt --all -- --check
git diff --check
git status --short
git diff -- src-tauri/src/lifecycle/mod.rs src-tauri/src/runtime/generation.rs
```

Stage only the two intended Rust files and commit:

```bash
git add src-tauri/src/lifecycle/mod.rs src-tauri/src/runtime/generation.rs
git commit -m "fix: keep disconnect generation monotonic"
```

### Task 2: Bind Disconnect Cleanup to Its Requested Generation

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs:286-295, 447-466, 770-2060`
- Test: `src-tauri/src/lifecycle/mod.rs` inline `tests` module

**Test style:** Lifecycle/actor-style test. It invokes the private post-capture lifecycle operation with a stale generation, observes actor handles through mailboxes, observes facts through `AppEventBus`, and observes logging through `TracingCapture`.

**Interfaces:**
- Consumes: the monotonic `RuntimeGeneration` API from Task 1.
- Produces:
  - private `AppLifecycle::disconnect_runtime_generation(&self, generation: u64) -> Result<ShowCommandResult, String>`;
  - private `AppLifecycle::abort_runtime_handles_without_advancing_generation(&self, expected_generation: u64) -> bool`;
  - unchanged public `AppLifecycle::disconnect_current_runtime()`.

- [ ] **Step 1: Write the failing superseded-disconnect test**

Add this test next to the Task 1 regression:

```rust
#[tokio::test(flavor = "current_thread")]
async fn disconnect_for_stale_generation_preserves_newer_runtime() {
    let capture = crate::test_support::TracingCapture::new();
    let _tracing_guard = capture.install();
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let lifecycle = lifecycle_for_test(event_bus);

    let stale_generation = lifecycle.begin_connecting().await.unwrap();
    let newer_generation = lifecycle.begin_connecting().await.unwrap();
    while events.try_recv().is_ok() {}

    let newer_lv1 = fake_lv1_handle(connected_snapshot());
    let (newer_fade_tx, _newer_fade_rx) = mpsc::channel(1);
    lifecycle
        .install_runtime_transaction(
            newer_generation,
            RuntimeHandles::with_runtime_targets(
                newer_lv1,
                FadeEngineHandle::new(newer_fade_tx),
            ),
        )
        .await
        .expect("newer runtime should install");

    let result = lifecycle
        .disconnect_runtime_generation(stale_generation)
        .await
        .expect("superseded disconnect should be a safe no-op");

    assert!(!result.changed);
    assert_eq!(lifecycle.active_generation().await, newer_generation);
    assert!(lifecycle.current_lv1().await.is_some());
    assert!(lifecycle.current_fade().await.is_some());
    assert!(matches!(
        events.try_recv(),
        Err(tokio::sync::broadcast::error::TryRecvError::Empty)
    ));
    assert!(
        capture
            .matching("lv1_disconnected", tracing::Level::INFO)
            .is_empty()
    );
    assert!(
        !capture
            .matching("lv1_disconnect_superseded", tracing::Level::DEBUG)
            .is_empty()
    );
}
```

This intentionally references the not-yet-created private operation so the test first fails to compile.

- [ ] **Step 2: Run the test and confirm the missing generation-scoped operation**

Run:

```bash
cargo nextest run -p advanced-show-control disconnect_for_stale_generation_preserves_newer_runtime --no-capture
```

Expected: compilation FAIL with no method named `disconnect_runtime_generation` on `AppLifecycle`.

- [ ] **Step 3: Make handle cleanup generation-scoped**

Replace the Task 1 cleanup method with:

```rust
async fn abort_runtime_handles_without_advancing_generation(
    &self,
    expected_generation: u64,
) -> bool {
    let generation = {
        let mut inner = self.inner.lock().await;
        let generation = inner.generation.current().await;
        if generation != expected_generation {
            return false;
        }
        inner.handles.abort_all();
        inner.runtime_handles_generation = None;
        generation
    };
    self.show_peers.clear_lv1(generation);
    #[cfg(test)]
    self.run_before_disconnect_generation_finalization().await;
    true
}
```

The expected-generation comparison and handle abort occur under the same lifecycle lock. Callers cannot validate one generation and accidentally clean up another.

- [ ] **Step 4: Pull post-capture behavior into the private lifecycle operation**

Replace `disconnect_current_runtime` with the narrow public wrapper and private generation-scoped operation:

```rust
pub async fn disconnect_current_runtime(&self) -> Result<ShowCommandResult, String> {
    tracing::debug!(
        event = "lv1_disconnect_requested",
        "LV1 disconnect requested"
    );
    let generation = self.active_generation().await;
    self.disconnect_runtime_generation(generation).await
}

async fn disconnect_runtime_generation(
    &self,
    generation: u64,
) -> Result<ShowCommandResult, String> {
    let reason = "Disconnected by user".to_string();
    if !self
        .abort_runtime_handles_without_advancing_generation(generation)
        .await
    {
        tracing::debug!(
            event = "lv1_disconnect_superseded",
            generation,
            "Disconnect request was superseded by a newer LV1 runtime"
        );
        return Ok(ShowCommandResult { changed: false });
    }
    self.event_bus.publish(AppEvent::Lv1 {
        generation,
        event: Lv1Event::Disconnected { reason },
    });
    self.clear_runtime_transaction(generation).await;
    tracing::info!(event = "lv1_disconnected", "Disconnected from LV1");
    Ok(ShowCommandResult { changed: true })
}
```

Do not expose `disconnect_runtime_generation` outside `AppLifecycle`; it exists to hide the post-capture transaction and to permit deterministic lifecycle testing.

- [ ] **Step 5: Update the existing ownership test for the narrowed helper**

In `accepted_runtime_install_records_owning_generation`, pass the installed generation and assert the cleanup occurred:

```rust
assert!(
    lifecycle
        .abort_runtime_handles_without_advancing_generation(generation)
        .await
);

assert_eq!(
    lifecycle.inner.lock().await.runtime_handles_generation,
    None
);
```

Search for every remaining caller and ensure each passes the generation it owns:

```bash
rg -n "abort_runtime_handles_without_advancing_generation" src-tauri/src
```

Expected: only the method definition, `disconnect_runtime_generation`, and lifecycle tests remain.

- [ ] **Step 6: Run both regression tests and the lifecycle suite**

Run:

```bash
cargo nextest run -p advanced-show-control stale_disconnect_finalization_preserves_newer_runtime --no-capture
cargo nextest run -p advanced-show-control disconnect_for_stale_generation_preserves_newer_runtime --no-capture
cargo nextest run -p advanced-show-control lifecycle
```

Expected: PASS. The exact rollback interleaving preserves the newer runtime, stale pre-cleanup requests report no change, and normal lifecycle behavior remains green.

- [ ] **Step 7: Run Rust verification**

Run:

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p advanced-show-control lifecycle
cargo build --workspace
```

Expected: every command exits successfully with no formatting or clippy warnings and all lifecycle tests passing.

- [ ] **Step 8: Inspect and commit Task 2**

Run:

```bash
git status --short
git diff --check
git diff -- src-tauri/src/lifecycle/mod.rs src-tauri/src/runtime/generation.rs
```

The diff should contain only generation-scoped disconnect behavior and its tests. Stage intended files and commit:

```bash
git add src-tauri/src/lifecycle/mod.rs
git commit -m "fix: scope disconnect cleanup by generation"
```

- [ ] **Step 9: Confirm final repository state**

Run:

```bash
git status --short
git log -3 --oneline
```

Expected: clean worktree with the two implementation commits above the design and plan commits. Do not close GitHub #52 until the implementation is pushed or included in the intended pull request workflow.
