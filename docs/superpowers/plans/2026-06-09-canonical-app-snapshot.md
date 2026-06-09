# Canonical App Snapshot Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure every returned `AppViewState` is built from shell state plus the real `ShowSnapshot`, so shell transitions cannot accidentally erase scene configs.

**Architecture:** Keep `ShellState::snapshot()` and `ShellState::snapshot_for_generation()` as the canonical UI projection paths. Remove `snapshot_from_inner()` as an `AppViewState` constructor, keep `snapshot_inner()` only as shell-owned intermediate extraction, and make production and test helpers drop the shell lock before returning full snapshots.

**Tech Stack:** Rust, Tokio, Tauri shell crate `advanced-show-control-tauri`, existing `ShowStateHandle` and app-state tests.

---

## File Structure

- Modify: `src-tauri/src/app_state/shell.rs`
  - Replace shell-only `AppViewState` returns with `self.snapshot().await` or `self.snapshot_for_generation(generation).await`.
  - Remove `snapshot_from_inner()`.
  - Keep `snapshot_inner()` and `snapshot_from_parts()` as internal building blocks.
  - Update tests that directly used `snapshot_from_inner()` so they use `ShellState` and canonical snapshots.
- Modify: `src-tauri/src/app_state/events.rs`
  - Remove the `snapshot_from_inner` import.
  - Make fade event projection and begin-connection helpers stop returning shell-only snapshots.
- Modify: `src-tauri/src/app_state/events_tests.rs`
  - Add coverage that LV1 disconnect event projection preserves existing show scene configs.
  - Keep the existing scene-list no-deadlock coverage.

No new modules are needed. Do not change frontend files, LV1 actor logic, fade logic, scene recall policy, show-file DTOs, or diagnostics.

---

### Task 1: Add Failing Shell Transition Tests

**Files:**
- Modify: `src-tauri/src/app_state/shell.rs`

- [ ] **Step 1: Add a local test helper for storing one scene config**

In `src-tauri/src/app_state/shell.rs`, inside `mod tests`, add this helper near the other test helpers:

```rust
async fn store_intro_scene_config(state: &ShellState) {
    state
        .show
        .store_scene_config(
            "1::Intro".to_string(),
            vec![ChannelInfo {
                group: 0,
                channel: 1,
                name: "Lead".to_string(),
                gain_db: -6.0,
                muted: false,
            }],
        )
        .await
        .unwrap()
        .unwrap();
}
```

- [ ] **Step 2: Replace duplicate setup in the existing connected-identity test**

In `established_connected_identity_snapshot_includes_show_configs`, replace the inline `state.show.store_scene_config(...).await.unwrap().unwrap();` block with:

```rust
store_intro_scene_config(&state).await;
```

- [ ] **Step 3: Add tests for shell transitions that currently return shell-only snapshots**

Add these tests after `established_connected_identity_snapshot_includes_show_configs`:

```rust
#[tokio::test]
async fn pending_identity_snapshot_includes_show_configs() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    store_intro_scene_config(&state).await;

    let snapshot = state
        .set_pending_lv1_identity_for_generation(
            generation,
            Some(crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1-FOH".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            }),
        )
        .await
        .expect("current generation should set pending identity");

    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
}

#[tokio::test]
async fn connect_failure_snapshot_includes_show_configs() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    store_intro_scene_config(&state).await;

    let snapshot = state
        .fail_connect_for_generation(generation, "LV1 did not connect")
        .await
        .expect("current generation failure should apply");

    assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
}

#[tokio::test]
async fn discovered_systems_snapshot_includes_show_configs() {
    let state = ShellState::default();
    store_intro_scene_config(&state).await;

    let snapshot = state
        .set_discovered_lv1_systems(vec![crate::connection_state::DiscoveredLv1System {
            identity: crate::connection_state::Lv1SystemIdentity {
                uuid: Some("uuid-1".to_string()),
                host: Some("LV1-FOH".to_string()),
                address: "192.168.1.35".to_string(),
                port: 50000,
            },
            latency_ms: Some(10),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        }])
        .await;

    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
}
```

- [ ] **Step 4: Run shell tests and verify failure**

Run with the user-requested 20s timeout:

```bash
cargo test -p advanced-show-control-tauri app_state::shell::tests
```

Expected: FAIL. At least `pending_identity_snapshot_includes_show_configs`, `connect_failure_snapshot_includes_show_configs`, or `discovered_systems_snapshot_includes_show_configs` should fail because the returned snapshot has `scene_configs.len() == 0`.

- [ ] **Step 5: Commit failing tests only**

Do not commit if the tests unexpectedly pass. If they fail for the expected empty `scene_configs` reason, commit:

```bash
git add src-tauri/src/app_state/shell.rs
git commit -m "test: cover canonical shell snapshots"
```

---

### Task 2: Route Shell Snapshot Returns Through Canonical Builders

**Files:**
- Modify: `src-tauri/src/app_state/shell.rs`

- [ ] **Step 1: Update test-only identity setters**

Replace `set_connected_lv1_identity` with:

```rust
#[cfg(test)]
pub async fn set_connected_lv1_identity(
    &self,
    identity: Option<Lv1SystemIdentity>,
) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.connected_lv1_identity = identity;
    refresh_discovered_statuses(&mut inner);
    drop(inner);
    self.snapshot().await
}
```

Replace `set_pending_lv1_identity` with:

```rust
#[cfg(test)]
pub async fn set_pending_lv1_identity(
    &self,
    identity: Option<Lv1SystemIdentity>,
) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.pending_lv1_identity = identity;
    refresh_discovered_statuses(&mut inner);
    drop(inner);
    self.snapshot().await
}
```

- [ ] **Step 2: Update generation-aware pending identity**

Replace `set_pending_lv1_identity_for_generation` with:

```rust
pub async fn set_pending_lv1_identity_for_generation(
    &self,
    generation: u64,
    identity: Option<Lv1SystemIdentity>,
) -> Option<AppViewState> {
    let mut inner = self.inner.lock().await;
    if inner.generation != generation {
        return None;
    }

    inner.pending_lv1_identity = identity;
    refresh_discovered_statuses(&mut inner);
    drop(inner);
    self.snapshot_for_generation(generation).await
}
```

- [ ] **Step 3: Update connection failure paths**

Replace the return path at the end of `fail_connect_for_generation` with:

```rust
inner.push_log(LogSource::App, LogSeverity::Warning, message.into());
drop(inner);
self.snapshot_for_generation(generation).await
```

Replace the return path at the end of `fail_reconnect_for_generation` with:

```rust
inner.push_log(LogSource::App, LogSeverity::Warning, message.into());
drop(inner);
self.snapshot_for_generation(generation).await
```

- [ ] **Step 4: Update discovery and reconnect test helpers**

Replace `set_discovered_lv1_systems` with:

```rust
pub async fn set_discovered_lv1_systems(
    &self,
    systems: Vec<DiscoveredLv1System>,
) -> AppViewState {
    let mut inner = self.inner.lock().await;
    inner.discovered_lv1_systems = systems;
    refresh_discovered_statuses(&mut inner);
    drop(inner);
    self.snapshot().await
}
```

Replace `set_reconnect_active` with:

```rust
#[cfg(test)]
pub async fn set_reconnect_active(&self, active: bool) -> AppViewState {
    let mut inner = self.inner.lock().await;
    if active {
        inner.reconnect_state.attempt = inner.reconnect_state.attempt.saturating_add(1);
    }
    inner.reconnect_state.active = active;
    drop(inner);
    self.snapshot().await
}
```

- [ ] **Step 5: Simplify reconnect timeout to canonical snapshot**

Replace `reconnect_timed_out` with:

```rust
pub async fn reconnect_timed_out(&self, attempt: u64) -> AppViewState {
    let mut inner = self.inner.lock().await;
    if inner.reconnect_state.active && inner.reconnect_state.attempt == attempt {
        inner.generation = inner.generation.saturating_add(1);
        inner.reconnect_state.active = false;
    }
    drop(inner);
    self.snapshot().await
}
```

- [ ] **Step 6: Remove the obsolete incomplete snapshot constructor**

Delete this function from `src-tauri/src/app_state/shell.rs`:

```rust
pub(super) fn snapshot_from_inner(inner: &ShellInner) -> AppViewState {
    snapshot_from_parts(snapshot_inner(inner), ShowSnapshot::empty())
}
```

- [ ] **Step 7: Run shell tests and verify pass**

Run with a 20s timeout:

```bash
cargo test -p advanced-show-control-tauri app_state::shell::tests
```

Expected: PASS for all shell tests.

- [ ] **Step 8: Commit shell implementation**

```bash
git add src-tauri/src/app_state/shell.rs
git commit -m "fix: use canonical shell snapshots"
```

---

### Task 3: Update Event Projection To Avoid Shell-Only Snapshots

**Files:**
- Modify: `src-tauri/src/app_state/events.rs`
- Modify: `src-tauri/src/app_state/events_tests.rs`

- [ ] **Step 1: Add a failing LV1 disconnect preservation test**

In `src-tauri/src/app_state/events_tests.rs`, add this test after `lv1_disconnected_event_enters_reconnect_state`:

```rust
#[tokio::test]
async fn lv1_disconnected_event_snapshot_includes_show_configs() {
    let state = ShellState::default();
    let (generation, _) = state.begin_connecting().await;
    state
        .show
        .store_scene_config(
            "1::Intro".to_string(),
            vec![ChannelInfo {
                group: 0,
                channel: 1,
                name: "Lead".to_string(),
                gain_db: -6.0,
                muted: false,
            }],
        )
        .await
        .unwrap()
        .unwrap();

    let snapshot = state
        .apply_lv1_event_for_generation(generation, &Lv1Event::Disconnected)
        .await
        .expect("disconnect should apply to current generation");

    assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scene_id, "1::Intro");
}
```

- [ ] **Step 2: Run event tests and verify failure**

Run with a 20s timeout:

```bash
cargo test -p advanced-show-control-tauri app_state::events_tests
```

Expected: FAIL. `lv1_disconnected_event_snapshot_includes_show_configs` should fail because event projection currently returns a shell-only snapshot through `snapshot_from_inner`.

- [ ] **Step 3: Remove the obsolete import**

In `src-tauri/src/app_state/events.rs`, change:

```rust
use super::shell::{
    MAX_LOGS, ShellInner, ShellState, current_timestamp, refresh_discovered_statuses,
    snapshot_from_inner,
};
```

to:

```rust
use super::shell::{
    MAX_LOGS, ShellInner, ShellState, current_timestamp, refresh_discovered_statuses,
};
```

- [ ] **Step 4: Update LV1 event projection return path**

At the end of `apply_lv1_event_for_generation`, replace:

```rust
snapshot_from_inner(inner)
```

with:

```rust
drop(inner);
self.snapshot_for_generation(generation).await
```

The function already returns `Option<AppViewState>`, so this expression should be the final expression.

- [ ] **Step 5: Update fade event projection return path**

At the end of `apply_fade_event_for_generation`, replace:

```rust
snapshot_from_inner(inner)
```

with:

```rust
drop(inner);
self.snapshot_for_generation(generation).await
```

- [ ] **Step 6: Update `apply_begin_connection` to mutate only**

Change the helper signature from:

```rust
fn apply_begin_connection(inner: &mut ShellInner, snapshot: Lv1StateSnapshot) -> AppViewState {
```

to:

```rust
fn apply_begin_connection(inner: &mut ShellInner, snapshot: Lv1StateSnapshot) {
```

Delete the final line in that helper:

```rust
snapshot_from_inner(inner)
```

The call sites already ignore the returned value with `let _ = apply_begin_connection(...)`, so after this change they can remain as:

```rust
apply_begin_connection(&mut inner, snapshot);
```

- [ ] **Step 7: Run event tests and verify pass**

Run with a 20s timeout:

```bash
cargo test -p advanced-show-control-tauri app_state::events_tests
```

Expected: PASS for all event tests.

- [ ] **Step 8: Commit event implementation**

```bash
git add src-tauri/src/app_state/events.rs src-tauri/src/app_state/events_tests.rs
git commit -m "fix: use canonical event snapshots"
```

---

### Task 4: Remove Obsolete Direct Snapshot Tests

**Files:**
- Modify: `src-tauri/src/app_state/shell.rs`

- [ ] **Step 1: Replace direct `snapshot_from_inner` mapping tests with canonical `ShellState` tests**

Delete `snapshot_maps_lv1_scene_and_counts` and add this async test in its place:

```rust
#[tokio::test]
async fn snapshot_maps_lv1_scene_and_counts() {
    let state = ShellState::default();

    let snapshot = state
        .begin_connection(Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: Some(SceneState {
                index: 3,
                name: "Verse".to_string(),
            }),
            scene_list: vec![SceneListEntry {
                index: 3,
                name: "Verse".to_string(),
            }],
            channels: vec![ChannelInfo {
                group: 0,
                channel: 0,
                name: "Lead".to_string(),
                gain_db: -6.0,
                muted: false,
            }],
        })
        .await;

    assert_eq!(snapshot.connection, AppConnectionState::Connected);
    assert_eq!(snapshot.current_scene.unwrap().name, "Verse");
    assert_eq!(snapshot.scene_count, 1);
    assert_eq!(snapshot.channel_count, 1);
    assert_eq!(snapshot.channels.len(), 1);
    assert_eq!(snapshot.channels[0].group, 0);
    assert_eq!(snapshot.channels[0].channel, 0);
    assert_eq!(snapshot.channels[0].name, "Lead");
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.scene_configs[0].scene_id, "3::Verse");
    assert_eq!(snapshot.selected_scene_id, Some("3::Verse".to_string()));
}
```

- [ ] **Step 2: Replace direct discovered-system snapshot test**

Delete `snapshot_includes_discovered_lv1_systems_and_reconnect_state` and add this async test in its place:

```rust
#[tokio::test]
async fn snapshot_includes_discovered_lv1_systems_and_reconnect_state() {
    let state = ShellState::default();
    let identity = crate::connection_state::Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        host: Some("LV1-FOH".to_string()),
        address: "192.168.1.35".to_string(),
        port: 50000,
    };

    state
        .set_connected_lv1_identity(Some(identity.clone()))
        .await;
    state.set_reconnect_active(true).await;
    let snapshot = state
        .set_discovered_lv1_systems(vec![crate::connection_state::DiscoveredLv1System {
            identity,
            latency_ms: Some(12),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        }])
        .await;

    assert_eq!(snapshot.discovered_lv1_systems.len(), 1);
    assert_eq!(
        snapshot.discovered_lv1_systems[0].identity.address,
        "192.168.1.35"
    );
    assert_eq!(
        snapshot.connected_lv1_identity.unwrap().address,
        "192.168.1.35"
    );
    assert!(snapshot.reconnect.active);
}
```

- [ ] **Step 3: Confirm `snapshot_from_inner` has no references**

Run:

```bash
rg "snapshot_from_inner" src-tauri/src/app_state
```

Expected: no matches.

- [ ] **Step 4: Run shell and event tests**

Run each command with a 20s timeout:

```bash
cargo test -p advanced-show-control-tauri app_state::shell::tests
cargo test -p advanced-show-control-tauri app_state::events_tests
```

Expected: both commands PASS.

- [ ] **Step 5: Commit obsolete-test cleanup**

```bash
git add src-tauri/src/app_state/shell.rs
git commit -m "test: remove shell-only snapshot coverage"
```

---

### Task 5: Targeted Verification

**Files:**
- No code changes expected unless verification finds a bug.

- [ ] **Step 1: Run command tests**

Run with a 20s timeout:

```bash
cargo test -p advanced-show-control-tauri commands::tests
```

Expected: PASS.

- [ ] **Step 2: Run formatting check**

Run with a 20s timeout:

```bash
cargo fmt --all -- --check
```

Expected: PASS with no formatting diff required.

- [ ] **Step 3: Run clippy**

Run with a 20s timeout:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: PASS. If this exceeds 20 seconds because the workspace is large, report the timeout and rerun only if the user approves a longer timeout.

- [ ] **Step 4: Inspect final status and diff**

Run:

```bash
git status --short
git diff --stat
```

Expected: clean working tree if all task commits were made. If any verification-only fixes were needed, only intended files should be dirty.

---

## Self-Review

- Spec coverage: The plan covers canonical `AppViewState` construction, removes `snapshot_from_inner`, preserves generation guards by using `snapshot_for_generation`, avoids holding the shell lock while awaiting show state, updates tests to use production paths, and verifies scene configs survive shell and event transitions.
- Placeholder scan: No placeholder markers, deferred implementation, or unspecified test steps remain.
- Type consistency: Function names and types match the current code: `ShellState`, `ShellInner`, `AppViewState`, `ShowSnapshot`, `Lv1StateSnapshot`, `ConnectionStatus`, `SceneState`, `SceneListEntry`, `ChannelInfo`, and `Lv1SystemIdentity`.
