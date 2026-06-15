# Shell State Projection Efficiency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bound Tauri shell frontend state projection to a 10 Hz cadence while preserving backend state application and removing the unused raw `lv1-event` stream.

**Architecture:** Keep `ShellState` as the owner of projected runtime state, but split event application from snapshot creation. The shell projector drains the `AppEventBus` continuously, marks projection dirty after active-generation events, and emits one coalesced `app-status-changed` snapshot on a 100 ms Tokio interval while dirty.

**Tech Stack:** Rust, Tokio broadcast channels and paused-time tests, Tauri event emission, existing `advanced-show-control-tauri` command and app-state modules.

---

## File Structure

- Modify `src-tauri/src/app_state/events.rs`: add event-application methods that mutate `ShellState` for the active generation without building `AppViewState` snapshots per event. Keep existing snapshot-returning helpers where command code or existing tests still use them.
- Modify `src-tauri/src/app_state/projection.rs`: add a no-snapshot projector helper for `AppEvent::SceneRecall` and diagnostics, or reuse event-specific helpers from `events.rs` so the projector can mark dirty without producing a snapshot immediately.
- Modify `src-tauri/src/commands.rs`: remove `Lv1EventPayload`, stop emitting `lv1-event`, and change `spawn_shell_state_projector` to use a `tokio::select!` loop with a 100 ms interval.
- Modify `src-tauri/src/commands.rs` tests: add paused-time tests for coalescing, no raw `lv1-event`, and event-bus draining without wall-clock sleeps.
- Do not modify frontend files for this change. The frontend continues to listen to `app-status-changed`.

## Important Existing Constraints

- The worktree may contain unrelated unstaged changes in `src-tauri/src/app_state/*`, `src-tauri/src/commands.rs`, and frontend files. Do not revert them.
- Stage only files changed for this task when committing.
- Runtime safety remains in backend actors and command paths. The UI projection cadence is display-only.
- Do not add sleeps in tests. Use `#[tokio::test(start_paused = true)]`, `tokio::time::advance(...)`, and yield/poll loops only for already-scheduled work.

### Task 1: Remove Raw `lv1-event` Contract

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write the failing test**

Add this test inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/commands.rs`:

```rust
#[tokio::test(start_paused = true)]
async fn projector_does_not_emit_raw_lv1_event() {
    let app = mock_app();
    let handle = app.handle().clone();
    let raw_events = Arc::new(Mutex::new(0usize));
    let raw_events_for_listener = raw_events.clone();

    handle.listen_any("lv1-event", move |_| {
        *raw_events_for_listener.lock().unwrap() += 1;
    });

    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let event_bus = AppEventBus::default();
    let projector = spawn_shell_state_projector(
        handle,
        state,
        ActiveCommandBus::default(),
        generation,
        event_bus.subscribe(),
    );

    event_bus.publish(AppEvent::Lv1(Lv1Event::Connected));
    tokio::task::yield_now().await;
    tokio::time::advance(std::time::Duration::from_millis(100)).await;
    tokio::task::yield_now().await;

    assert_eq!(*raw_events.lock().unwrap(), 0);
    projector.abort();
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo nextest run -p advanced-show-control-tauri projector_does_not_emit_raw_lv1_event`

Expected: FAIL because the current projector emits `lv1-event` for LV1 events.

- [ ] **Step 3: Remove raw event emission and payload type**

In `src-tauri/src/commands.rs`, delete this block from `spawn_shell_state_projector`:

```rust
if let Err(err) = app.emit("lv1-event", &Lv1EventPayload::from(event)) {
    eprintln!("failed to emit lv1-event: {err}");
}
```

Delete the `Lv1EventPayload` struct near the tests:

```rust
#[derive(Debug, Clone, Serialize)]
struct Lv1EventPayload {
    kind: String,
    message: String,
}
```

Delete the entire `impl From<&Lv1Event> for Lv1EventPayload` at the end of `src-tauri/src/commands.rs`.

If `serde::Serialize` becomes unused at the top of `src-tauri/src/commands.rs`, remove that import only if the compiler reports it as unused.

- [ ] **Step 4: Run the test again**

Run: `cargo nextest run -p advanced-show-control-tauri projector_does_not_emit_raw_lv1_event`

Expected: PASS.

- [ ] **Step 5: Commit only this task**

Run: `git status --short`

Review the relevant diff: `git diff -- src-tauri/src/commands.rs`

Stage only intended files: `git add src-tauri/src/commands.rs`

Commit: `git commit -m "fix: remove raw lv1 event projection"`

### Task 2: Add No-Snapshot Runtime Event Application

**Files:**
- Modify: `src-tauri/src/app_state/events.rs`
- Modify: `src-tauri/src/app_state/projection.rs`
- Test: `src-tauri/src/app_state/events_tests.rs` or `src-tauri/src/commands.rs`

- [ ] **Step 1: Write the failing test**

Add this test to `src-tauri/src/commands.rs` tests. It proves LV1 state is applied before the cadence snapshot is emitted:

```rust
#[tokio::test(start_paused = true)]
async fn projector_applies_runtime_events_before_coalesced_snapshot() {
    let app = mock_app();
    let handle = app.handle().clone();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_for_listener = observed.clone();

    handle.listen_any("app-status-changed", move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload())
            .expect("app-status-changed payload should be valid JSON");
        observed_for_listener.lock().unwrap().push(payload);
    });

    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .begin_connection_for_generation(
            generation,
            Lv1StateSnapshot {
                connection: ConnectionStatus::Connected,
                scene: None,
                scene_list: Vec::new(),
                channels: vec![advanced_show_control::lv1::types::ChannelState {
                    group: 0,
                    channel: 1,
                    name: "Vocal".to_string(),
                    gain_db: -10.0,
                    muted: false,
                    pan: None,
                    balance: None,
                    width: None,
                }],
            },
        )
        .await
        .expect("current generation should accept initial state");

    let event_bus = AppEventBus::default();
    let projector = spawn_shell_state_projector(
        handle,
        state.clone(),
        ActiveCommandBus::default(),
        generation,
        event_bus.subscribe(),
    );

    event_bus.publish(AppEvent::Lv1(Lv1Event::FaderChanged {
        group: 0,
        channel: 1,
        gain_db: -3.0,
    }));
    tokio::task::yield_now().await;

    let snapshot_before_emit = state.snapshot_for_generation(generation).await.unwrap();
    assert_eq!(snapshot_before_emit.channel_count, 1);

    tokio::time::advance(std::time::Duration::from_millis(100)).await;
    tokio::task::yield_now().await;

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0]["channelCount"], 1);
    projector.abort();
}
```

- [ ] **Step 2: Run the failing test**

Run: `cargo nextest run -p advanced-show-control-tauri projector_applies_runtime_events_before_coalesced_snapshot`

Expected: FAIL because the current projector emits a snapshot immediately instead of waiting for the tick.

- [ ] **Step 3: Add no-snapshot application methods**

In `src-tauri/src/app_state/events.rs`, add methods beside the existing snapshot-returning methods:

```rust
pub async fn apply_lv1_event_without_snapshot_for_generation(
    &self,
    generation: u64,
    event: &Lv1Event,
) -> bool {
    let mut inner = self.inner.lock().await;
    if inner.generation != generation {
        return false;
    }

    apply_lv1_event_locked(self, &mut inner, event).await
}

pub async fn apply_fade_event_without_snapshot_for_generation(
    &self,
    generation: u64,
    event: &FadeEvent,
) -> bool {
    let mut inner = self.inner.lock().await;
    if inner.generation != generation {
        return false;
    }

    apply_fade_event_locked(&mut inner, event);
    true
}
```

Refactor the existing `apply_lv1_event_for_generation` so the event mutation logic lives in one helper. The helper must return `false` only for stale generation and `true` when the event was applied. Preserve the existing scene-list reconciliation behavior and lock ordering. The final snapshot-returning method should call the helper, then `snapshot_for_generation(generation).await`.

The intended shape is:

```rust
pub async fn apply_lv1_event_for_generation(
    &self,
    generation: u64,
    event: &Lv1Event,
) -> Option<AppViewState> {
    if !self
        .apply_lv1_event_without_snapshot_for_generation(generation, event)
        .await
    {
        return None;
    }

    self.snapshot_for_generation(generation).await
}
```

Do not hold `inner` while calling `snapshot_for_generation`.

- [ ] **Step 4: Add no-snapshot projection helper**

In `src-tauri/src/app_state/projection.rs`, add a helper that mutates for `SceneRecall` and `Diagnostic` without snapshot creation:

```rust
pub async fn project_event_without_snapshot_for_generation(
    &self,
    generation: u64,
    event: &AppEvent,
) -> bool {
    match event {
        AppEvent::SceneRecall(scene_recall_event) => self
            .apply_scene_recall_event_without_snapshot_for_generation(generation, scene_recall_event)
            .await,
        AppEvent::Diagnostic { source, message } => {
            let log_message = format!("{source}: {message}");
            self.push_log_for_generation(generation, LogSource::App, LogSeverity::Warning, log_message)
                .await
        }
        _ => false,
    }
}
```

If the exact helper names for logs or scene recall do not exist yet, add minimal generation-checked helpers in the same module or adjacent app-state module. They should lock `inner`, verify `inner.generation == generation`, mutate logs or recall state, return `true`, and not call `snapshot()`.

- [ ] **Step 5: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control-tauri app_state commands::tests::projector_applies_runtime_events_before_coalesced_snapshot`

Expected: PASS for the new test and existing app-state tests.

- [ ] **Step 6: Commit only this task**

Run: `git status --short`

Review relevant diffs: `git diff -- src-tauri/src/app_state/events.rs src-tauri/src/app_state/projection.rs src-tauri/src/commands.rs`

Stage only intended files: `git add src-tauri/src/app_state/events.rs src-tauri/src/app_state/projection.rs src-tauri/src/commands.rs`

Commit: `git commit -m "refactor: apply shell events without per-event snapshots"`

### Task 3: Coalesce Projector Snapshots at 10 Hz

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Write the failing coalescing test**

Add this test to `src-tauri/src/commands.rs` tests:

```rust
#[tokio::test(start_paused = true)]
async fn projector_coalesces_runtime_updates_to_ten_hz() {
    let app = mock_app();
    let handle = app.handle().clone();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_for_listener = observed.clone();

    handle.listen_any("app-status-changed", move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload())
            .expect("app-status-changed payload should be valid JSON");
        observed_for_listener.lock().unwrap().push(payload);
    });

    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let event_bus = AppEventBus::default();
    let projector = spawn_shell_state_projector(
        handle,
        state,
        ActiveCommandBus::default(),
        generation,
        event_bus.subscribe(),
    );

    for gain_db in [-10.0, -9.0, -8.0, -7.0, -6.0] {
        event_bus.publish(AppEvent::Lv1(Lv1Event::FaderChanged {
            group: 0,
            channel: 1,
            gain_db,
        }));
    }
    tokio::task::yield_now().await;
    assert_eq!(observed.lock().unwrap().len(), 0);

    tokio::time::advance(std::time::Duration::from_millis(100)).await;
    tokio::task::yield_now().await;
    assert_eq!(observed.lock().unwrap().len(), 1);

    tokio::time::advance(std::time::Duration::from_millis(100)).await;
    tokio::task::yield_now().await;
    assert_eq!(observed.lock().unwrap().len(), 1);

    projector.abort();
}
```

- [ ] **Step 2: Write the event-draining test**

Add this test to `src-tauri/src/commands.rs` tests:

```rust
#[tokio::test(start_paused = true)]
async fn projector_drains_events_while_waiting_for_projection_tick() {
    let app = mock_app();
    let handle = app.handle().clone();
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observed_for_listener = observed.clone();

    handle.listen_any("app-status-changed", move |event| {
        let payload: serde_json::Value = serde_json::from_str(event.payload())
            .expect("app-status-changed payload should be valid JSON");
        observed_for_listener.lock().unwrap().push(payload);
    });

    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    let event_bus = AppEventBus::new(4);
    let projector = spawn_shell_state_projector(
        handle,
        state,
        ActiveCommandBus::default(),
        generation,
        event_bus.subscribe(),
    );

    for gain_db in 0..20 {
        event_bus.publish(AppEvent::Lv1(Lv1Event::FaderChanged {
            group: 0,
            channel: 1,
            gain_db: gain_db as f32,
        }));
        tokio::task::yield_now().await;
    }

    tokio::time::advance(std::time::Duration::from_millis(100)).await;
    tokio::task::yield_now().await;

    let observed = observed.lock().unwrap();
    assert_eq!(observed.len(), 1);
    let logs = observed[0]["logs"].as_array().unwrap();
    assert!(!logs.iter().any(|entry| entry["message"]
        .as_str()
        .unwrap_or_default()
        .contains("shell-state-projector event subscriber lagged")));

    projector.abort();
}
```

- [ ] **Step 3: Run the failing tests**

Run: `cargo nextest run -p advanced-show-control-tauri projector_coalesces_runtime_updates_to_ten_hz projector_drains_events_while_waiting_for_projection_tick`

Expected: FAIL until the projector loop is changed.

- [ ] **Step 4: Implement the 10 Hz projector loop**

In `src-tauri/src/commands.rs`, add a constant near `emit_snapshot`:

```rust
const SHELL_PROJECTION_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
```

Change the body of `spawn_shell_state_projector` so the spawned task uses an interval and dirty flag:

```rust
tokio::spawn(async move {
    let _ = crate::diagnostics::append_diagnostic(
        &diagnostics_path,
        "tauri-shell",
        &format!("projector started generation={generation}"),
    );

    let mut projection_tick = tokio::time::interval(SHELL_PROJECTION_INTERVAL);
    projection_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = false;

    loop {
        tokio::select! {
            event = events.recv() => {
                match event {
                    Ok(app_event) => {
                        if apply_projector_event(
                            &app,
                            &state,
                            &active_command_bus,
                            generation,
                            &diagnostics_path,
                            &app_event,
                        ).await {
                            dirty = true;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        let log_message = format!(
                            "shell-state-projector event subscriber lagged and missed {count} events"
                        );
                        state.append_log(LogSeverity::Warning, log_message).await;
                        dirty = true;
                        log_lagged_subscriber("shell-state-projector", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = projection_tick.tick() => {
                if dirty {
                    if let Some(snapshot) = state.snapshot_for_generation(generation).await {
                        emit_snapshot(&app, &snapshot);
                    }
                    dirty = false;
                }
            }
        }
    }
})
```

Add this helper below `spawn_shell_state_projector`:

```rust
async fn apply_projector_event<R: Runtime>(
    app: &AppHandle<R>,
    state: &ShellState,
    active_command_bus: &ActiveCommandBus,
    generation: u64,
    diagnostics_path: &std::path::Path,
    app_event: &AppEvent,
) -> bool {
    match app_event {
        AppEvent::Lv1(event) => {
            if let Lv1Event::SceneListChanged(scenes) = event {
                let message = state
                    .show
                    .scene_reconciliation_diagnostic(scenes.clone())
                    .await;
                let _ = crate::diagnostics::append_diagnostic(
                    diagnostics_path,
                    "show-state",
                    &message,
                );
            }

            let applied = state
                .apply_lv1_event_without_snapshot_for_generation(generation, event)
                .await;

            if applied && matches!(event, Lv1Event::Disconnected { .. }) {
                state
                    .clear_runtime_handles_for_generation(generation, active_command_bus)
                    .await;
            }

            applied
        }
        AppEvent::Fade(event) => state
            .apply_fade_event_without_snapshot_for_generation(generation, event)
            .await,
        AppEvent::CommandFailed { command, message } => {
            let log_message = format!("command failed: {command}: {message}");
            state.append_log(LogSeverity::Error, log_message).await;
            true
        }
        AppEvent::Diagnostic { source, message } => {
            if let Err(err) = crate::diagnostics::append_diagnostic(diagnostics_path, source, message) {
                eprintln!("failed to write diagnostic log: {err}");
            }
            state
                .project_event_without_snapshot_for_generation(generation, app_event)
                .await
        }
        AppEvent::SceneRecall(_) => state
            .project_event_without_snapshot_for_generation(generation, app_event)
            .await,
    }
}
```

If `app` is unused in the helper, remove it from the helper signature and call site.

- [ ] **Step 5: Run the projector tests**

Run: `cargo nextest run -p advanced-show-control-tauri projector_`

Expected: PASS for all projector tests.

- [ ] **Step 6: Commit only this task**

Run: `git status --short`

Review relevant diff: `git diff -- src-tauri/src/commands.rs src-tauri/src/app_state/events.rs src-tauri/src/app_state/projection.rs`

Stage only intended files: `git add src-tauri/src/commands.rs src-tauri/src/app_state/events.rs src-tauri/src/app_state/projection.rs`

Commit: `git commit -m "fix: coalesce shell projection snapshots"`

### Task 4: Update Existing Projector Tests for 10 Hz Semantics

**Files:**
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Update tests that expect immediate projector emission**

In existing tests such as `initial_connection_snapshot_is_emitted_before_projector_events`, `diagnostic_event_updates_shell_state_log_and_snapshot`, and `scene_recall_events_emit_fresh_app_status_snapshot`, add `start_paused = true` to the `#[tokio::test]` attribute if missing and explicitly advance time by 100 ms before expecting the projector-emitted snapshot:

```rust
tokio::time::advance(std::time::Duration::from_millis(100)).await;
tokio::task::yield_now().await;
```

Do not add `tokio::time::sleep(...)`.

- [ ] **Step 2: Rename tests whose names imply immediate emission**

Rename tests to match 10 Hz semantics. Use these names:

```rust
initial_connection_snapshot_is_emitted_before_coalesced_projector_events
diagnostic_event_updates_shell_state_log_and_coalesced_snapshot
scene_recall_events_emit_coalesced_app_status_snapshot
```

- [ ] **Step 3: Run updated tests**

Run: `cargo nextest run -p advanced-show-control-tauri initial_connection_snapshot_is_emitted_before_coalesced_projector_events diagnostic_event_updates_shell_state_log_and_coalesced_snapshot scene_recall_events_emit_coalesced_app_status_snapshot`

Expected: PASS.

- [ ] **Step 4: Commit only this task**

Run: `git status --short`

Review relevant diff: `git diff -- src-tauri/src/commands.rs`

Stage only intended files: `git add src-tauri/src/commands.rs`

Commit: `git commit -m "test: update shell projector cadence expectations"`

### Task 5: Verification and Documentation Cleanup

**Files:**
- Modify: `docs/roadmap.md` only if the roadmap item should be marked complete by the project owner.

- [ ] **Step 1: Run formatting check**

Run: `cargo fmt --all -- --check`

Expected: PASS. If it fails, run `cargo fmt --all`, review the diff, and include formatting changes in the current task commit.

- [ ] **Step 2: Run targeted Rust tests**

Run: `cargo nextest run -p advanced-show-control-tauri commands::tests app_state`

Expected: PASS.

- [ ] **Step 3: Run broader Rust verification**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

Run: `cargo nextest run --workspace`

Expected: PASS.

- [ ] **Step 4: Inspect final status and diff**

Run: `git status --short`

Run: `git diff --stat`

Confirm only intended files remain unstaged. Do not revert unrelated user changes.

- [ ] **Step 5: Commit verification/doc updates if needed**

If Task 5 changed files, stage only those files and commit:

```bash
git add docs/roadmap.md
git commit -m "docs: note shell projection efficiency completion"
```

If Task 5 did not change files, do not create an empty commit.

## Self-Review

- Spec coverage: raw `lv1-event` removal is covered in Task 1; 10 Hz coalescing and continuous bus draining are covered in Task 3; no wall-clock sleeps are covered in Tasks 2-4; backend-only safety decision-making is preserved by not changing runtime safety paths.
- Placeholder scan: no task contains TBD/TODO/fill-in placeholders. Steps include exact files, code shapes, commands, and expected results.
- Type consistency: plan uses current project types `ShellState`, `AppEvent`, `Lv1Event`, `FadeEvent`, `AppViewState`, `ActiveCommandBus`, `Lv1StateSnapshot`, and existing Tauri test helpers.
