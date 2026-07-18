# Lifecycle Connection Finalization Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete issue #55 by making production and lifecycle tests use one generation-safe connection completion implementation.

**Architecture:** Extend the started-runtime bundle to retain exact candidate handles, then move fresh validation, Show transitions, generation acceptance, scene-peer installation, remembered-identity persistence, and scenes task startup into one private lifecycle function. Track which generation owns installed runtime handles so stale cleanup drops only the rejected transaction and never clears newer peers.

**Tech Stack:** Rust 2024, Tokio actors, Tauri lifecycle, `AppEventBus`, cargo-nextest

## Global Constraints

- Execute last, after issue #56; assume complete Show transitions and issue #53's unit settings reply already exist.
- Fresh LV1 validation must immediately gate the accepted sequence.
- Generation acceptance precedes scene-peer installation and remembered-identity persistence.
- A stale transaction aborts only its own handles and cannot clear newer-generation LV1, Show, or cue-list peers.
- Persistence failure remains non-fatal and emits one generation-guarded error; stale persistence emits no misleading error.
- Run `make check`, then one final successful `make smoke`, and inspect `logs/debug-smoke-report.txt` before completion.

---

### Task 1: Track Exact Runtime Ownership For Stale Cleanup

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs:22-79,163-180,238-300`
- Test: `src-tauri/src/lifecycle/mod.rs:1971-1987`

**Interfaces:**
- Consumes: existing `RuntimeHandles`, `RuntimeGeneration`, and generation-specific `ShowActorPeers::clear_lv1`.
- Produces: `LifecycleInner::runtime_handles_generation: Option<u64>` and `abort_rejected_connection_transaction(generation, handles)`.

- [ ] **Step 1: Add failing generation-ownership assertions**

Extend `stale_runtime_install_returns_abortable_handles` and runtime-clear tests to assert the new marker:

```rust
assert_eq!(lifecycle.inner.lock().await.runtime_handles_generation, None);
```

Add an accepted install assertion:

```rust
let generation = lifecycle.begin_connecting().await.unwrap();
let lv1 = fake_lv1_handle(connected_snapshot());
let (fade_tx, _fade_rx) = tokio::sync::mpsc::channel(1);
lifecycle
    .install_runtime_transaction(
        generation,
        RuntimeHandles::with_runtime_targets(lv1, FadeEngineHandle::new(fade_tx)),
    )
    .await
    .unwrap();
assert_eq!(
    lifecycle.inner.lock().await.runtime_handles_generation,
    Some(generation)
);
```

- [ ] **Step 2: Run lifecycle tests red**

Run `cargo nextest run -p advanced-show-control lifecycle`. Expected: compilation fails because `runtime_handles_generation` does not exist.

- [ ] **Step 3: Add and maintain the ownership marker**

Add to `LifecycleInner` and initialize it to `None` in `AppLifecycle::new`:

```rust
runtime_handles_generation: Option<u64>,
```

In `install_runtime_transaction`, after generation and required-handle validation:

```rust
inner.handles = handles;
inner.runtime_handles_generation = Some(generation);
inner.connecting = false;
```

In `install_accepted_scene_recall_fader`, reject unless both guards match:

```rust
if inner.generation.current().await != generation
    || inner.runtime_handles_generation != Some(generation)
{
    return false;
}
```

Set `runtime_handles_generation = None` whenever `clear_runtime_transaction` or `abort_runtime_handles_without_advancing_generation` aborts `inner.handles`.

- [ ] **Step 4: Add generation-specific rejected cleanup**

Add:

```rust
async fn abort_rejected_connection_transaction(
    &self,
    generation: u64,
    mut candidate_handles: RuntimeHandles,
) {
    candidate_handles.abort_all();
    let mut inner = self.inner.lock().await;
    if inner.runtime_handles_generation == Some(generation) {
        inner.handles.abort_all();
        inner.runtime_handles_generation = None;
    }
    drop(inner);
    self.show_peers.clear_lv1(generation);
}
```

Do not clear Show/cue-list scenes peers here; a rejected candidate has not installed them, and any existing value may belong to a newer accepted generation.

- [ ] **Step 5: Run lifecycle tests green**

Run `cargo nextest run -p advanced-show-control lifecycle`. Expected: ownership marker and existing generation tests pass.

---

### Task 2: Replace Duplicate Completion With One Shared Function

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs:50-79,302-461`
- Test: `src-tauri/src/lifecycle/mod.rs:1217-1969`

**Interfaces:**
- Consumes: Task 1 cleanup, issue #56's `complete_lv1_connection_metadata` / `fail_lv1_connection_metadata`, and issue #53's `remember_last_connected_lv1(...) -> Result<(), String>`.
- Produces: one private `finish_connect_transaction(...)`; removes `finish_connect_transaction_inner`.

- [ ] **Step 1: Make the started runtime carry all candidate handles and the narrow test hook**

Change `StartedConnectedRuntime` to:

```rust
struct StartedConnectedRuntime {
    lv1: Lv1ActorHandle,
    fade: FadeEngineHandle,
    scene_recall_fader: ScenesHandle,
    scene_recall_task: crate::scenes::ScenesTask,
    #[cfg(test)]
    before_scene_recall_start: Option<Box<dyn FnOnce(RuntimeGeneration) + Send>>,
}
```

Change `BuiltConnectedRuntime::spawn_lv1_and_fade` to preserve `fade` and initialize the hook:

```rust
fn spawn_lv1_and_fade(self) -> StartedConnectedRuntime {
    self.lv1_task.spawn();
    self.fade_task.spawn();
    StartedConnectedRuntime {
        lv1: self.lv1,
        fade: self.fade,
        scene_recall_fader: self.scene_recall_fader,
        scene_recall_task: self.scene_recall_task,
        #[cfg(test)]
        before_scene_recall_start: None,
    }
}
```

- [ ] **Step 2: Add a test-only setup helper and route tests to the planned shared function**

Inside lifecycle tests, add:

```rust
async fn started_runtime_for_test(
    lifecycle: &AppLifecycle,
    generation: u64,
    runtime_generation: RuntimeGeneration,
    event_bus: AppEventBus,
    lv1: Lv1ActorHandle,
    fade: FadeEngineHandle,
    before_scene_recall_start: Option<Box<dyn FnOnce(RuntimeGeneration) + Send>>,
) -> StartedConnectedRuntime {
    lifecycle
        .install_runtime_transaction(
            generation,
            RuntimeHandles::with_runtime_targets(lv1.clone(), fade.clone()),
        )
        .await
        .expect("test runtime targets should install");
    let initial_settings = lifecycle.settings_snapshot().await.unwrap();
    let (scene_recall_fader, scene_recall_task, scene_recall_peers) = build_scenes_actor(
        generation,
        runtime_generation,
        event_bus.clone(),
        event_bus.subscribe(),
        lifecycle.settings.clone(),
        initial_settings,
    );
    scene_recall_peers.set_peers(lv1.clone(), fade.clone());
    StartedConnectedRuntime {
        lv1,
        fade,
        scene_recall_fader,
        scene_recall_task,
        before_scene_recall_start,
    }
}
```

For each current `finish_connect_transaction_inner` test, build this bundle and replace the call with:

```rust
lifecycle
    .finish_connect_transaction(identity, failure_mode, generation, started_runtime)
    .await
```

Keep each existing assertion for connected/disconnected validation, peer installation, logs, current runtime handles, remembered identity, persistence failure, and stale persistence.

- [ ] **Step 3: Run lifecycle tests red against the missing shared function**

Run `cargo nextest run -p advanced-show-control lifecycle`. Expected: compilation fails because `finish_connect_transaction` does not exist.

- [ ] **Step 4: Implement the one shared production/test completion function**

Add this private method and delete `finish_connect_transaction_inner`:

```rust
async fn finish_connect_transaction(
    &self,
    identity: crate::connection_state::Lv1SystemIdentity,
    failure_mode: ConnectFailureMode,
    generation: u64,
    started_runtime: StartedConnectedRuntime,
) -> Result<ConnectCommandResult, String> {
    let StartedConnectedRuntime {
        lv1,
        fade,
        scene_recall_fader,
        scene_recall_task,
        #[cfg(test)]
        before_scene_recall_start,
    } = started_runtime;

    let (reply, rx) = oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| error.to_string())?;
    let initial_snapshot = rx
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed.to_string())?;
    if initial_snapshot.connection != ConnectionStatus::Connected {
        self.clear_runtime_transaction(generation).await;
        let _ = self.fail_lv1_connection_metadata(failure_mode).await;
        log_lv1_connect_failed(&identity, failure_mode);
        return Err("LV1 did not connect".to_string());
    }

    let connect_result = self
        .complete_lv1_connection_metadata(identity.clone())
        .await
        .map_err(|error| error.to_string())?;

    #[cfg(test)]
    if let Some(before_scene_recall_start) = before_scene_recall_start {
        before_scene_recall_start(self.current_runtime_generation().await);
    }
    #[cfg(test)]
    tokio::task::yield_now().await;

    if !self
        .install_accepted_scene_recall_fader(generation, scene_recall_fader.clone())
        .await
    {
        self.abort_rejected_connection_transaction(
            generation,
            RuntimeHandles {
                lv1: Some(lv1),
                fade: Some(fade),
                scene_recall_fader: Some(scene_recall_fader),
            },
        )
        .await;
        return Err("generation is stale".to_string());
    }

    log_lv1_connected(&identity);
    if let Err(error) = self.remember_last_connected_lv1(generation, identity).await {
        self.log_last_connected_lv1_save_failure(generation, error)
            .await;
    }
    scene_recall_task.spawn();
    Ok(connect_result)
}
```

The connected log moves after exact generation acceptance so stale completion work cannot create a misleading UI log.

- [ ] **Step 5: Route production through the shared function**

In `connect_to_identity`, retain runtime construction, candidate target installation, and LV1/fade task startup. Replace its duplicated fresh validation and accepted sequence with:

```rust
let started_runtime = built_runtime.spawn_lv1_and_fade();
let _ = app;
self.finish_connect_transaction(identity, failure_mode, generation, started_runtime)
    .await
```

Delete the old production validation/metadata/install/persistence/task-start block. Search for `finish_connect_transaction_inner`; expected: no matches.

- [ ] **Step 6: Run focused lifecycle tests**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle
```

Expected: accepted, failed-snapshot, peer-installation, logging, persistence, and stale-generation tests pass through the same function.

---

### Task 3: Prove Stale Cleanup Preserves Newer Peers

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs:1283-1325,1866-1905`
- Test: `src-tauri/src/lifecycle/mod.rs`

**Interfaces:**
- Consumes: Task 2's narrow test hook and Task 1's generation-specific cleanup.
- Produces: actor-style regression coverage that stale finalization neither persists identity nor clears newer scene peers or logs success.

- [ ] **Step 1: Strengthen the stale-generation test before final verification**

Build an unspawned newer scenes actor before invoking the stale transaction:

```rust
let newer_generation = generation + 1;
let (newer_scenes, _newer_task, _newer_peers) = build_scenes_actor(
    newer_generation,
    runtime_generation.clone(),
    event_bus.clone(),
    event_bus.subscribe(),
    lifecycle.settings.clone(),
    lifecycle.settings_snapshot().await.unwrap(),
);
```

Use the stale transaction's hook to install these sentinel peers and advance generation:

```rust
let lifecycle_for_hook = lifecycle.clone();
let newer_scenes_for_hook = newer_scenes.clone();
let (flip_tx, flip_rx) = oneshot::channel();
let hook = Some(Box::new(move |_runtime_generation: RuntimeGeneration| {
    lifecycle_for_hook
        .show_peers
        .set_scenes(newer_scenes_for_hook.clone());
    lifecycle_for_hook
        .cue_lists_peers
        .set_scenes(newer_scenes_for_hook);
    let lifecycle = lifecycle_for_hook.clone();
    tokio::spawn(async move {
        lifecycle.begin_connecting().await;
        let _ = flip_tx.send(());
    });
}) as Box<dyn FnOnce(RuntimeGeneration) + Send>);
```

After `finish_connect_transaction` returns stale, assert:

```rust
assert!(flip_rx.await.is_ok());
assert!(matches!(result, Err(message) if message == "generation is stale"));
assert!(lifecycle.show_peers.scenes().is_some());
assert!(lifecycle.cue_lists_peers.scenes().is_some());
assert_eq!(get_last_connected_lv1(&settings).await, Some(remembered));
assert!(
    capture
        .matching("lv1_connected", tracing::Level::INFO)
        .is_empty()
);
```

Use issue #38's `TracingCapture` with its guard at test start. Keep `_newer_task` alive through all assertions so the sentinel handle remains valid.

- [ ] **Step 2: Run stale and persistence coverage**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle::tests::generation_flip_before_scene_peer_install_leaves_newer_peers_intact
cargo nextest run -p advanced-show-control lifecycle::tests::stale_connect_does_not_replace_remembered_identity
cargo nextest run -p advanced-show-control lifecycle::tests::stale_persistence_handoff_does_not_store_identity_or_log_an_error
cargo nextest run -p advanced-show-control lifecycle::tests::accepted_connect_logs_one_error_when_identity_cannot_be_remembered
```

Expected: all pass; stale work preserves newer peers/identity and emits no connected or persistence-failure log.

---

### Task 4: Final Repository Verification And Commit

**Files:**
- Verify: entire repository
- Verify: `logs/debug-smoke-report.txt`

**Interfaces:**
- Consumes: all seven completed refactor issues.
- Produces: CI-style and hardware evidence that the full refactor sequence remains operational.

- [ ] **Step 1: Run targeted Rust verification**

```bash
cargo nextest run -p advanced-show-control lifecycle
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: lifecycle tests, formatting, and clippy pass.

- [ ] **Step 2: Run final CI-style verification**

```bash
make check
```

Expected: all Rust and frontend formatting, lint, tests, Storybook tests, and builds included by `make check` pass.

- [ ] **Step 3: Run the seventh and final smoke checkpoint**

```bash
make smoke
```

Read `logs/debug-smoke-report.txt`. Expected: its authoritative suite result reports success. If it fails, diagnose and fix the regression, rerun affected targeted checks and `make check`, rerun `make smoke`, and reread the report.

- [ ] **Step 4: Commit the verified issue**

```bash
git status --short
git diff -- src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/lifecycle/mod.rs
git commit -m "refactor: unify connection finalization"
```
