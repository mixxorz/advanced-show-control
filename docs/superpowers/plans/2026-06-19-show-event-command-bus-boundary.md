# Show Event Command Bus Boundary Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement approved architecture spec phase 7 by adding `AppEvent::Show(ShowEvent)` and removing `AppEventBus` ownership from `AppCommandBus`.

**Architecture:** This phase makes `show/` own the event-bus reference it needs to publish show/app facts, replaces the current placeholder show event with an explicit `ShowEvent` contract, wires that event into the app-wide facts bus, and makes `AppCommandBus` a pure command/query target holder. Existing direct Tauri command behavior remains in place, but current show-state mutations publish typed show snapshot-change facts so the future projector can observe show/app changes through `AppEventBus` instead of command-bus coupling.

**Tech Stack:** Rust 2024, Tokio broadcast channels, Tauri 2, cargo-nextest, existing `AppEventBus`, existing `AppCommandBus`, existing `ShowStateHandle`.

## Global Constraints

- Follow approved architecture spec phase 7 only.
- Do not route low-risk show/app commands through `AppCommandBus` in this plan; that is phase 8.
- Do not change LV1 protocol behavior.
- Do not weaken lockout, exact scene identity validation, generation guards, disconnect behavior, manual override, abort, or overlap safety behavior.
- Do not route logs through `AppEventBus`.
- Do not remove `ShellState`, `ActiveCommandBus`, direct emits, or command-return snapshots in this plan.
- Preserve existing Tauri command names and frontend command return payloads.
- Preserve current `app-status-changed` behavior until the projector-only phase.
- Preserve `lv1-probe` as a supported binary.
- Every non-log show mutation that affects `AppViewState` must publish `AppEvent::Show(ShowEvent::SnapshotChanged { reason })`.
- Every task must leave the app compiling and tests passing for the touched area.

---

## File Structure

- Modify: `src-tauri/src/runtime/events.rs`
  - Adds `AppEvent::Show(ShowEvent)`.
  - Adds tests proving show events travel through `AppEventBus` and remain safe without subscribers.
- Modify: `src-tauri/src/show/events.rs`
  - Replaces the placeholder `ShowEvent` with the phase-7 event contract.
  - Defines `ShowEvent::SnapshotChanged { reason: ShowSnapshotChange }`.
  - Defines `ShowSnapshotChange` variants for current show mutations that affect `AppViewState`.
- Modify: `src-tauri/src/show/handle.rs`
  - Stores `AppEventBus` in `ShowStateHandle`.
  - Publishes `AppEvent::Show(ShowEvent::SnapshotChanged { reason })` after successful show mutations.
  - Does not publish when a mutation returns `Ok(false)` or otherwise makes no state change.
- Modify: `src-tauri/src/app_state/shell.rs`
  - Constructs `ShowStateHandle` with the app event bus.
  - Adds `ShellState::new(event_bus: AppEventBus) -> Self` for production setup so show events publish to the shared runtime bus.
- Modify: `src-tauri/src/runtime/commands.rs`
  - Removes `AppEventBus` from `AppCommandBus::new`.
  - Updates tests for the new `ShowStateHandle` constructor and the parameterless command-bus constructor.
- Modify: `src-tauri/src/commands.rs`
  - Updates runtime setup and tests to use `AppCommandBus::new()`.
  - Keeps the existing separate `event_bus` variable for runtime actors, show state, shell projection, and direct emit behavior.
- Modify: `src-tauri/src/lifecycle/mod.rs`
  - Updates lifecycle tests to construct a command bus without an event bus.
- Modify: `src-tauri/src/fade/actor.rs`
  - Updates tests and helpers to construct a command bus without an event bus while still passing `event_bus` to publishers/subscribers.
- Modify: `src-tauri/src/scene_recall/actor.rs`
  - Updates tests and helpers to construct a command bus without an event bus while preserving the event bus used for recall subscriptions/publications and show state.
- Modify: `src-tauri/src/bin/lv1-probe.rs`
  - Updates developer CLI runtime setup to construct a command bus without an event bus.
- Modify: `src-tauri/tests/runtime_bus.rs`
  - Updates integration coverage for the command bus/event bus boundary.
- Modify: `src-tauri/tests/fade_engine.rs`
  - Updates integration tests to construct a command bus without an event bus.
- Modify: `docs/architecture.md`
  - Documents that `AppEventBus` carries show/app facts and `AppCommandBus` no longer owns or receives the event bus.

---

### Task 1: Define And Wire `ShowEvent` Into `AppEventBus`

**Files:**
- Modify: `src-tauri/src/runtime/events.rs`
- Verify existing: `src-tauri/src/show/events.rs`

**Interfaces:**
- Produces: `crate::show::events::ShowEvent`
- Produces: `crate::show::events::ShowSnapshotChange`
- Produces: `AppEvent::Show(ShowEvent)`

- [ ] **Step 1: Replace the placeholder show event type**

Replace the entire contents of `src-tauri/src/show/events.rs` with this phase-7 event contract:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowEvent {
    SnapshotChanged { reason: ShowSnapshotChange },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowSnapshotChange {
    CueScene,
    Lockout,
    SceneDuration,
    SceneScopeFaders,
    SceneScopePan,
    ChannelScope,
    AllChannelsScope,
    StoreSceneConfig,
    SceneListReconciled,
    SnapshotReplaced,
    Cleared,
}
```

Keep `ShowEvent` broad. The current projector phase will only need to know that show snapshot data changed and pull a fresh show snapshot later, but `ShowSnapshotChange` keeps the fact explicit enough for diagnostics, tests, and future subscribers.

- [ ] **Step 2: Add failing show-event bus tests**

In `src-tauri/src/runtime/events.rs`, add this import near the existing imports:

```rust
use crate::show::events::{ShowEvent, ShowSnapshotChange};
```

Then add these tests to the existing `#[cfg(test)] mod tests`:

```rust
    #[tokio::test]
    async fn subscriber_receives_published_show_event() {
        let bus = AppEventBus::new(16);
        let mut rx = bus.subscribe();

        bus.publish(AppEvent::Show(ShowEvent::SnapshotChanged {
            reason: ShowSnapshotChange::Lockout,
        }));

        let event = rx.recv().await.unwrap();
        assert!(matches!(
            event,
            AppEvent::Show(ShowEvent::SnapshotChanged {
                reason: ShowSnapshotChange::Lockout
            })
        ));
    }

    #[tokio::test]
    async fn show_fact_publish_without_subscribers_is_safe() {
        let bus = AppEventBus::new(1);

        let sent = bus.publish(AppEvent::Show(ShowEvent::SnapshotChanged {
            reason: ShowSnapshotChange::SnapshotReplaced,
        }));

        assert_eq!(sent, 0);
    }
```

- [ ] **Step 3: Run the failing event tests**

Run: `cargo nextest run -p advanced-show-control runtime::events::tests::subscriber_receives_published_show_event runtime::events::tests::show_fact_publish_without_subscribers_is_safe`

Expected: FAIL because `AppEvent::Show` does not exist yet.

- [ ] **Step 4: Add the app-wide show event variant**

In `src-tauri/src/runtime/events.rs`, update the imports to include `ShowEvent`:

```rust
use tokio::sync::broadcast;

use crate::fade::events::FadeEvent;
use crate::lv1::events::Lv1Event;
use crate::show::events::ShowEvent;
```

Then update `AppEvent` to include `Show`:

```rust
#[derive(Debug, Clone)]
pub enum AppEvent {
    Lv1(Lv1Event),
    Fade(FadeEvent),
    SceneRecall(crate::scene_recall::events::SceneRecallEvent),
    Show(ShowEvent),
}
```

- [ ] **Step 5: Update current event consumers that match exhaustively**

In `src-tauri/src/commands.rs`, update the projection event handler match so show events are ignored by the current shell projector in this phase:

```rust
AppEvent::Show(_) => ProjectionOutcome::Ignored,
```

Keep the existing `Lv1`, `Fade`, and `SceneRecall` handling unchanged.

- [ ] **Step 6: Run the event tests again**

Run: `cargo nextest run -p advanced-show-control runtime::events::tests::subscriber_receives_published_show_event runtime::events::tests::show_fact_publish_without_subscribers_is_safe`

Expected: PASS.

- [ ] **Step 7: Run event-module regression tests**

Run: `cargo nextest run -p advanced-show-control runtime::events`

Expected: PASS.

- [ ] **Step 8: Commit show event wiring**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/runtime/events.rs src-tauri/src/show/events.rs src-tauri/src/commands.rs
git commit -m "refactor: add show events to app event bus"
```

---

### Task 2: Make `ShowStateHandle` Own And Publish Show Events

**Files:**
- Modify: `src-tauri/src/show/handle.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/runtime/commands.rs`
- Modify: `src-tauri/src/scene_recall/actor.rs`

**Interfaces:**
- Consumes: `AppEventBus`, `AppEvent::Show(ShowEvent::SnapshotChanged { reason })`
- Produces: `ShowStateHandle::new_empty(event_bus: AppEventBus) -> Self`
- Produces: `ShellState::new(event_bus: AppEventBus) -> Self`

- [ ] **Step 1: Add failing show-handle event publication tests**

Add this `#[cfg(test)]` module to the bottom of `src-tauri/src/show/handle.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::show::events::{ShowEvent, ShowSnapshotChange};
    use crate::show::types::{SceneConfig, SceneScopeToggles, ShowSnapshot};

    fn scene_config() -> SceneConfig {
        SceneConfig {
            scene_id: "1:Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 0,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    async fn recv_show_event(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        expected_reason: ShowSnapshotChange,
    ) {
        let event = events.recv().await.unwrap();
        assert!(matches!(
            event,
            AppEvent::Show(ShowEvent::SnapshotChanged { reason }) if reason == expected_reason
        ));
    }

    #[tokio::test]
    async fn set_lockout_publishes_show_event_when_changed() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        assert!(show.set_lockout(true).await);

        recv_show_event(&mut events, ShowSnapshotChange::Lockout).await;
    }

    #[tokio::test]
    async fn no_op_lockout_change_does_not_publish_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        assert!(!show.set_lockout(false).await);

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn replace_snapshot_publishes_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        show.replace_snapshot(ShowSnapshot {
            lockout: true,
            scene_configs: vec![scene_config()],
            cued_scene_id: None,
        })
        .await;

        recv_show_event(&mut events, ShowSnapshotChange::SnapshotReplaced).await;
    }
}
```

- [ ] **Step 2: Run the failing show-handle tests**

Run: `cargo nextest run -p advanced-show-control show::handle::tests`

Expected: FAIL because `ShowStateHandle::new_empty` does not accept an event bus and no show mutation publishes events.

- [ ] **Step 3: Add event-bus ownership to `ShowStateHandle`**

In `src-tauri/src/show/handle.rs`, add these imports:

```rust
use crate::runtime::events::{AppEvent, AppEventBus};
use super::events::{ShowEvent, ShowSnapshotChange};
```

Update the struct and constructor:

```rust
#[derive(Clone)]
pub struct ShowStateHandle {
    state: Arc<Mutex<ShowState>>,
    event_bus: AppEventBus,
}

impl ShowStateHandle {
    pub fn new_empty(event_bus: AppEventBus) -> Self {
        Self {
            state: Arc::new(Mutex::new(ShowState::default())),
            event_bus,
        }
    }

    fn publish_snapshot_changed(&self, reason: ShowSnapshotChange) {
        self.event_bus
            .publish(AppEvent::Show(ShowEvent::SnapshotChanged { reason }));
    }
```

- [ ] **Step 4: Publish only after changed show mutations**

Update each `ShowStateHandle` mutating method so it publishes after a true state change:

```rust
    pub async fn cue_scene(&self, scene_id: String) -> Result<bool, String> {
        let changed = self.state.lock().await.cue_scene(&scene_id)?;
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::CueScene);
        }
        Ok(changed)
    }

    pub async fn set_lockout(&self, enabled: bool) -> bool {
        let changed = self.state.lock().await.set_lockout(enabled);
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::Lockout);
        }
        changed
    }
```

Apply the same pattern to these `Result<bool, String>` methods with these exact reasons:

```text
set_scene_duration -> ShowSnapshotChange::SceneDuration
set_scene_scope_faders_enabled -> ShowSnapshotChange::SceneScopeFaders
set_scene_scope_pan_enabled -> ShowSnapshotChange::SceneScopePan
set_channel_scoped -> ShowSnapshotChange::ChannelScope
set_all_channels_scoped -> ShowSnapshotChange::AllChannelsScope
store_scene_config -> ShowSnapshotChange::StoreSceneConfig
```

For `reconcile_scene_list`, publish when the returned `bool` is true:

```rust
    pub async fn reconcile_scene_list(&self, scenes: Vec<SceneListEntry>) -> bool {
        let changed = self
            .state
            .lock()
            .await
            .reconcile_scene_fade_configs(&scenes);
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::SceneListReconciled);
        }
        changed
    }
```

For unconditional snapshot replacements and clears, publish after the mutation:

```rust
    pub async fn replace_snapshot(&self, snapshot: ShowSnapshot) {
        self.state.lock().await.replace_snapshot(snapshot);
        self.publish_snapshot_changed(ShowSnapshotChange::SnapshotReplaced);
    }

    pub async fn clear(&self) {
        self.state.lock().await.clear();
        self.publish_snapshot_changed(ShowSnapshotChange::Cleared);
    }
```

- [ ] **Step 5: Add explicit shell-state construction with the shared event bus**

In `src-tauri/src/app_state/shell.rs`, add this import:

```rust
use crate::runtime::events::AppEventBus;
```

Then replace the current `Default` construction body with an explicit constructor and have `Default` delegate to a standalone bus for tests and default-only call sites:

```rust
impl Default for ShellState {
    fn default() -> Self {
        Self::new(AppEventBus::default())
    }
}

impl ShellState {
    pub fn new(event_bus: AppEventBus) -> Self {
        cover_state_variants();
        Self {
            handles: Arc::new(Mutex::new(RuntimeHandles::default())),
            show: ShowStateHandle::new_empty(event_bus),
            inner: Arc::new(Mutex::new(ShellInner::default())),
        }
    }
```

Do not leave production setup on `ShellState::default()`, because that would create a private event bus and hide show events from the shared runtime bus.

- [ ] **Step 6: Update Tauri setup to pass the shared event bus to `ShellState`**

In `src-tauri/src/commands.rs`, update the runtime setup state construction from:

```rust
let shell = ShellState::default();
```

to:

```rust
let shell = ShellState::new(event_bus.clone());
```

Use the existing runtime `event_bus` that is also passed to LV1/fade/recall/projection setup. If the local variable name differs, use the shared `AppEventBus` value from that setup scope.

- [ ] **Step 7: Update test show-state construction call sites**

Replace test-only construction:

```rust
ShowStateHandle::new_empty()
```

with:

```rust
ShowStateHandle::new_empty(AppEventBus::default())
```

or, where the test already owns an event bus that should observe show events:

```rust
ShowStateHandle::new_empty(event_bus.clone())
```

Known current call sites to update are in:

```text
src-tauri/src/runtime/commands.rs
src-tauri/src/scene_recall/actor.rs
```

- [ ] **Step 8: Run show-handle tests**

Run: `cargo nextest run -p advanced-show-control show::handle::tests`

Expected: PASS.

- [ ] **Step 9: Run show-related regression tests**

Run: `cargo nextest run -p advanced-show-control show app_state::shell::tests runtime::commands::tests::present_show_returns_snapshot scene_recall`

Expected: PASS.

- [ ] **Step 10: Commit show event ownership and publication**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/show/handle.rs src-tauri/src/app_state/shell.rs src-tauri/src/commands.rs src-tauri/src/runtime/commands.rs src-tauri/src/scene_recall/actor.rs
git commit -m "refactor: publish show snapshot events from show state"
```

---

### Task 3: Remove `AppEventBus` From `AppCommandBus::new`

**Files:**
- Modify: `src-tauri/src/runtime/commands.rs`

**Interfaces:**
- Consumes: `AppCommandBus::new(_event_bus: AppEventBus) -> Self`
- Produces: `AppCommandBus::new() -> Self`

- [ ] **Step 1: Add a constructor test for the new boundary**

In `src-tauri/src/runtime/commands.rs`, add this test near the top of the existing `#[cfg(test)] mod tests`:

```rust
    #[tokio::test]
    async fn command_bus_constructs_without_event_bus() {
        let bus = AppCommandBus::new();

        let err = bus.get_lv1_state().await.unwrap_err();

        assert_eq!(err, AppCommandError::Lv1Unavailable);
    }
```

- [ ] **Step 2: Run the failing constructor test**

Run: `cargo nextest run -p advanced-show-control runtime::commands::tests::command_bus_constructs_without_event_bus`

Expected: FAIL because `AppCommandBus::new` still requires an `AppEventBus` argument.

- [ ] **Step 3: Update the command bus constructor**

In `src-tauri/src/runtime/commands.rs`, remove this production import:

```rust
use crate::runtime::events::AppEventBus;
```

Then replace the constructor implementation:

```rust
    pub fn new(_event_bus: AppEventBus) -> Self {
        Self {
            targets: Arc::new(Mutex::new(AppCommandTargets::default())),
        }
    }
```

with:

```rust
    pub fn new() -> Self {
        Self {
            targets: Arc::new(Mutex::new(AppCommandTargets::default())),
        }
    }
```

- [ ] **Step 4: Keep event-bus imports test-local where needed**

Inside the existing `#[cfg(test)] mod tests`, keep or add this import because several tests still create standalone event buses for assertions and `ShowStateHandle::new_empty`:

```rust
    use crate::runtime::events::AppEventBus;
```

- [ ] **Step 5: Update command-bus unit test constructors**

In `src-tauri/src/runtime/commands.rs`, replace command-bus construction with:

```rust
let bus = AppCommandBus::new();
```

For tests that subscribe to an event bus, keep the event bus and subscription but do not pass it to the command bus:

```rust
let event_bus = AppEventBus::default();
let mut events = event_bus.subscribe();
let bus = AppCommandBus::new();
```

For tests that install show state, construct the show handle with its own event bus:

```rust
bus.set_show(Some(ShowStateHandle::new_empty(AppEventBus::default())))
    .await;
```

- [ ] **Step 6: Run command-bus tests**

Run: `cargo nextest run -p advanced-show-control runtime::commands`

Expected: PASS.

- [ ] **Step 7: Commit command-bus constructor change**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/runtime/commands.rs
git commit -m "refactor: decouple command bus from event bus"
```

---

### Task 4: Update Remaining Command-Bus Call Sites

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/fade/actor.rs`
- Modify: `src-tauri/src/scene_recall/actor.rs`
- Modify: `src-tauri/src/bin/lv1-probe.rs`
- Modify: `src-tauri/tests/runtime_bus.rs`
- Modify: `src-tauri/tests/fade_engine.rs`

**Interfaces:**
- Consumes: `AppCommandBus::new() -> AppCommandBus`
- Produces: all production, CLI, unit-test, and integration-test Rust call sites compile with the parameterless constructor.

- [ ] **Step 1: Update Tauri runtime setup**

In `src-tauri/src/commands.rs`, update runtime setup from:

```rust
let command_bus = AppCommandBus::new(event_bus.clone());
```

to:

```rust
let command_bus = AppCommandBus::new();
```

Keep the existing `event_bus` variable and all existing uses that pass it to LV1, fade, recall, show state, shell projection, or direct event publication.

- [ ] **Step 2: Update all remaining Rust call sites**

Replace every command-bus construction that passes an event bus:

```rust
AppCommandBus::new(event_bus.clone())
AppCommandBus::new(event_bus)
AppCommandBus::new(AppEventBus::default())
AppCommandBus::new(crate::runtime::events::AppEventBus::default())
```

with:

```rust
AppCommandBus::new()
```

Keep event bus variables when they are used by actors, event subscriptions, explicit `publish(...)`, or `ShowStateHandle::new_empty(event_bus.clone())`.

- [ ] **Step 3: Run a constructor call-site search**

Run: `rg "AppCommandBus::new\(" src-tauri/src src-tauri/tests`

Expected: every match is exactly `AppCommandBus::new()` with no arguments.

- [ ] **Step 4: Run targeted compile/test checks**

Run: `cargo nextest run -p advanced-show-control runtime_bus fade_engine lifecycle::tests commands::tests scene_recall`

Expected: PASS.

- [ ] **Step 5: Build the preserved probe binary**

Run: `cargo build -p advanced-show-control --bin lv1-probe`

Expected: PASS.

- [ ] **Step 6: Commit call-site updates**

Run: `git status --short`

Then commit:

```bash
git add src-tauri/src/commands.rs src-tauri/src/lifecycle/mod.rs src-tauri/src/app_state/shell.rs src-tauri/src/fade/actor.rs src-tauri/src/scene_recall/actor.rs src-tauri/src/bin/lv1-probe.rs src-tauri/tests/runtime_bus.rs src-tauri/tests/fade_engine.rs
git commit -m "refactor: update command bus construction"
```

---

### Task 5: Document Phase 7 Boundary And Run Full Verification

**Files:**
- Modify: `docs/architecture.md`

**Interfaces:**
- Consumes: phase-7 implementation from Tasks 1-4.
- Produces: documented boundary stating `AppEventBus` carries show facts, `show/` owns event publication, and `AppCommandBus` does not own or receive `AppEventBus`.

- [ ] **Step 1: Update bus contract docs**

In `docs/architecture.md`, update the `AppEventBus carries broadcast facts only.` section to explicitly mention show facts:

```markdown
`AppEventBus` carries broadcast facts only. It currently carries LV1, fade, scene-recall, and show/app facts.
```

- [ ] **Step 2: Update command bus contract docs**

In `docs/architecture.md`, update the `AppCommandBus carries acknowledged requests only.` section to include this line:

```markdown
`AppCommandBus` does not own or receive `AppEventBus`; modules that publish facts own their event-bus reference directly.
```

- [ ] **Step 3: Update show-state ownership docs**

In `docs/architecture.md`, update the `ShowState` ownership bullet to mention event publication:

```markdown
- `ShowState` owns show data only: scene configs, one shared scoped channel list, `FADERS` and `PAN` scene toggles, stored target values, show-file persistence, and show/app snapshot-change fact publication. It is app-lifetime state behind a cloneable mutex-backed handle, not a spawned Tokio actor.
```

- [ ] **Step 4: Run formatting**

Run: `cargo fmt --all -- --check`

Expected: PASS.

If it fails because Rust formatting changed, run `cargo fmt --all`, inspect the diff, then rerun `cargo fmt --all -- --check`.

- [ ] **Step 5: Run Rust linting**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 6: Run full Rust tests**

Run: `cargo nextest run --workspace`

Expected: PASS.

- [ ] **Step 7: Run workspace build**

Run: `cargo build --workspace`

Expected: PASS.

- [ ] **Step 8: Run probe binary build**

Run: `cargo build -p advanced-show-control --bin lv1-probe`

Expected: PASS.

- [ ] **Step 9: Run frontend typecheck as command-boundary smoke coverage**

Run: `npm --prefix ui run typecheck`

Expected: PASS.

- [ ] **Step 10: Run Tauri build smoke check**

Run: `npm run tauri -- build`

Expected: PASS. The existing bundle identifier warning about `com.advancedshowcontrol.app` ending with `.app` is non-fatal and does not fail this task.

- [ ] **Step 11: Commit docs and verification-ready state**

Run: `git status --short`

Then commit:

```bash
git add docs/architecture.md
git commit -m "docs: describe show event command boundary"
```

---

## Self-Review Checklist

- Spec coverage: This plan implements approved phase 7 by adding `AppEvent::Show(ShowEvent)`, making `show/` own the event-bus reference it publishes through, and removing `AppEventBus` from `AppCommandBus::new`.
- Projection rule: Existing show mutations that affect `AppViewState` publish `ShowEvent::SnapshotChanged { reason }`; the current shell projector ignores `AppEvent::Show` until the new projector phase.
- Scope guard: This plan does not route low-risk show/app commands through `AppCommandBus`, move show-file ownership, move UI recall, build the new projector cache, move logging to projector input, remove direct emits, update React command contracts, eliminate `ShellState`, or remove `ActiveCommandBus`.
- Safety guard: Existing LV1 protocol, fade, recall, generation, lockout, direct emit, and frontend command-return behavior are preserved.
- Type consistency: The app-wide event shape is `AppEvent::Show(ShowEvent)`, show publication uses `ShowEvent::SnapshotChanged { reason: ShowSnapshotChange }`, `ShowStateHandle::new_empty(event_bus: AppEventBus) -> Self`, and the command-bus constructor becomes `AppCommandBus::new() -> Self`.
- Verification: Targeted tests cover the new show event path, show-state event publication, and constructor boundary; full verification covers workspace behavior and the preserved `lv1-probe` binary.
