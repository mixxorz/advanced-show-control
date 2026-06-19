# Show State Command Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove `ShellState`, move app/session source state into private `show::ShowState`, move runtime lifecycle state into `AppLifecycle`, and make projector-only `AppViewState` delivery work through generated events and frontend readiness.

**Architecture:** `show/` owns app/session state and publishes full `ShowProjectionState` payloads. `AppLifecycle` owns runtime handles, generation, the app-lifetime command bus, and frontend readiness startup orchestration. The projector is a dumb 10 Hz coalescing cache that consumes `AppEventBus` plus UI log input, ignores stale runtime generations, and never pulls from `show`.

**Tech Stack:** Rust/Tauri/Tokio, `tokio::sync::broadcast`, React/TypeScript, Vitest, cargo nextest.

## Global Constraints

- Use the spec at `docs/superpowers/specs/2026-06-20-show-state-command-cutover-design.md` as the source of truth.
- This is a single cutover; do not preserve `ShellState` with compatibility shims.
- `show/` owns app/session state only; it does not own LV1, fade, or log state.
- `AppLifecycle` owns runtime lifecycle state: generation, runtime handles, app-lifetime command bus, startup readiness, and cleanup.
- `AppEventBus` is app-lifetime state shared by show, lifecycle/runtime actors, and projector.
- Runtime-originated events carry generation; stale-generation runtime events must not affect projection or show-owned state.
- `AppLifecycle` publishes active runtime generation changes on the app-lifetime `AppEventBus`; projector and show-owned runtime listeners use that event to update their internal active generation.
- Show/app/session commands that do not require live LV1 runtime must work while disconnected through an app-lifetime/show-capable command bus or lifecycle-owned direct show path.
- Projector does not pull from `show`; it applies `ShowProjectionState` from `ShowEvent`.
- Projector does not clear or rewrite show-owned connection metadata on LV1 disconnect.
- `AppEventBus` remains lossy; lag recovery for missed show projection events is intentionally deferred.
- React updates app state only from `app-status-changed`; mutating commands do not return `AppViewState`.
- The backend does not start runtime producers or emit initial app state before `frontend_ready`.
- No LV1 protocol behavior changes.
- No saved show-file format redesign.
- No frontend `AppViewState` schema redesign.
- Preserve lockout, exact scene identity, generation guards, disconnect behavior, manual override, abort, overlap, and same-scene safety behavior.
- Run verification before claiming completion: `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo nextest run --workspace`, `cargo build --workspace`, `npm --prefix ui run typecheck`, `npm --prefix ui run test`.

---

## File Structure Map

### Rust Backend

- `src-tauri/src/app_state/view.rs`: Keep `AppViewState`, `AppConnectionState`, `AppFadeState`, `AppLogEntry`, `ChannelSummary`, `SceneSummary`, and `LogSeverity`. Remove re-export of show snapshot DTOs after show types are renamed.
- `src-tauri/src/app_state/mod.rs`: Shrink to view-model exports only, then remove `shell`, `events`, and shell-specific tests.
- `src-tauri/src/app_state/shell.rs`: Delete in the final cutover task.
- `src-tauri/src/app_state/events.rs`: Delete after projector and command paths no longer use shell projection helpers.
- `src-tauri/src/app_state/show_file_mapping.rs`: Fold remaining useful mapping into `show::commands` and `show::show_file`, then delete this file.
- `src-tauri/src/show/state.rs`: Expand private `ShowState` source-of-truth fields and keep all fields private.
- `src-tauri/src/show/types.rs`: Keep scene/domain DTOs and remove `ShowSnapshot`.
- `src-tauri/src/show/events.rs`: Define `ShowEvent { reason, state }`, `ShowProjectionState`, and `ShowProjectionReason`.
- `src-tauri/src/show/handle.rs`: Keep a cloneable mutex-backed handle. Expose command execution helpers to `show::commands` only with `pub(super)`/`pub(crate)` where necessary. Remove broad public mutation setters.
- `src-tauri/src/show/commands.rs`: Become the authoritative implementation of show/app/session commands and command-specific result types.
- `src-tauri/src/show/show_file.rs`: Keep persistence DTOs and import/export mapping. Rename parameters from `ShowSnapshot` to `ShowProjectionState`.
- `src-tauri/src/lifecycle/mod.rs`: Replace `ActiveCommandBus`; own runtime handles, generation, frontend readiness, projector startup, app-lifetime command bus, and runtime cleanup.
- `src-tauri/src/runtime/events.rs`: Make runtime-originated `AppEvent` variants carry generation, add lifecycle generation events, keep `ShowEvent` ungenerated, keep lag logging diagnostic only.
- `src-tauri/src/runtime/commands.rs`: Route all show/app commands and read-only queries through `AppCommandBus`; expose lifecycle-friendly methods for LV1/fade command targets and generation.
- `src-tauri/src/projector/cache.rs`: Own `state_version`, LV1/fade/log projection caches, show projection cache, `apply_show_state`, and generated runtime event application.
- `src-tauri/src/projector/runtime.rs`: Consume app-lifetime bus/log channel, gate runtime events by generation, emit only after `frontend_ready`, no show pulls.
- `src-tauri/src/commands.rs`: Delete after moving Tauri adapter commands to `src-tauri/src/ui/commands.rs`.
- `src-tauri/src/ui/mod.rs`: Manage app-lifetime event bus, show handle, lifecycle, UI log receiver, projector bootstrap, and command registration.
- `src-tauri/src/ui/commands.rs`: Thin Tauri command wrappers. Show/app state changes go through the app-lifetime `AppCommandBus`; lifecycle-owned runtime operations go through `AppLifecycle` methods.
- `src-tauri/src/lib.rs`: Update module exports after `app_state` shrink and command movement.
- `src-tauri/src/lv1/actor.rs`, `src-tauri/src/fade/actor.rs`, `src-tauri/src/scene_recall/actor.rs`: Publish generated runtime events through the app-lifetime event bus.
- `src-tauri/src/bin/lv1-probe.rs`: Should remain buildable; no behavior changes planned.

### Frontend

- `ui/src/App.tsx`: Add `frontend_ready` invoke after listener registration path is available through services. Change commands away from `Promise<AppViewState>`.
- `ui/src/AppRuntime.tsx`: Remove `runSnapshot`, do not apply command return values as app state, and drive startup via listener registration then `frontendReady`.
- `ui/src/AppRuntime.test.tsx`: Add tests for frontend-ready order and command result handling.
- `ui/src/commands.ts`: Change command return types from `AppViewState` to command-specific result types or `void` according to Task 7's command list.
- `ui/src/appContext.tsx`: Update command callback result types so no app command callback returns `AppViewState`.
- `ui/src/types.ts`: Keep `AppViewState` shape unchanged and add named command result types used by frontend services.

---

### Task 1: Define Generated App Events And Show Projection Contract

**Files:**
- Modify: `src-tauri/src/runtime/events.rs`
- Modify: `src-tauri/src/show/events.rs`
- Modify: `src-tauri/src/show/types.rs`
- Modify: `src-tauri/src/show/state.rs`
- Modify: `src-tauri/src/show/handle.rs`

**Interfaces:**
- Produces: `GeneratedAppEvent<T> { pub generation: u64, pub event: T }`
- Produces: `AppEvent::Lv1(GeneratedAppEvent<Lv1Event>)`
- Produces: `AppEvent::Fade(GeneratedAppEvent<FadeEvent>)`
- Produces: `AppEvent::SceneRecall(GeneratedAppEvent<SceneRecallEvent>)`
- Produces: `RuntimeLifecycleEvent::ActiveGenerationChanged { generation: u64 }`
- Produces: `AppEvent::Runtime(RuntimeLifecycleEvent)`
- Produces: `ShowProjectionState`
- Produces: `ShowEvent::StateChanged { reason: ShowProjectionReason, state: ShowProjectionState }`
- Produces: `ShowState::projection_state(&self) -> ShowProjectionState`

- [ ] **Step 1: Write failing runtime event generation tests**

Add to `src-tauri/src/runtime/events.rs` tests:

```rust
#[tokio::test]
async fn runtime_events_carry_generation() {
    let bus = AppEventBus::new(16);
    let mut rx = bus.subscribe();

    bus.publish(AppEvent::Lv1(GeneratedAppEvent {
        generation: 42,
        event: Lv1Event::Connected,
    }));

    let event = rx.recv().await.unwrap();
    match event {
        AppEvent::Lv1(generated) => {
            assert_eq!(generated.generation, 42);
            assert!(matches!(generated.event, Lv1Event::Connected));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

Also add:

```rust
#[tokio::test]
async fn lifecycle_events_publish_active_generation_changes() {
    let bus = AppEventBus::new(16);
    let mut rx = bus.subscribe();

    bus.publish_runtime_generation_changed(7);

    let event = rx.recv().await.unwrap();
    match event {
        AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation }) => {
            assert_eq!(generation, 7);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

- [ ] **Step 2: Write failing show event payload tests**

Add to `src-tauri/src/show/handle.rs` tests:

```rust
#[tokio::test]
async fn show_event_carries_full_projection_state() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);

    show.command_set_lockout(true).await;

    let event = events.recv().await.unwrap();
    match event {
        AppEvent::Show(ShowEvent::StateChanged { reason, state }) => {
            assert_eq!(reason, ShowProjectionReason::Lockout);
            assert!(state.lockout);
            assert_eq!(state.show_file_name, "Untitled Show");
            assert!(!state.show_file_dirty);
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

- [ ] **Step 3: Run tests and confirm they fail**

Run:

```bash
cargo nextest run -p advanced-show-control runtime::events::tests::runtime_events_carry_generation show::handle::tests::show_event_carries_full_projection_state
```

Expected: fail to compile because `GeneratedAppEvent`, `RuntimeLifecycleEvent`, `ShowProjectionState`, `ShowProjectionReason`, `ShowEvent::StateChanged`, and `command_set_lockout` do not exist.

- [ ] **Step 4: Implement generated event wrapper**

Change `src-tauri/src/runtime/events.rs`:

```rust
#[derive(Debug, Clone)]
pub struct GeneratedAppEvent<T> {
    pub generation: u64,
    pub event: T,
}

#[derive(Debug, Clone)]
pub enum AppEvent {
    Runtime(RuntimeLifecycleEvent),
    Lv1(GeneratedAppEvent<Lv1Event>),
    Fade(GeneratedAppEvent<FadeEvent>),
    SceneRecall(GeneratedAppEvent<crate::scene_recall::events::SceneRecallEvent>),
    Show(ShowEvent),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeLifecycleEvent {
    ActiveGenerationChanged { generation: u64 },
}
```

Update existing tests in this file to wrap LV1 and scene recall events with `GeneratedAppEvent { generation: 0, event: ... }`. Keep `Show` unchanged.

Add an `AppEventBus` helper:

```rust
pub fn publish_runtime_generation_changed(&self, generation: u64) -> usize {
    self.publish(AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation }))
}
```

- [ ] **Step 5: Define show projection types**

In `src-tauri/src/show/events.rs`, replace the current enum definitions with:

```rust
use std::path::PathBuf;

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};

use super::types::SceneConfig;

#[derive(Debug, Clone, PartialEq)]
pub enum ShowEvent {
    StateChanged {
        reason: ShowProjectionReason,
        state: ShowProjectionState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShowProjectionReason {
    CueScene,
    Lockout,
    SceneDuration,
    SceneScopeFaders,
    SceneScopePan,
    ChannelScope,
    AllChannelsScope,
    StoreSceneConfig,
    SceneListReconciled,
    ShowReplaced,
    Cleared,
    SelectedScene,
    ShowFileCreated,
    ShowFileLoaded,
    ShowFileSaved,
    ShowFileDirty,
    DiscoveryUpdated,
    PendingIdentity,
    ConnectedIdentity,
    ReconnectState,
    Disconnected,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShowProjectionState {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_id: Option<String>,
    pub selected_scene_id: Option<String>,
    pub show_file_path: Option<PathBuf>,
    pub show_file_name: String,
    pub show_file_dirty: bool,
    pub show_file_last_saved_at: Option<String>,
    pub discovered_lv1_systems: Vec<DiscoveredLv1System>,
    pub connected_lv1_identity: Option<Lv1SystemIdentity>,
    pub pending_lv1_identity: Option<Lv1SystemIdentity>,
    pub reconnect: ReconnectState,
    pub last_event_at: Option<String>,
}
```

- [ ] **Step 6: Add projection state builder**

In `src-tauri/src/show/state.rs`, make current fields private and add projection fields:

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShowState {
    lockout: bool,
    scene_configs: Vec<SceneConfig>,
    cued_scene_id: Option<String>,
    selected_scene_id: Option<String>,
    show_file_path: Option<std::path::PathBuf>,
    show_file_dirty: bool,
    show_file_last_saved_at: Option<String>,
    discovered_lv1_systems: Vec<crate::connection_state::DiscoveredLv1System>,
    connected_lv1_identity: Option<crate::connection_state::Lv1SystemIdentity>,
    pending_lv1_identity: Option<crate::connection_state::Lv1SystemIdentity>,
    reconnect: crate::connection_state::ReconnectState,
    last_event_at: Option<String>,
}
```

Add:

```rust
impl ShowState {
    pub fn projection_state(&self) -> super::events::ShowProjectionState {
        let show_file_name = self
            .show_file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| "Untitled Show".to_string());

        super::events::ShowProjectionState {
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
            cued_scene_id: self.cued_scene_id.clone(),
            selected_scene_id: self.selected_scene_id.clone(),
            show_file_path: self.show_file_path.clone(),
            show_file_name,
            show_file_dirty: self.show_file_dirty,
            show_file_last_saved_at: self.show_file_last_saved_at.clone(),
            discovered_lv1_systems: self.discovered_lv1_systems.clone(),
            connected_lv1_identity: self.connected_lv1_identity.clone(),
            pending_lv1_identity: self.pending_lv1_identity.clone(),
            reconnect: self.reconnect.clone(),
            last_event_at: self.last_event_at.clone(),
        }
    }
}
```

- [ ] **Step 7: Update show handle publisher**

In `src-tauri/src/show/handle.rs`, replace `publish_snapshot_changed` with:

```rust
fn publish_state_changed(&self, reason: ShowProjectionReason, state: &ShowState) {
    self.event_bus.publish(AppEvent::Show(ShowEvent::StateChanged {
        reason,
        state: state.projection_state(),
    }));
}
```

For this task, add a minimal command helper used by the new test:

```rust
pub(crate) async fn command_set_lockout(&self, enabled: bool) -> bool {
    let mut state = self.state.lock().await;
    let changed = state.set_lockout(enabled);
    if changed {
        self.publish_state_changed(ShowProjectionReason::Lockout, &state);
    }
    changed
}
```

Temporarily update existing `set_lockout` to call `command_set_lockout` so existing tests continue to pass until later tasks remove public setters.

- [ ] **Step 8: Update compile errors from renamed event reason**

In files that refer to `ShowSnapshotChange`, replace with `ShowProjectionReason` for now. In tests matching `ShowEvent::SnapshotChanged { reason }`, match `ShowEvent::StateChanged { reason, .. }`.

- [ ] **Step 9: Run targeted tests**

Run:

```bash
cargo nextest run -p advanced-show-control runtime::events show::handle::tests::show_event_carries_full_projection_state
```

Expected: all selected tests pass.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/runtime/events.rs src-tauri/src/show/events.rs src-tauri/src/show/types.rs src-tauri/src/show/state.rs src-tauri/src/show/handle.rs
git commit -m "refactor: add generated events and show projection state"
```

---

### Task 2: Make Runtime Publishers Carry Generation

**Files:**
- Modify: `src-tauri/src/lv1/actor.rs`
- Modify: `src-tauri/src/fade/actor.rs`
- Modify: `src-tauri/src/scene_recall/actor.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: tests under affected modules

**Interfaces:**
- Consumes: `GeneratedAppEvent<T>` from Task 1.
- Produces: `spawn_actor(host, port, event_bus, generation)`.
- Produces: `spawn_engine(command_bus, event_bus, generation)`.
- Produces: `spawn_scene_recall_fader(generation, command_bus, event_bus)` publishing generated scene-recall events.

- [ ] **Step 1: Write failing LV1 publisher test**

In `src-tauri/src/lv1/state.rs`, update `actor_publishes_scene_changes_to_event_bus` to receive `AppEvent::Lv1(generated)` and assert `generated.generation == 7`.

Example assertion shape:

```rust
match event {
    AppEvent::Lv1(generated) => {
        assert_eq!(generated.generation, 7);
        assert!(matches!(generated.event, Lv1Event::Connected));
    }
    other => panic!("unexpected event: {other:?}"),
}
```

- [ ] **Step 2: Write failing fade publisher test**

In `src-tauri/src/fade/actor.rs` tests, update a fade-start publication assertion to expect:

```rust
match event {
    AppEvent::Fade(generated) => {
        assert_eq!(generated.generation, 7);
        assert!(matches!(generated.event, FadeEvent::FadeStarted));
    }
    other => panic!("unexpected event: {other:?}"),
}
```

- [ ] **Step 3: Run targeted tests and confirm failure**

Run:

```bash
cargo nextest run -p advanced-show-control lv1::state::tests::actor_publishes_scene_changes_to_event_bus fade::actor::tests::timed_fade_sends_due_writes_in_one_batch
```

Expected: fail because publishers still send ungenerated events or spawn signatures lack generation.

- [ ] **Step 4: Add generated publishing helper**

In `src-tauri/src/runtime/events.rs`, add:

```rust
impl AppEventBus {
    pub fn publish_lv1(&self, generation: u64, event: Lv1Event) -> usize {
        self.publish(AppEvent::Lv1(GeneratedAppEvent { generation, event }))
    }

    pub fn publish_fade(&self, generation: u64, event: FadeEvent) -> usize {
        self.publish(AppEvent::Fade(GeneratedAppEvent { generation, event }))
    }

    pub fn publish_scene_recall(
        &self,
        generation: u64,
        event: crate::scene_recall::events::SceneRecallEvent,
    ) -> usize {
        self.publish(AppEvent::SceneRecall(GeneratedAppEvent { generation, event }))
    }
}
```

- [ ] **Step 5: Update LV1 actor spawn signature**

Change `src-tauri/src/lv1/actor.rs` spawn entry point from:

```rust
pub fn spawn_actor(host: String, port: u16, events: AppEventBus) -> Lv1ActorHandle
```

to:

```rust
pub fn spawn_actor(host: String, port: u16, events: AppEventBus, generation: u64) -> Lv1ActorHandle
```

Store `generation` in the actor state and replace `events.publish(AppEvent::Lv1(event))` with `events.publish_lv1(generation, event)`.

- [ ] **Step 6: Update fade engine spawn signature**

Change `src-tauri/src/fade/actor.rs` spawn entry point from:

```rust
pub fn spawn_engine(command_bus: AppCommandBus, events: AppEventBus) -> FadeEngineHandle
```

to:

```rust
pub fn spawn_engine(command_bus: AppCommandBus, events: AppEventBus, generation: u64) -> FadeEngineHandle
```

Replace fade event publishes with `events.publish_fade(generation, event)`.

- [ ] **Step 7: Update scene recall publishing**

In `src-tauri/src/scene_recall/actor.rs`, keep the existing `generation` parameter and replace scene recall event publishes with `events.publish_scene_recall(generation, event)`.

- [ ] **Step 8: Update spawn call sites**

In `src-tauri/src/commands.rs` while still transitional, update:

```rust
let lv1 = spawn_actor(identity.address.clone(), identity.port, event_bus.clone(), generation);
let fade = spawn_engine(command_bus, event_bus.clone(), generation);
```

Later lifecycle tasks will move these calls out of `commands.rs`.

- [ ] **Step 9: Update event subscribers to unwrap generated events**

Search for `AppEvent::Lv1(`, `AppEvent::Fade(`, and `AppEvent::SceneRecall(`. Update matches:

```rust
Ok(AppEvent::Lv1(generated)) if generated.generation == generation => {
    let event = generated.event;
    // existing logic
}
Ok(AppEvent::Lv1(_)) => {}
```

For subscribers that are not generation-scoped yet, unwrap and pass through temporarily with a comment in the plan implementation commit message; Task 5 and Task 6 will remove transitional gaps.

- [ ] **Step 10: Run targeted tests**

Run:

```bash
cargo nextest run -p advanced-show-control lv1::state fade::actor scene_recall::actor runtime::events
```

Expected: pass.

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/runtime/events.rs src-tauri/src/lv1 src-tauri/src/fade src-tauri/src/scene_recall src-tauri/src/commands.rs
git commit -m "refactor: tag runtime events with generation"
```

---

### Task 3: Expand ShowState And Move Show/App Metadata Mutations Into Show Commands

**Files:**
- Modify: `src-tauri/src/show/state.rs`
- Modify: `src-tauri/src/show/handle.rs`
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/show/show_file.rs`
- Modify: `src-tauri/src/show/types.rs`
- Test: `src-tauri/src/show/commands.rs`

**Interfaces:**
- Consumes: `ShowProjectionState`, `ShowProjectionReason`, `ShowEvent::StateChanged` from Task 1.
- Produces: `show::commands::new_show_file(show, lv1) -> Result<NewShowFileResult, String>` that updates metadata and publishes state.
- Produces: `show::commands::load_show_file_from_dto(show, path, file, lv1) -> Result<LoadShowFileResult, String>` that updates metadata and publishes state.
- Produces: `show::commands::mark_show_file_saved(show, path, saved_at) -> ShowCommandResult`.
- Produces: `show::commands::set_discovered_lv1_systems(show, systems) -> ShowCommandResult`.
- Produces: `show::commands::set_pending_lv1_identity(show, generation, identity) -> ShowCommandResult`.
- Produces: `show::commands::establish_connected_lv1_identity(show, generation, identity) -> ShowCommandResult`.
- Produces: `show::commands::handle_runtime_disconnected(show, generation, reason) -> ShowCommandResult`.

- [ ] **Step 1: Write failing command transaction tests**

Add tests in `src-tauri/src/show/commands.rs`:

```rust
#[tokio::test]
async fn new_show_file_updates_metadata_and_publishes_state() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    let lv1 = recall_lv1(ConnectionStatus::Connected, "Intro");

    let result = new_show_file(&show, Some(lv1)).await.unwrap();

    assert_eq!(result.selected_scene_id, Some("1::Intro".to_string()));
    let event = events.recv().await.unwrap();
    match event {
        AppEvent::Show(ShowEvent::StateChanged { reason, state }) => {
            assert_eq!(reason, ShowProjectionReason::ShowFileCreated);
            assert_eq!(state.selected_scene_id, Some("1::Intro".to_string()));
            assert!(!state.show_file_dirty);
            assert_eq!(state.show_file_name, "Untitled Show");
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[tokio::test]
async fn changed_scene_duration_marks_show_dirty_in_command() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    new_show_file(&show, Some(recall_lv1(ConnectionStatus::Connected, "Intro"))).await.unwrap();
    drain_show_events(&mut events).await;

    let result = set_scene_duration_ms(&show, "1::Intro".to_string(), 1500).await.unwrap();

    assert!(result.changed);
    let event = events.recv().await.unwrap();
    match event {
        AppEvent::Show(ShowEvent::StateChanged { state, .. }) => assert!(state.show_file_dirty),
        other => panic!("unexpected event: {other:?}"),
    }
}
```

Add helper in the test module:

```rust
async fn drain_show_events(events: &mut tokio::sync::broadcast::Receiver<AppEvent>) {
    while events.try_recv().is_ok() {}
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo nextest run -p advanced-show-control show::commands::tests::new_show_file_updates_metadata_and_publishes_state show::commands::tests::changed_scene_duration_marks_show_dirty_in_command
```

Expected: fail because commands do not yet update all metadata/publish desired reasons.

- [ ] **Step 3: Add private mutation transaction helper**

In `src-tauri/src/show/handle.rs`, add:

```rust
impl ShowStateHandle {
    pub(crate) async fn mutate_for_command<R>(
        &self,
        reason: ShowProjectionReason,
        apply: impl FnOnce(&mut ShowState) -> (bool, R),
    ) -> R {
        let mut state = self.state.lock().await;
        let (changed, result) = apply(&mut state);
        if changed {
            self.publish_state_changed(reason, &state);
        }
        result
    }

    pub(crate) async fn query<R>(&self, read: impl FnOnce(&ShowState) -> R) -> R {
        let state = self.state.lock().await;
        read(&state)
    }
}
```

This helper publishes only when `apply` returns `changed == true`. No-op commands must return `changed == false` so they do not publish a redundant `ShowEvent`.

- [ ] **Step 4: Move dirty/file metadata updates into command handlers**

In `src-tauri/src/show/commands.rs`, implement command handlers that mutate `ShowState` directly through `mutate_for_command`.

Example for duration:

```rust
pub async fn set_scene_duration_ms(
    show: &ShowStateHandle,
    scene_id: String,
    duration_ms: u64,
) -> Result<ShowCommandResult, String> {
    show.query(|state| state.get_scene_config(&scene_id))
        .await
        .ok_or_else(|| "Scene config not found".to_string())?;

    let changed = show
        .mutate_for_command(ShowProjectionReason::SceneDuration, |state| {
            let changed = state.set_scene_duration_ms(&scene_id, duration_ms)?;
            if changed {
                state.mark_dirty();
            }
            (changed, Ok::<bool, String>(changed))
        })
        .await?;

    Ok(ShowCommandResult { changed })
}
```

Add `ShowState::mark_dirty(&mut self)` in `show/state.rs`:

```rust
pub(crate) fn mark_dirty(&mut self) {
    self.show_file_dirty = true;
}
```

- [ ] **Step 5: Implement new/load/save metadata commands**

In `src-tauri/src/show/commands.rs`, implement:

```rust
pub async fn new_show_file(
    show: &ShowStateHandle,
    lv1: Option<Lv1StateSnapshot>,
) -> Result<NewShowFileResult, String> {
    let selected_scene_id = show
        .mutate_for_command(ShowProjectionReason::ShowFileCreated, |state| {
            state.clear();
            if let Some(lv1) = lv1
                && !lv1.scene_list.is_empty()
            {
                state.reconcile_scene_fade_configs(&lv1.scene_list);
            }
            state.selected_scene_id = state.scene_configs.first().map(|scene| scene.scene_id.clone());
            state.show_file_path = None;
            state.show_file_dirty = false;
            state.show_file_last_saved_at = None;
            (true, state.selected_scene_id.clone())
        })
        .await;

    tracing::info!(event = "show_file_created", "New show file created");
    Ok(NewShowFileResult { selected_scene_id })
}
```

Implement `mark_show_file_saved`:

```rust
pub async fn mark_show_file_saved(
    show: &ShowStateHandle,
    path: std::path::PathBuf,
    saved_at: String,
) -> ShowCommandResult {
    show.mutate_for_command(ShowProjectionReason::ShowFileSaved, |state| {
        state.show_file_path = Some(path);
        state.show_file_last_saved_at = Some(saved_at);
        state.show_file_dirty = false;
        (true, ())
    })
    .await;
    tracing::info!(event = "show_file_saved", "Show file saved");
    ShowCommandResult { changed: true }
}
```

- [ ] **Step 6: Implement discovery/identity/reconnect metadata commands**

In `src-tauri/src/show/commands.rs`, add command handlers:

```rust
pub async fn set_discovered_lv1_systems(
    show: &ShowStateHandle,
    systems: Vec<DiscoveredLv1System>,
) -> ShowCommandResult {
    let changed = show
        .mutate_for_command(ShowProjectionReason::DiscoveryUpdated, |state| {
            if state.discovered_lv1_systems == systems {
                (false, false)
            } else {
                state.discovered_lv1_systems = systems;
                (true, true)
            }
        })
        .await;
    ShowCommandResult { changed }
}
```

Add handlers for pending identity, connected identity, reconnect state, active runtime generation tracking, and active-generation disconnect with these exact names:

```rust
pub async fn set_pending_lv1_identity(show: &ShowStateHandle, identity: Option<Lv1SystemIdentity>) -> ShowCommandResult
pub async fn establish_connected_lv1_identity(show: &ShowStateHandle, identity: Lv1SystemIdentity) -> ShowCommandResult
pub async fn clear_connected_lv1_identity(show: &ShowStateHandle) -> ShowCommandResult
pub async fn set_reconnect_state(show: &ShowStateHandle, reconnect: ReconnectState) -> ShowCommandResult
pub async fn set_active_runtime_generation(show: &ShowStateHandle, generation: u64) -> ShowCommandResult
pub async fn handle_runtime_disconnected(show: &ShowStateHandle, generation: u64, reason: String) -> ShowCommandResult
```

`handle_runtime_disconnected` first validates `generation` against the show-owned active runtime generation tracked from `RuntimeLifecycleEvent::ActiveGenerationChanged`. If stale, it returns `ShowCommandResult { changed: false }` and publishes nothing. If active, it clears `connected_lv1_identity` and `pending_lv1_identity`, sets reconnect metadata according to current behavior from `ShellState`, and publishes `ShowProjectionReason::Disconnected`.

`set_active_runtime_generation` updates the show-owned active runtime generation used only for stale runtime fact filtering. It should not publish a `ShowEvent` unless a visible show-owned field changes.

- [ ] **Step 7: Run show command tests**

Run:

```bash
cargo nextest run -p advanced-show-control show::commands show::handle
```

Expected: pass after updating older tests to new event names.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/show
git commit -m "refactor: move app session mutations into show commands"
```

---

### Task 4: Make Projector Apply ShowProjectionState And Ignore Stale Runtime Events

**Files:**
- Modify: `src-tauri/src/projector/cache.rs`
- Modify: `src-tauri/src/projector/runtime.rs`
- Modify: `src-tauri/src/projector/mod.rs`

**Interfaces:**
- Consumes: `ShowProjectionState`, `GeneratedAppEvent<T>`.
- Produces: `ProjectionCache::apply_show_state(&mut self, state: ShowProjectionState)`.
- Produces: `ProjectionCache::set_active_generation(&mut self, generation: u64)`.
- Produces: projector event handling for `AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation })`.
- Produces: `ProjectionCache::apply_lv1_event(&mut self, generation: u64, event: &Lv1Event) -> bool` returns false for stale events.
- Produces: `ProjectionCache::build_snapshot(&mut self) -> AppViewState` with no show argument.

- [ ] **Step 1: Write failing projector show apply test**

In `src-tauri/src/projector/cache.rs` tests, add:

```rust
#[test]
fn cache_applies_show_projection_state_without_show_pull() {
    let mut cache = ProjectionCache::new();
    cache.apply_show_state(ShowProjectionState {
        lockout: true,
        scene_configs: vec![scene_config("1::Intro", 1, "Intro")],
        cued_scene_id: Some("1::Intro".to_string()),
        selected_scene_id: Some("1::Intro".to_string()),
        show_file_path: Some(std::path::PathBuf::from("/tmp/test.lv1show")),
        show_file_name: "test.lv1show".to_string(),
        show_file_dirty: true,
        show_file_last_saved_at: Some("2026-01-01T00:00:00.000Z".to_string()),
        discovered_lv1_systems: Vec::new(),
        connected_lv1_identity: None,
        pending_lv1_identity: None,
        reconnect: ReconnectState::default(),
        last_event_at: None,
    });

    let snapshot = cache.build_snapshot();

    assert!(snapshot.lockout);
    assert_eq!(snapshot.scene_configs.len(), 1);
    assert_eq!(snapshot.selected_scene_id, Some("1::Intro".to_string()));
    assert_eq!(snapshot.show_file_name, "test.lv1show");
    assert!(snapshot.show_file_dirty);
}
```

- [ ] **Step 2: Write failing stale generation projector test**

In `src-tauri/src/projector/cache.rs` tests, add:

```rust
#[test]
fn cache_ignores_stale_runtime_events() {
    let mut cache = ProjectionCache::new();
    cache.set_active_generation(2);

    let changed = cache.apply_lv1_event(1, &Lv1Event::Connected);
    let snapshot = cache.build_snapshot();

    assert!(!changed);
    assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
}
```

Add a connected-generation acceptance test:

```rust
#[test]
fn cache_accepts_runtime_events_after_active_generation_changes() {
    let mut cache = ProjectionCache::new();
    cache.set_active_generation(1);

    let changed = cache.apply_lv1_event(1, &Lv1Event::Connected);
    let snapshot = cache.build_snapshot();

    assert!(changed);
    assert_eq!(snapshot.connection, AppConnectionState::Connected);
}
```

- [ ] **Step 3: Write failing disconnect metadata guard test**

In `src-tauri/src/projector/cache.rs` tests, add:

```rust
#[test]
fn lv1_disconnect_does_not_clear_show_owned_connection_metadata() {
    let mut cache = ProjectionCache::new();
    cache.set_active_generation(7);
    cache.apply_show_state(show_projection_with_connected_identity("10.0.0.2"));

    let changed = cache.apply_lv1_event(7, &Lv1Event::Disconnected {
        reason: "test".to_string(),
    });
    let snapshot = cache.build_snapshot();

    assert!(changed);
    assert_eq!(snapshot.connection, AppConnectionState::Disconnected);
    assert!(snapshot.connected_lv1_identity.is_some());
}
```

Add test helper `show_projection_with_connected_identity(host: &str) -> ShowProjectionState` in the same test module.

- [ ] **Step 4: Run failing projector tests**

Run:

```bash
cargo nextest run -p advanced-show-control projector::cache
```

Expected: fail because methods/signatures do not exist and `build_snapshot` still requires a show argument.

- [ ] **Step 5: Move show-owned cache fields into ShowProjectionState cache**

In `ProjectionCache`, replace individual show-owned fields with:

```rust
active_generation: u64,
show_state: ShowProjectionState,
```

Implement `Default` for `ShowProjectionState`:

```rust
impl Default for ShowProjectionState {
    fn default() -> Self {
        Self {
            lockout: false,
            scene_configs: Vec::new(),
            cued_scene_id: None,
            selected_scene_id: None,
            show_file_path: None,
            show_file_name: "Untitled Show".to_string(),
            show_file_dirty: false,
            show_file_last_saved_at: None,
            discovered_lv1_systems: Vec::new(),
            connected_lv1_identity: None,
            pending_lv1_identity: None,
            reconnect: ReconnectState::default(),
            last_event_at: None,
        }
    }
}
```

- [ ] **Step 6: Implement apply methods**

In `src-tauri/src/projector/cache.rs`:

```rust
pub fn set_active_generation(&mut self, generation: u64) {
    self.active_generation = generation;
}

pub fn apply_show_state(&mut self, state: ShowProjectionState) {
    self.show_state = state;
}

pub fn apply_lv1_event(&mut self, generation: u64, event: &Lv1Event) -> bool {
    if generation != self.active_generation {
        return false;
    }
    match event {
        Lv1Event::Disconnected { .. } => {
            self.lv1_snapshot = None;
        }
        // keep existing LV1-owned field behavior for other variants
    }
    true
}
```

Do not clear `connected_lv1_identity`, `pending_lv1_identity`, or `reconnect` here.

- [ ] **Step 7: Remove show parameter from build_snapshot**

Change:

```rust
pub fn build_snapshot(&mut self, show: ShowSnapshot) -> AppViewState
```

to:

```rust
pub fn build_snapshot(&mut self) -> AppViewState
```

Populate show-owned `AppViewState` fields from `self.show_state`.

- [ ] **Step 8: Update projector runtime**

In `src-tauri/src/projector/runtime.rs`, remove `shell_state` from `ProjectorInputs`. In event application:

```rust
match app_event {
    AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation }) => {
        cache.set_active_generation(generation);
        true
    }
    AppEvent::Lv1(generated) => cache.apply_lv1_event(generated.generation, &generated.event),
    AppEvent::Fade(generated) => cache.apply_fade_event(generated.generation, &generated.event),
    AppEvent::SceneRecall(generated) => cache.apply_scene_recall_event(generated.generation, &generated.event),
    AppEvent::Show(ShowEvent::StateChanged { state, .. }) => {
        cache.apply_show_state(state.clone());
        true
    }
}
```

On tick:

```rust
let snapshot = cache.build_snapshot();
emit_snapshot(&app, &snapshot);
```

- [ ] **Step 9: Add static guard against show pulls**

In `projector::runtime::tests::projector_runtime_does_not_call_owner_side_effects`, add forbidden terms:

```rust
["get", "snapshot"].join("_")
```

and direct text checks for:

```rust
"ShowStateHandle"
"show."
"shell_state"
```

- [ ] **Step 10: Run projector tests**

Run:

```bash
cargo nextest run -p advanced-show-control projector
```

Expected: pass after updating older projector tests to new API.

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/projector src-tauri/src/show/events.rs
git commit -m "refactor: project show state from events"
```

---

### Task 5: Move Runtime Handles And Frontend Readiness Into AppLifecycle

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/runtime/commands.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/ui/commands.rs`

**Interfaces:**
- Produces: `AppLifecycle::new(event_bus: AppEventBus, show: ShowStateHandle) -> Self`.
- Produces: `AppLifecycle::frontend_ready<R: Runtime>(&self, app: AppHandle<R>, logs: broadcast::Receiver<UiLogEvent>) -> Result<(), String>`.
- Produces: `AppLifecycle::begin_connecting(&self) -> Option<u64>`.
- Produces: `AppLifecycle::install_runtime(&self, generation: u64, handles: RuntimeHandles) -> Result<(), RuntimeHandles>`.
- Produces: app-lifetime projector handle storage outside `RuntimeHandles`.
- Produces: app-lifetime/show-capable `AppCommandBus` available before LV1 runtime connection.
- Produces: `AppLifecycle::current_command_bus(&self) -> AppCommandBus`.
- Removes: `ActiveCommandBus`.

- [ ] **Step 1: Write failing lifecycle tests**

In `src-tauri/src/lifecycle/mod.rs` tests, replace ActiveCommandBus tests with:

```rust
#[tokio::test]
async fn lifecycle_allocates_monotonic_generations() {
    let event_bus = AppEventBus::default();
    let show = ShowStateHandle::new_empty(event_bus.clone());
    let lifecycle = AppLifecycle::new(event_bus, show);

    let first = lifecycle.begin_connecting().await.unwrap();
    lifecycle.abort_current_runtime().await;
    let second = lifecycle.begin_connecting().await.unwrap();

    assert!(second > first);
}

#[tokio::test]
async fn lifecycle_rejects_stale_runtime_install() {
    let event_bus = AppEventBus::default();
    let show = ShowStateHandle::new_empty(event_bus.clone());
    let lifecycle = AppLifecycle::new(event_bus, show);
    let stale = lifecycle.begin_connecting().await.unwrap();
    let current = lifecycle.begin_connecting().await.unwrap();

    let rejected = lifecycle
        .install_runtime(stale, RuntimeHandles::default())
        .await
        .is_err();

    assert!(rejected);
    assert_eq!(lifecycle.active_generation().await, current);
}

#[tokio::test]
async fn lifecycle_publishes_active_generation_when_connecting_begins() {
    let event_bus = AppEventBus::default();
    let mut rx = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus.clone());
    let lifecycle = AppLifecycle::new(event_bus, show);

    let generation = lifecycle.begin_connecting().await.unwrap();

    let event = rx.recv().await.unwrap();
    assert!(matches!(
        event,
        AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation: event_generation })
            if event_generation == generation
    ));
}

#[tokio::test]
async fn lifecycle_exposes_show_command_bus_before_runtime_connection() {
    let event_bus = AppEventBus::default();
    let show = ShowStateHandle::new_empty(event_bus.clone());
    let lifecycle = AppLifecycle::new(event_bus, show);

    let bus = lifecycle.current_command_bus().await;
    assert!(bus.has_show_target_for_test());
}
```

- [ ] **Step 2: Run failing lifecycle tests**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle
```

Expected: fail because new lifecycle API does not exist.

- [ ] **Step 3: Move RuntimeHandles into lifecycle**

In `src-tauri/src/lifecycle/mod.rs`, define:

```rust
#[derive(Default)]
pub struct RuntimeHandles {
    pub lv1: Option<crate::lv1::handle::Lv1ActorHandle>,
    pub fade: Option<crate::fade::handle::FadeEngineHandle>,
    pub scene_recall_fader: Option<JoinHandle<()>>,
    pub lifecycle_event_monitor: Option<JoinHandle<()>>,
    pub show_scene_list_monitor: Option<JoinHandle<()>>,
}
```

Move `abort_all` implementation from `ShellState::RuntimeHandles` into this struct, but do not include or abort the app-lifetime projector in `RuntimeHandles`.

- [ ] **Step 4: Implement lifecycle inner state**

In `src-tauri/src/lifecycle/mod.rs`:

```rust
#[derive(Default)]
struct LifecycleInner {
    generation: u64,
    connecting: bool,
    frontend_ready: bool,
    handles: RuntimeHandles,
    projector: Option<JoinHandle<()>>,
    command_bus: AppCommandBus,
}

#[derive(Clone)]
pub struct AppLifecycle {
    inner: Arc<Mutex<LifecycleInner>>,
    event_bus: AppEventBus,
    show: ShowStateHandle,
}
```

Implement `new`, `begin_connecting`, `active_generation`, `install_runtime`, `clear_runtime_handles`, `abort_current_runtime`, and `current_command_bus`. `new` must create an app-lifetime `AppCommandBus` with show target installed so disconnected show/app/session commands work before LV1 connects. Runtime install updates only generation-scoped LV1/fade targets and task handles; it must not replace the app-lifetime bus.

`begin_connecting` must publish `AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation })` after incrementing the active generation, so projector and show-owned runtime listeners accept events from the new runtime.

- [ ] **Step 5: Implement frontend readiness projector start**

Add to `AppLifecycle`:

```rust
pub async fn frontend_ready<R: Runtime>(
    &self,
    app: AppHandle<R>,
    logs: tokio::sync::broadcast::Receiver<crate::logging::UiLogEvent>,
) -> Result<(), String> {
    let mut inner = self.inner.lock().await;
    if inner.frontend_ready {
        return Ok(());
    }
    inner.frontend_ready = true;
    let generation = inner.generation;
    let (start_tx, start_rx) = tokio::sync::oneshot::channel();
    inner.projector = Some(crate::projector::spawn_projector(crate::projector::ProjectorInputs {
        app,
        active_generation: generation,
        events: self.event_bus.subscribe(),
        logs,
        start_rx,
    }));
    drop(inner);
    let initial = self.show.initial_projection_state().await;
    self.event_bus.publish(AppEvent::Show(ShowEvent::StateChanged {
        reason: ShowProjectionReason::ShowReplaced,
        state: initial,
    }));
    let _ = start_tx.send(());
    Ok(())
}
```

Use this construction with the Task 4 `ProjectorInputs` shape: subscriptions are registered before `start_tx`, the initial show state event is published before first emission, and runtime producers are not started before `frontend_ready`.

- [ ] **Step 6: Remove ActiveCommandBus**

Delete `ActiveCommandBus`. Replace `command_bus_holder()` callers with `current_command_bus()` or lifecycle methods. Transitional compile fixes may touch `commands.rs`; later tasks move those commands.

- [ ] **Step 7: Run lifecycle tests**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle
```

Expected: pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lifecycle/mod.rs src-tauri/src/runtime/commands.rs src-tauri/src/ui src-tauri/src/commands.rs
git commit -m "refactor: move runtime lifecycle into AppLifecycle"
```

---

### Task 6: Move Connection/Discovery/Reconnect Commands Through Show And Lifecycle

**Files:**
- Modify: `src-tauri/src/runtime/commands.rs`
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/ui/commands.rs`
- Modify: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: App-lifetime event bus and lifecycle from Task 5.
- Consumes: app-lifetime/show-capable command bus from Task 5; show-owned commands remain available while disconnected.
- Produces: `AppCommandBus::set_discovered_lv1_systems(systems) -> Result<ShowCommandResult, AppCommandError>`.
- Produces: `AppCommandBus::set_pending_lv1_identity(identity) -> Result<ShowCommandResult, AppCommandError>`.
- Produces: lifecycle-owned `connect_to_identity` orchestration.
- Produces: lifecycle-owned disconnect cleanup that calls show command for metadata updates.

- [ ] **Step 1: Write failing app-lifetime bus test**

In `src-tauri/src/lifecycle/mod.rs` tests, add:

```rust
#[tokio::test]
async fn lifecycle_uses_app_lifetime_event_bus_for_runtime_handles() {
    let event_bus = AppEventBus::default();
    let show = ShowStateHandle::new_empty(event_bus.clone());
    let lifecycle = AppLifecycle::new(event_bus.clone(), show);
    let generation = lifecycle.begin_connecting().await.unwrap();
    let mut rx = event_bus.subscribe();

    event_bus.publish_lv1(generation, Lv1Event::Connected);

    let event = rx.recv().await.unwrap();
    assert!(matches!(event, AppEvent::Lv1(generated) if generated.generation == generation));
}

#[tokio::test]
async fn disconnected_show_commands_use_app_lifetime_command_bus() {
    let event_bus = AppEventBus::default();
    let show = ShowStateHandle::new_empty(event_bus.clone());
    let lifecycle = AppLifecycle::new(event_bus, show);
    let bus = lifecycle.current_command_bus().await;

    let result = bus.set_lockout(true).await.unwrap();

    assert!(result.changed);
}
```

- [ ] **Step 2: Write failing disconnect show metadata test**

In `src-tauri/src/show/commands.rs` tests:

```rust
#[tokio::test]
async fn active_generation_disconnect_clears_show_owned_connection_metadata() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    establish_connected_lv1_identity(&show, identity("10.0.0.2")).await;
    drain_show_events(&mut events).await;

    set_active_runtime_generation(&show, 9).await;
    handle_runtime_disconnected(&show, 9, "test disconnect".to_string()).await;

    let event = events.recv().await.unwrap();
    match event {
        AppEvent::Show(ShowEvent::StateChanged { reason, state }) => {
            assert_eq!(reason, ShowProjectionReason::Disconnected);
            assert!(state.connected_lv1_identity.is_none());
            assert!(state.pending_lv1_identity.is_none());
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

Also add a show runtime-listener test that publishes `RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 9 }`, then publishes `AppEvent::Lv1(GeneratedAppEvent { generation: 8, event: Lv1Event::Disconnected { ... } })`, and asserts show-owned connection metadata is not cleared. This test should exercise the listener/orchestrator path, not only the direct command function.

Add a stale disconnect test:

```rust
#[tokio::test]
async fn stale_generation_disconnect_does_not_clear_show_owned_connection_metadata() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    establish_connected_lv1_identity(&show, identity("10.0.0.2")).await;
    set_active_runtime_generation(&show, 9).await;
    drain_show_events(&mut events).await;

    let result = handle_runtime_disconnected(&show, 8, "stale disconnect".to_string()).await;

    assert!(!result.changed);
    assert!(events.try_recv().is_err());
    let state = show.projection_state_for_test().await;
    assert!(state.connected_lv1_identity.is_some());
}
```

- [ ] **Step 3: Run failing tests**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle::tests::lifecycle_uses_app_lifetime_event_bus_for_runtime_handles show::commands::tests::active_generation_disconnect_clears_show_owned_connection_metadata
```

Expected: fail until commands/lifecycle are wired.

- [ ] **Step 4: Extend AppCommandBus for show metadata commands**

In `src-tauri/src/runtime/commands.rs`, add methods that call `show::commands`:

```rust
pub async fn set_discovered_lv1_systems(&self, systems: Vec<DiscoveredLv1System>) -> Result<ShowCommandResult, AppCommandError>
pub async fn set_pending_lv1_identity(&self, identity: Option<Lv1SystemIdentity>) -> Result<ShowCommandResult, AppCommandError>
pub async fn establish_connected_lv1_identity(&self, identity: Lv1SystemIdentity) -> Result<ShowCommandResult, AppCommandError>
pub async fn clear_connected_lv1_identity(&self) -> Result<ShowCommandResult, AppCommandError>
pub async fn set_reconnect_state(&self, reconnect: ReconnectState) -> Result<ShowCommandResult, AppCommandError>
pub async fn set_active_runtime_generation(&self, generation: u64) -> Result<ShowCommandResult, AppCommandError>
pub async fn handle_runtime_disconnected(&self, generation: u64, reason: String) -> Result<ShowCommandResult, AppCommandError>
```

`handle_runtime_disconnected` must forward the supplied generation to `show::commands::handle_runtime_disconnected`; callers that process runtime facts pass `generated.generation`.

- [ ] **Step 5: Move discovery command to thin wrapper**

In `src-tauri/src/ui/commands.rs`, implement `refresh_lv1_discovery` as:

```rust
#[tauri::command]
pub async fn refresh_lv1_discovery(
    lifecycle: State<'_, AppLifecycle>,
    timeout_ms: Option<u64>,
) -> Result<crate::show::commands::ShowCommandResult, String> {
    let started = std::time::Instant::now();
    let timeout = timeout_ms
        .unwrap_or(DEFAULT_DISCOVERY_TIMEOUT_MS)
        .clamp(MIN_DISCOVERY_TIMEOUT_MS, MAX_DISCOVERY_TIMEOUT_MS);
    let entries = spawn_blocking(move || {
        crate::lv1::discovery::discover(crate::lv1::discovery::DiscoverOptions {
            timeout: std::time::Duration::from_millis(timeout),
            ..Default::default()
        })
    })
    .await
    .map_err(|err| format!("Failed to run LV1 discovery task: {err}"))?
    .map_err(|err| format!("Failed to discover LV1 systems: {err}"))?;

    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);
    let systems = entries
        .iter()
        .filter_map(crate::connection_state::identity_from_discovery)
        .map(|identity| crate::connection_state::DiscoveredLv1System {
            identity,
            latency_ms: Some(latency_ms),
            status: crate::connection_state::DiscoveredLv1Status::Available,
        })
        .collect();

    let bus = lifecycle.current_command_bus().await;
    bus.set_discovered_lv1_systems(systems).await.map_err(map_app_command_error)
}
```

This still performs IO in adapter/infrastructure, but state mutation goes through the app-lifetime command bus. This command must work before any LV1 runtime is connected.

- [ ] **Step 6: Move connect orchestration into lifecycle**

In `src-tauri/src/lifecycle/mod.rs`, implement a method with this shape:

```rust
pub async fn connect_to_identity<R: Runtime>(
    &self,
    app: AppHandle<R>,
    identity: Lv1SystemIdentity,
    failure_mode: ConnectFailureMode,
) -> Result<crate::show::commands::ConnectCommandResult, String>
```

The method should:

- abort current runtime handles
- call `begin_connecting` for a new generation after old runtime handles are cleared
- create `AppCommandBus`
- set generation, LV1, fade, and show targets
- spawn LV1 actor and fade engine with the app-lifetime bus and generation
- spawn scene recall fader with generation
- install runtime handles if generation is still current
- update pending/connected identity through `AppCommandBus`
- publish show events through show commands
- never emit `app-status-changed` directly

The `begin_connecting` call inside this method is what publishes the active runtime generation event. Do not start LV1/fade producers before that event has been published.

Add or update the show-owned runtime listener so it consumes `RuntimeLifecycleEvent::ActiveGenerationChanged` by calling `AppCommandBus::set_active_runtime_generation(generation)`, and consumes generated LV1 disconnect facts by calling `AppCommandBus::handle_runtime_disconnected(generated.generation, reason)`. The listener must not clear show-owned metadata for stale disconnect facts.

Define `ConnectFailureMode` in `src-tauri/src/lifecycle/mod.rs` so lifecycle-owned connect orchestration does not depend on `ui/commands.rs`.

- [ ] **Step 7: Move disconnect handling into lifecycle and show commands**

Implement `AppLifecycle::disconnect_current_runtime(&self) -> Result<ShowCommandResult, String>`:

```rust
pub async fn disconnect_current_runtime(&self) -> Result<ShowCommandResult, String> {
    let generation = self.active_generation().await;
    self.abort_current_runtime().await;
    Ok(crate::show::commands::handle_runtime_disconnected(
        &self.show,
        generation,
        "Disconnected by user".to_string(),
    )
    .await)
}
```

Do not fetch `current_command_bus()` after aborting runtime handles in this method. The disconnect metadata update uses the lifecycle-owned `ShowStateHandle` directly through `show::commands::handle_runtime_disconnected`.

- [ ] **Step 8: Run connection command tests**

Run:

```bash
cargo nextest run -p advanced-show-control commands::tests lifecycle show::commands
```

Expected: pass after updating tests to command-specific result types.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/lifecycle/mod.rs src-tauri/src/runtime/commands.rs src-tauri/src/show/commands.rs src-tauri/src/ui/commands.rs src-tauri/src/commands.rs
git commit -m "refactor: route connection metadata through show commands"
```

---

### Task 7: Convert Tauri Commands To Thin Wrappers And Add Frontend Ready

**Files:**
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/ui/commands.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/ui/mod.rs`
- Test: rewrite `src-tauri/src/commands.rs` tests under `ui::commands`

**Interfaces:**
- Produces: `#[tauri::command] pub async fn frontend_ready(app, lifecycle) -> Result<(), String>`.
- Produces: all mutating Tauri commands return command-specific result or `()`; none return `AppViewState`.
- Removes: direct `emit_snapshot` helper outside projector.

- [ ] **Step 1: Add static failing tests for command boundary**

Create `src-tauri/src/ui/commands_tests.rs` and add:

```rust
#[test]
fn tauri_commands_do_not_return_app_view_state() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(manifest_dir.join("src/ui/commands.rs")).unwrap();

    assert!(!source.contains("Result<AppViewState"));
    assert!(!source.contains("-> AppViewState"));
}

#[test]
fn tauri_commands_do_not_emit_app_status_changed() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(manifest_dir.join("src/ui/commands.rs")).unwrap();

    assert!(!source.contains("app-status-changed"));
    assert!(!source.contains("app.emit"));
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo nextest run -p advanced-show-control ui::tests::tauri_commands_do_not_return_app_view_state ui::tests::tauri_commands_do_not_emit_app_status_changed
```

Expected: fail while current commands return `AppViewState` and emit snapshots.

- [ ] **Step 3: Move command implementations to ui/commands.rs**

Replace any `src-tauri/src/ui/commands.rs` re-export of `crate::commands::*` with real Tauri command wrappers. Keep helper functions such as file dialogs and IO here, but route state changes through `AppCommandBus`.

Example wrapper:

```rust
#[tauri::command]
pub async fn set_lockout(
    lifecycle: State<'_, AppLifecycle>,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    let bus = lifecycle.current_command_bus().await;
    bus.set_lockout(enabled).await.map_err(map_app_command_error)
}
```

Example `frontend_ready`:

```rust
#[tauri::command]
pub async fn frontend_ready<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<(), String> {
    let logs = app.state::<UiLogReceiverState>().subscribe();
    lifecycle.frontend_ready(app, logs).await
}
```

- [ ] **Step 4: Remove direct snapshot returns from wrappers**

For each command in the invoke handler, use these return types:

- `frontend_ready -> Result<(), String>`
- `refresh_lv1_discovery -> Result<ShowCommandResult, String>`
- `new_show_file -> Result<NewShowFileResult, String>`
- `open_show_file_dialog -> Result<LoadShowFileResult, String>`
- `save_show_file -> Result<ShowCommandResult, String>`
- `save_show_file_as_dialog -> Result<ShowCommandResult, String>`
- `set_scene_duration_ms -> Result<ShowCommandResult, String>`
- `select_scene_config -> Result<SelectedSceneResult, String>`
- `cue_scene -> Result<CueSceneResult, String>`
- `recall_scene -> Result<RecallSceneResult, String>`
- `connect_lv1 -> Result<ConnectCommandResult, String>`
- `connect_lv1_system -> Result<ConnectCommandResult, String>`
- `attempt_reconnect_lv1 -> Result<ConnectCommandResult, String>`
- `startup_auto_connect_lv1 -> Result<ConnectCommandResult, String>`
- `disconnect_lv1 -> Result<ShowCommandResult, String>`
- `reconnect_timed_out -> Result<ShowCommandResult, String>`
- `abort_all_fades -> Result<(), String>`
- `store_scene_config -> Result<ShowCommandResult, String>`
- `set_channel_scoped -> Result<ShowCommandResult, String>`
- `set_all_channels_scoped -> Result<ShowCommandResult, String>`
- `set_scene_scope_faders_enabled -> Result<ShowCommandResult, String>`
- `set_scene_scope_pan_enabled -> Result<ShowCommandResult, String>`
- `set_lockout -> Result<ShowCommandResult, String>`

- [ ] **Step 5: Update invoke handler**

In `src-tauri/src/ui/mod.rs`, register `commands::frontend_ready` and remove `commands::get_app_status` from the invoke handler.

- [ ] **Step 6: Remove `src-tauri/src/commands.rs`**

Delete `src-tauri/src/commands.rs` and remove `pub mod commands;` from `src-tauri/src/lib.rs`. All Tauri command wrappers live in `src-tauri/src/ui/commands.rs`.

- [ ] **Step 7: Run command boundary tests**

Run:

```bash
cargo nextest run -p advanced-show-control ui::tests commands::tests
```

```bash
cargo nextest run -p advanced-show-control ui
```

Expected: pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/ui src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/runtime/commands.rs
git commit -m "refactor: make tauri commands thin wrappers"
```

---

### Task 8: Update React To Listener-Only App State And Frontend Ready

**Files:**
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/commands.ts`
- Modify: `ui/src/types.ts`
- Test: create `ui/src/AppRuntime.test.tsx`

**Interfaces:**
- Consumes: `frontend_ready` Tauri command from Task 7.
- Produces: `AppRuntimeServices.frontendReady: () => Promise<void>`.
- Produces: command service methods returning `Promise<void>` or named command-specific result types, not `Promise<AppViewState>`.

- [ ] **Step 1: Write failing frontend-ready order test**

Create `ui/src/AppRuntime.test.tsx` with Vitest/React Testing Library patterns used in the repo.

Preferred component test shape:

```tsx
it("registers app-status listener before signaling frontend readiness", async () => {
  const calls: string[] = [];
  const services = testServices({
    listenForAppStatus: async () => {
      calls.push("listen");
      return () => undefined;
    },
    frontendReady: async () => {
      calls.push("ready");
    },
  });

  render(<AppRuntime services={services} />);
  await waitFor(() => expect(calls).toEqual(["listen", "ready"]));
});
```

- [ ] **Step 2: Write failing command result non-application test**

Add:

```tsx
it("does not apply command return values as app state", async () => {
  const services = testServices({
    newShowFile: async () => ({ selectedSceneId: "1::Intro" }),
  });
  render(<AppRuntime services={services} />);

  await userEvent.click(screen.getByRole("button", { name: /new show/i }));

  expect(screen.queryByText("1::Intro")).not.toBeInTheDocument();
});
```

Use the current New Show button label from the rendered app shell. If that label changes during implementation, update the test query to the new visible label in the same commit.

- [ ] **Step 3: Run failing frontend tests**

Run:

```bash
npm --prefix ui run test -- AppRuntime
```

Expected: fail because services still return `AppViewState`, `runSnapshot` applies returns, and no `frontendReady` exists.

- [ ] **Step 4: Update service types**

In `ui/src/AppRuntime.tsx`, change `AppRuntimeServices`:

```ts
export type AppRuntimeServices = {
  frontendReady: () => Promise<void>;
  abortAll: () => Promise<void> | void;
  attemptReconnectLv1: () => Promise<unknown>;
  connectLv1System: (identity: Lv1SystemIdentity) => Promise<unknown>;
  disconnectLv1: () => Promise<unknown>;
  listenForAppStatus: (listener: AppStatusListener) => Promise<() => void>;
  newShowFile: () => Promise<unknown>;
  openShowFile: () => Promise<unknown>;
  reconnectTimedOut: (attempt: number) => Promise<unknown>;
  refreshLv1Discovery: () => Promise<unknown>;
  saveShowFile: () => Promise<unknown>;
  saveShowFileAs: () => Promise<unknown>;
  cueScene: (sceneId: string) => Promise<unknown>;
  recallScene: (sceneId: string) => Promise<unknown>;
  selectSceneConfig: (sceneId: string) => Promise<unknown>;
  setAllChannelsScoped: (sceneId: string, scoped: boolean) => Promise<unknown>;
  setChannelScoped: (sceneId: string, group: number, channel: number, scoped: boolean) => Promise<unknown>;
  setLockout: (enabled: boolean) => Promise<unknown>;
  setSceneDurationMs: (sceneId: string, durationMs: number) => Promise<unknown>;
  setSceneScopeFadersEnabled: (sceneId: string, enabled: boolean) => Promise<unknown>;
  setSceneScopePanEnabled: (sceneId: string, enabled: boolean) => Promise<unknown>;
  startupAutoConnectLv1: () => Promise<unknown>;
  storeSceneConfig: (sceneId: string) => Promise<unknown>;
};
```

Replace each `unknown` with a named result type when the frontend uses fields from that command. Do not use `AppViewState` as a command result.

- [ ] **Step 5: Replace runSnapshot with runCommand**

In `AppRuntime.tsx`, replace:

```ts
const runSnapshot = useCallback(async (command: () => Promise<AppViewState>) => { ... })
```

with:

```ts
const runCommand = useCallback(
  async (command: () => Promise<unknown>) => {
    setCommandError(null);
    try {
      await command();
      return true;
    } catch (error) {
      setCommandError(String(error));
      return false;
    }
  },
  [],
);
```

Update all command callbacks to use `runCommand` and remove `refreshAppState` fallback.

- [ ] **Step 6: Add frontend ready sequence**

In startup `useEffect`, change sequence to:

```ts
useEffect(() => {
  let cancelled = false;

  async function start() {
    try {
      const unlisten = await services.listenForAppStatus((snapshot) => {
        if (!cancelled && applySnapshot(snapshot)) {
          closeStartupModalIfConnected(snapshot);
        }
      });
      if (cancelled) {
        unlisten();
        return;
      }
      await services.frontendReady();
      await services.startupAutoConnectLv1();
    } catch (error) {
      if (!cancelled) {
        setCommandError(String(error));
        setConnectionModalMode("startup");
      }
    }
  }

  void start();
  return () => {
    cancelled = true;
  };
}, [applySnapshot, closeStartupModalIfConnected, services]);
```

Preserve cleanup by storing `unlisten` in a `useRef<null | (() => void)>(null)` and calling it in the effect cleanup when present.

- [ ] **Step 7: Update App.tsx invocations**

In `ui/src/App.tsx`, add:

```ts
frontendReady: () => invoke<void>("frontend_ready"),
```

Change all command invokes from `invoke<AppViewState>` to `invoke<CommandResultType>` or `invoke<void>` according to Task 7's command list. Keep `listenForAppStatus` typed as `AppViewState`.

- [ ] **Step 8: Run frontend tests and typecheck**

Run:

```bash
npm --prefix ui run typecheck
npm --prefix ui run test -- AppRuntime
```

Expected: pass.

- [ ] **Step 9: Commit**

```bash
git add ui/src/App.tsx ui/src/AppRuntime.tsx ui/src/commands.ts ui/src/types.ts ui/src/AppRuntime.test.tsx
git commit -m "refactor: use event-only frontend app state"
```

---

### Task 9: Move Show File Save/Open/New Transactions Fully Behind AppCommandBus

**Files:**
- Modify: `src-tauri/src/ui/commands.rs`
- Modify: `src-tauri/src/runtime/commands.rs`
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/show/show_file.rs`
- Tests: `src-tauri/src/show/commands.rs`, `src-tauri/src/runtime/commands.rs`, `src-tauri/src/ui/commands.rs`

**Interfaces:**
- Consumes: command-specific Tauri wrappers from Task 7.
- Produces: `AppCommandBus::export_show_file_for_save(saved_at) -> Result<ShowFile, AppCommandError>`.
- Produces: `AppCommandBus::mark_show_file_saved(path, saved_at) -> Result<ShowCommandResult, AppCommandError>`.
- Produces: `AppCommandBus::load_show_file_from_path(path, file, lv1) -> Result<LoadShowFileResult, AppCommandError>`.

- [ ] **Step 1: Write failing save transaction test**

In `src-tauri/src/show/commands.rs` tests:

```rust
#[tokio::test]
async fn export_for_save_does_not_mark_show_clean() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    new_show_file(&show, Some(recall_lv1(ConnectionStatus::Connected, "Intro"))).await.unwrap();
    set_scene_duration_ms(&show, "1::Intro".to_string(), 1500).await.unwrap();
    drain_show_events(&mut events).await;

    let _file = export_show_file_for_save(&show, "2026-01-01T00:00:00.000Z".to_string()).await;

    assert!(events.try_recv().is_err());
    let state = show.projection_state_for_test().await;
    assert!(state.show_file_dirty);
}

#[tokio::test]
async fn mark_show_file_saved_marks_clean_after_io_step() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let show = ShowStateHandle::new_empty(event_bus);
    new_show_file(&show, Some(recall_lv1(ConnectionStatus::Connected, "Intro"))).await.unwrap();
    set_scene_duration_ms(&show, "1::Intro".to_string(), 1500).await.unwrap();
    drain_show_events(&mut events).await;

    mark_show_file_saved(
        &show,
        std::path::PathBuf::from("/tmp/test.lv1show"),
        "2026-01-01T00:00:00.000Z".to_string(),
    ).await;

    let event = events.recv().await.unwrap();
    match event {
        AppEvent::Show(ShowEvent::StateChanged { reason, state }) => {
            assert_eq!(reason, ShowProjectionReason::ShowFileSaved);
            assert!(!state.show_file_dirty);
            assert_eq!(state.show_file_path, Some(std::path::PathBuf::from("/tmp/test.lv1show")));
        }
        other => panic!("unexpected event: {other:?}"),
    }
}
```

- [ ] **Step 2: Run failing save tests**

Run:

```bash
cargo nextest run -p advanced-show-control show::commands::tests::export_for_save_does_not_mark_show_clean show::commands::tests::mark_show_file_saved_marks_clean_after_io_step
```

Expected: fail until helper/test accessors exist.

- [ ] **Step 3: Add read-only test accessor gated to tests**

In `src-tauri/src/show/handle.rs`:

```rust
#[cfg(test)]
pub(crate) async fn projection_state_for_test(&self) -> ShowProjectionState {
    self.state.lock().await.projection_state()
}
```

- [ ] **Step 4: Ensure export is read-only**

In `show::commands::export_show_file_for_save`, use `show.query` and do not call `mutate_for_command`:

```rust
pub async fn export_show_file_for_save(show: &ShowStateHandle, saved_at: String) -> ShowFile {
    let state = show.query(|state| state.projection_state()).await;
    export_show_file(state, saved_at)
}
```

Update `show::show_file::export_show_file` to accept `ShowProjectionState`.

- [ ] **Step 5: Ensure UI save wrapper marks saved only after IO success**

In `src-tauri/src/ui/commands.rs`, save flow must be:

```rust
let saved_at = crate::time::current_timestamp_millis();
let file = bus.export_show_file_for_save(saved_at.clone()).await.map_err(map_app_command_error)?;
write_show_file(&path, &file)?;
create backup/prune as current behavior requires;
bus.mark_show_file_saved(path, saved_at).await.map_err(map_app_command_error)
```

Do not call `mark_show_file_saved` before `write_show_file` and backup handling return success.

- [ ] **Step 6: Run save-related tests**

Run:

```bash
cargo nextest run -p advanced-show-control show_file show::commands runtime::commands::tests::export_show_file_for_save_routes_through_show_state
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/show src-tauri/src/runtime/commands.rs src-tauri/src/ui/commands.rs
git commit -m "refactor: enforce show file save transaction"
```

---

### Task 10: Delete ShellState And AppState Shell Modules

**Files:**
- Delete: `src-tauri/src/app_state/shell.rs`
- Delete: `src-tauri/src/app_state/events.rs`
- Delete: `src-tauri/src/app_state/events_tests.rs`
- Delete: `src-tauri/src/app_state/capture_tests.rs`
- Delete: `src-tauri/src/app_state/test_support.rs`
- Delete: `src-tauri/src/app_state/show_file_mapping.rs`
- Delete: `src-tauri/src/app_state/show_file_mapping_tests.rs`
- Modify: `src-tauri/src/app_state/mod.rs`
- Modify: all imports referencing `ShellState`, `RuntimeHandles`, `ProjectionOutcome`, or shell helpers

**Interfaces:**
- Consumes: prior tasks' replacement lifecycle/show/projector APIs.
- Produces: `app_state` module as view DTO exports only.

- [ ] **Step 1: Add failing static guard tests**

Create `src-tauri/src/architecture_tests.rs`:

```rust
#[test]
fn shell_state_source_is_removed() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    assert!(!manifest_dir.join("src/app_state/shell.rs").exists());

    let src = std::fs::read_to_string(manifest_dir.join("src/app_state/mod.rs")).unwrap();
    assert!(!src.contains("ShellState"));
    assert!(!src.contains("RuntimeHandles"));
}

#[test]
fn no_active_command_bus_source_remains() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lifecycle = std::fs::read_to_string(manifest_dir.join("src/lifecycle/mod.rs")).unwrap();
    assert!(!lifecycle.contains("ActiveCommandBus"));
}
```

Register `mod architecture_tests;` in `src-tauri/src/lib.rs` under `#[cfg(test)]`.

- [ ] **Step 2: Run failing guard tests**

Run:

```bash
cargo nextest run -p advanced-show-control architecture_tests
```

Expected: fail because shell files still exist.

- [ ] **Step 3: Remove shell module exports**

Change `src-tauri/src/app_state/mod.rs` to:

```rust
mod view;

pub use view::{
    AppConnectionState, AppFadeState, AppLogEntry, AppViewState, ChannelSummary, LogSeverity,
    SceneSummary,
};
```

- [ ] **Step 4: Delete shell-owned files**

Delete the listed `app_state` shell/test/mapping files with apply_patch delete operations.

- [ ] **Step 5: Fix remaining imports**

Run searches:

```bash
rg "ShellState|RuntimeHandles|ProjectionOutcome|snapshot_for_generation|mark_show_file_dirty|apply_loaded_show_file_metadata|apply_new_show_file_metadata|ActiveCommandBus" src-tauri/src
```

Expected after fixes: no matches in `src-tauri/src`.

Replace remaining code with the concrete APIs from prior tasks: `AppLifecycle`, `ShowStateHandle`, `show::commands`, `AppCommandBus`, `ProjectionCache`, and `ProjectorInputs`.

- [ ] **Step 6: Run architecture tests**

Run:

```bash
cargo nextest run -p advanced-show-control architecture_tests
```

Expected: pass.

- [ ] **Step 7: Run broad Rust compile check**

Run:

```bash
cargo check --workspace --all-targets
```

Expected: pass. Fix compile errors by removing stale imports and updating tests.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/app_state src-tauri/src/lib.rs src-tauri/src/architecture_tests.rs src-tauri/src
git commit -m "refactor: remove ShellState"
```

---

### Task 11: Finalize Projector-Only AppViewState Emission And Guardrails

**Files:**
- Modify: `src-tauri/src/projector/runtime.rs`
- Modify: `src-tauri/src/logging.rs`
- Modify: `src-tauri/src/ui/commands.rs`
- Modify: `src-tauri/src/architecture_tests.rs`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

**Interfaces:**
- Consumes: no shell state, thin Tauri wrappers, projector cache.
- Produces: exactly one `app.emit("app-status-changed"...)` call site in projector.
- Produces: static guardrails matching the spec.

- [ ] **Step 1: Add projector-only emit guard**

In `src-tauri/src/architecture_tests.rs`:

```rust
#[test]
fn only_projector_emits_app_status_changed() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/projector/runtime.rs",
        "src/ui/commands.rs",
        "src/logging.rs",
        "src/lifecycle/mod.rs",
        "src/show/commands.rs",
    ];

    for file in files {
        let source = std::fs::read_to_string(manifest_dir.join(file)).unwrap();
        let contains_emit = source.contains("app-status-changed") && source.contains("app.emit");
        if file == "src/projector/runtime.rs" {
            assert!(contains_emit, "projector should emit app-status-changed");
        } else {
            assert!(!contains_emit, "{file} must not emit app-status-changed");
        }
    }
}
```

- [ ] **Step 2: Add no show pull guard**

In `src-tauri/src/architecture_tests.rs`:

```rust
#[test]
fn projector_does_not_pull_from_show() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(manifest_dir.join("src/projector/runtime.rs")).unwrap();

    for forbidden in ["ShowStateHandle", "get_snapshot", "projection_state", "show.", "shell_state"] {
        assert!(!source.contains(forbidden), "projector runtime contains forbidden {forbidden}");
    }
}
```

- [ ] **Step 3: Add no isolated event bus guard**

In `src-tauri/src/architecture_tests.rs`:

```rust
#[test]
fn connect_paths_do_not_create_isolated_event_bus() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let lifecycle = std::fs::read_to_string(manifest_dir.join("src/lifecycle/mod.rs")).unwrap();
    let ui_commands = std::fs::read_to_string(manifest_dir.join("src/ui/commands.rs")).unwrap();

    assert!(!lifecycle.contains("AppEventBus::default()"));
    assert!(!ui_commands.contains("AppEventBus::default()"));
}
```

- [ ] **Step 4: Run failing guard tests**

Run:

```bash
cargo nextest run -p advanced-show-control architecture_tests
```

Expected: fail while direct emit or show pull remains.

- [ ] **Step 5: Remove remaining direct emit/log paths**

Search:

```bash
rg "app-status-changed|app\.emit|emit_snapshot|get_app_status|ShellState|ActiveCommandBus" src-tauri/src ui/src
```

Required final state:

- Only `src-tauri/src/projector/runtime.rs` contains `app.emit("app-status-changed"`.
- UI may contain the string `app-status-changed` only in listener setup.
- No Rust code references `ShellState` or `ActiveCommandBus`.
- No Tauri command returns `AppViewState`.

- [ ] **Step 6: Update docs/architecture.md**

Replace transitional statements with final architecture:

- `ShowState` owns app/session state and publishes `ShowEvent { state: ShowProjectionState }`.
- `AppLifecycle` owns generation/runtime handles/app-lifetime command bus/frontend readiness.
- `AppEventBus` is app-lifetime and generated runtime events carry generation.
- Projector emits `app-status-changed` and does not pull from show.
- React state updates only from `app-status-changed`.

- [ ] **Step 7: Run architecture guard tests**

Run:

```bash
cargo nextest run -p advanced-show-control architecture_tests projector logging ui
```

Expected: pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/projector src-tauri/src/logging.rs src-tauri/src/ui src-tauri/src/architecture_tests.rs docs/architecture.md docs/roadmap.md
git commit -m "test: guard show state cutover architecture"
```

---

### Task 12: Full Verification And Cleanup

**Files:**
- Modify: files reported by verification failures.
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap.md`

**Interfaces:**
- Consumes: all previous tasks.
- Produces: verified cutover branch.

- [ ] **Step 1: Run Rust formatting**

Run:

```bash
cargo fmt --all -- --check
```

Expected: pass. If it fails, run `cargo fmt --all`, inspect `git diff`, and include formatting changes in the final cleanup commit.

- [ ] **Step 2: Run Rust clippy**

Run:

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: pass. Fix all warnings without adding broad `allow` attributes.

- [ ] **Step 3: Run Rust tests**

Run:

```bash
cargo nextest run --workspace
```

Expected: all tests pass.

- [ ] **Step 4: Run Rust build**

Run:

```bash
cargo build --workspace
```

Expected: pass, including the preserved `lv1-probe` binary.

- [ ] **Step 5: Run frontend typecheck**

Run:

```bash
npm --prefix ui run typecheck
```

Expected: pass.

- [ ] **Step 6: Run frontend tests**

Run:

```bash
npm --prefix ui run test
```

Expected: pass.

- [ ] **Step 7: Run frontend build**

Run:

```bash
npm --prefix ui run build
```

Expected: pass.

- [ ] **Step 8: Inspect final architecture searches**

Run:

```bash
rg "ShellState|ActiveCommandBus|emit_snapshot|Result<AppViewState|get_app_status|ShowSnapshot" src-tauri/src ui/src
```

Expected:

- no `ShellState`
- no `ActiveCommandBus`
- no `emit_snapshot`
- no mutating command returning `Result<AppViewState`
- no frontend command service returning `AppViewState`
- no source-of-truth type named `ShowSnapshot`

Remove every `ShowSnapshot` match from `src-tauri/src` and `ui/src`. Historical docs outside those paths can remain unchanged.

- [ ] **Step 9: Commit cleanup fixes**

When verification caused file changes, commit them:

```bash
git add src-tauri ui docs
git commit -m "fix: complete show state cutover verification"
```

When verification caused no file changes, do not create an empty commit.

- [ ] **Step 10: Request final code review**

Use superpowers:requesting-code-review with:

- Description: `Single cutover removing ShellState and routing app/session state through show commands, AppLifecycle, generated AppEvent, and projector-only AppViewState emission.`
- Base SHA: the commit before Task 1.
- Head SHA: current branch HEAD.
- Requirements: `docs/superpowers/specs/2026-06-20-show-state-command-cutover-design.md` and this plan.

Address Critical and Important findings before merge.
