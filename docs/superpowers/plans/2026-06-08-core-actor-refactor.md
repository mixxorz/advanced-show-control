# Core Actor Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the backend into independent bus-connected core modules, with `src-tauri` reduced to Tauri/native adaptation and UI projection.

**Architecture:** `Lv1Actor`, `FadeEngine`, `ShowState`, and `SceneRecallFader` are independent core modules. They communicate through `AppEventBus` and `AppCommandBus`; no central `AppRuntime` is introduced. `SceneRecallFader` owns recall decisions, while `ShowState` owns app-managed show data only.

**Tech Stack:** Rust 2024, Tokio, Tauri 2, React/TypeScript, Cargo workspace.

---

## Reference Documents

- Design spec: `docs/superpowers/specs/2026-06-08-core-actor-refactor-design.md`
- Current architecture doc to update: `docs/architecture.md`
- Project safety rules: `AGENTS.md`

## File Structure Target

Create or modify these core files:

- `src/lib.rs`: export new `show` and `scene_recall` modules.
- `src/lv1/mod.rs`: export standardized LV1 module files.
- `src/lv1/actor.rs`: actor loop and `spawn_actor`, moved from current `state.rs`.
- `src/lv1/handle.rs`: `Lv1ActorHandle`.
- `src/lv1/commands.rs`: `Lv1Command`.
- `src/lv1/events.rs`: `Lv1Event` and actor-facing errors if appropriate.
- `src/lv1/state.rs`: private `ActorState` and state mutation helpers.
- `src/lv1/types.rs`: public LV1 data types moved from `model.rs`.
- `src/fade/mod.rs`: export standardized fade module files.
- `src/fade/actor.rs`: actor loop and `spawn_engine`, moved from current `engine.rs`.
- `src/fade/handle.rs`: `FadeEngineHandle`.
- `src/fade/commands.rs`: `FadeCommand`.
- `src/fade/events.rs`: `FadeEvent`.
- `src/fade/state.rs`: private `EngineState` and active fade internals.
- `src/fade/types.rs`: public fade config/data types only.
- `src/show/mod.rs`: new show module exports.
- `src/show/actor.rs`: `ShowState` command loop.
- `src/show/handle.rs`: `ShowStateHandle`.
- `src/show/commands.rs`: `ShowCommand` and reply types.
- `src/show/events.rs`: `ShowEvent`.
- `src/show/state.rs`: owned show data and mutation methods.
- `src/show/types.rs`: `SceneConfig`, `ChannelConfig`, `ChannelRef`, `ShowSnapshot`.
- `src/show/capture.rs`: store/scope/duration domain mutation helpers.
- `src/scene_recall/mod.rs`: new scene-recall module exports.
- `src/scene_recall/actor.rs`: `spawn_scene_recall_fader` actor loop.
- `src/scene_recall/events.rs`: `SceneRecallEvent`.
- `src/scene_recall/state.rs`: trigger gate and recall actor state.
- `src/scene_recall/policy.rs`: pure scene recall validation and `FadeConfig` construction.
- `src/runtime/commands.rs`: route LV1, fade, and show commands.
- `src/runtime/events.rs`: replace generic automation event with show and scene-recall event envelopes.

Modify these Tauri files:

- `src-tauri/src/commands.rs`: wire `ShowState`, new module paths, and Tauri command wrappers through `AppCommandBus`.
- `src-tauri/src/app_state/shell.rs`: remove domain show state ownership and keep shell/projection/native state only.
- `src-tauri/src/app_state/view.rs`: import core show types or map from core snapshots.
- `src-tauri/src/app_state/show_file_mapping.rs`: map show-file DTOs to/from core show commands/snapshots.
- `src-tauri/src/app_state/events.rs`: reduce to projection/event handling or absorb into `projection.rs`.
- `src-tauri/src/app_state/mod.rs`: remove deleted modules and export projection/log modules.
- `src-tauri/src/show_file.rs`: update imports to `lv1::types`.
- `src-tauri/src/main.rs`: remove `mod scene_recall_fader`.
- `docs/architecture.md`: rewrite to reflect the final actor/module architecture.

Delete after migration:

- `src/lv1/model.rs`
- `src/fade/engine.rs`
- `src-tauri/src/scene_recall_fader.rs`
- `src-tauri/src/app_state/scene_recall.rs`
- `src-tauri/src/app_state/capture.rs`

Do not leave compatibility re-exports for old paths.

---

### Task 1: Standardize LV1 Module File Boundaries

**Files:**
- Create: `src/lv1/actor.rs`
- Create: `src/lv1/handle.rs`
- Create: `src/lv1/commands.rs`
- Create: `src/lv1/events.rs`
- Create: `src/lv1/types.rs`
- Modify: `src/lv1/state.rs`
- Modify: `src/lv1/mod.rs`
- Delete: `src/lv1/model.rs`

- [ ] **Step 1: Move public LV1 data types into `types.rs`**

Move the contents of current `src/lv1/model.rs` into `src/lv1/types.rs`. The public names must stay the same, but the module path changes.

Expected public imports after this step:

```rust
use advanced_show_control::lv1::types::{
    ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry, SceneState,
};
```

- [ ] **Step 2: Split commands/events/handle out of the current actor file**

Move these definitions from current `src/lv1/state.rs` or `src/lv1/messages.rs`, depending on where they currently live:

```rust
// src/lv1/commands.rs
pub enum Lv1Command {
    GetState {
        reply: tokio::sync::oneshot::Sender<crate::lv1::types::Lv1StateSnapshot>,
    },
    SetGain {
        group: i32,
        channel: i32,
        gain_db: f64,
        reply: tokio::sync::oneshot::Sender<Result<(), crate::lv1::events::Lv1ActorError>>,
    },
}
```

```rust
// src/lv1/events.rs
#[derive(Debug, Clone, PartialEq)]
pub enum Lv1Event {
    Connected,
    Disconnected,
    SceneChanged(crate::lv1::types::SceneState),
    SceneListChanged(Vec<crate::lv1::types::SceneListEntry>),
    ChannelTopologyChanged(Vec<crate::lv1::types::ChannelInfo>),
    FaderChanged { group: i32, channel: i32, gain_db: f64 },
    MuteChanged { group: i32, channel: i32, muted: bool },
}
```

```rust
// src/lv1/handle.rs
#[derive(Clone)]
pub struct Lv1ActorHandle {
    tx: tokio::sync::mpsc::Sender<crate::lv1::commands::Lv1Command>,
}
```

Preserve the existing error type and methods exactly, but relocate them to the new files. Do not add old-path re-exports.

- [ ] **Step 3: Move actor loop into `actor.rs` and keep state internals in `state.rs`**

`src/lv1/actor.rs` should own `spawn_actor` and the async loop. `src/lv1/state.rs` should contain only internal mutable state and helpers.

Use this shape in `actor.rs`:

```rust
pub fn spawn_actor(
    address: String,
    port: u16,
    event_bus: crate::runtime::events::AppEventBus,
) -> crate::lv1::handle::Lv1ActorHandle {
    // Move the existing implementation here without changing behavior.
}
```

- [ ] **Step 4: Update `src/lv1/mod.rs`**

Expected module declarations:

```rust
pub mod actor;
pub mod commands;
pub mod discovery;
pub mod events;
pub mod handle;
pub mod messages;
pub mod parsers;
pub mod probe;
pub mod state;
pub mod tcp;
pub mod types;
```

Do not include `pub mod model;`.

- [ ] **Step 5: Update imports and run targeted LV1 tests**

Replace all `lv1::model::` imports with `lv1::types::`. Replace `lv1::messages::Lv1Event` imports with `lv1::events::Lv1Event` if `Lv1Event` moved out of `messages.rs`.

Run:

```bash
cargo test -p advanced-show-control lv1
```

Expected: LV1-related tests pass or reveal only import errors for old module paths. Fix all old path errors before continuing.

- [ ] **Step 6: Commit LV1 module split**

```bash
git add src/lv1 src/runtime src-tauri src tests docs/superpowers/plans/2026-06-08-core-actor-refactor.md
git commit -m "refactor: standardize lv1 actor module"
```

Only commit intended files. Do not commit unrelated user changes.

---

### Task 2: Standardize Fade Module File Boundaries

**Files:**
- Create: `src/fade/actor.rs`
- Create: `src/fade/handle.rs`
- Create: `src/fade/commands.rs`
- Create: `src/fade/events.rs`
- Create: `src/fade/state.rs`
- Modify: `src/fade/types.rs`
- Modify: `src/fade/mod.rs`
- Delete: `src/fade/engine.rs`

- [ ] **Step 1: Split command/event/handle types from current `types.rs` and `engine.rs`**

After the split, imports should look like:

```rust
use advanced_show_control::fade::commands::FadeCommand;
use advanced_show_control::fade::events::FadeEvent;
use advanced_show_control::fade::handle::FadeEngineHandle;
use advanced_show_control::fade::types::{FadeConfig, FadeSceneIdentity, FadeTarget};
```

Keep `FadeConfig`, `FadeSceneIdentity`, and `FadeTarget` in `src/fade/types.rs`.

- [ ] **Step 2: Move fade actor loop to `actor.rs`**

`src/fade/actor.rs` should expose:

```rust
pub fn spawn_engine(
    command_bus: crate::runtime::commands::AppCommandBus,
    event_bus: crate::runtime::events::AppEventBus,
) -> crate::fade::handle::FadeEngineHandle {
    // Move the existing implementation here without changing behavior.
}
```

- [ ] **Step 3: Move internal engine state to `state.rs`**

Move `EngineState`, active fade structs, and internal helpers into `src/fade/state.rs`. Keep private internals private to the fade module.

- [ ] **Step 4: Confirm no `finish_now` command exists**

Search for `finish_now`, `FinishNow`, and `finish now`.

Run:

```bash
rg "finish_now|FinishNow|finish now" src src-tauri ui docs
```

Expected: no production code references to a finish-now command. If documentation mentions removed behavior, update it.

- [ ] **Step 5: Update `src/fade/mod.rs`**

Expected module declarations:

```rust
pub mod actor;
pub mod commands;
pub mod curve;
pub mod events;
pub mod fader_law;
pub mod handle;
pub mod state;
pub mod tick;
pub mod types;
```

Do not include `pub mod engine;`.

- [ ] **Step 6: Run fade tests**

```bash
cargo test -p advanced-show-control fade
```

Expected: fade tests pass.

- [ ] **Step 7: Commit fade module split**

```bash
git add src/fade src/runtime src-tauri src docs/superpowers/plans/2026-06-08-core-actor-refactor.md
git commit -m "refactor: standardize fade actor module"
```

---

### Task 3: Add Core ShowState Actor And Types

**Files:**
- Create: `src/show/mod.rs`
- Create: `src/show/types.rs`
- Create: `src/show/state.rs`
- Create: `src/show/commands.rs`
- Create: `src/show/events.rs`
- Create: `src/show/handle.rs`
- Create: `src/show/actor.rs`
- Create: `src/show/capture.rs`
- Modify: `src/lib.rs`
- Modify: `src/runtime/commands.rs`
- Modify: `src/runtime/events.rs`

- [ ] **Step 1: Write failing show type and mutation tests**

Add tests in `src/show/state.rs` or `src/show/capture.rs` for these behaviors moved from Tauri tests:

```rust
#[test]
fn reconcile_scene_fade_configs_preserves_existing_config_for_matching_scene() {
    use crate::lv1::types::SceneListEntry;
    use crate::show::state::ShowState;
    use crate::show::types::{ChannelConfig, ChannelRef, SceneConfig};

    let mut state = ShowState::default();
    state.scene_configs = vec![SceneConfig {
        scene_id: "1::Intro".to_string(),
        scene_index: 1,
        scene_name: "Intro".to_string(),
        duration_ms: 5_000,
        channel_configs: vec![ChannelConfig { group: 0, channel: 2, fader_db: Some(-12.5) }],
        scoped_channels: vec![ChannelRef { group: 0, channel: 2 }],
    }];

    state.reconcile_scene_fade_configs(&[SceneListEntry { index: 1, name: "Intro".to_string() }]);

    assert_eq!(state.scene_configs.len(), 1);
    assert_eq!(state.scene_configs[0].duration_ms, 5_000);
    assert_eq!(state.scene_configs[0].channel_configs.len(), 1);
    assert_eq!(state.scene_configs[0].scoped_channels.len(), 1);
}
```

Also add tests for invalid duration and channel scope mutation.

- [ ] **Step 2: Run failing show tests**

```bash
cargo test -p advanced-show-control show
```

Expected: fail because `show` module does not exist yet.

- [ ] **Step 3: Implement `src/show/types.rs`**

Use these core data types, moved from Tauri view types where applicable:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelRef {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneConfig {
    pub scene_id: String,
    pub scene_index: i32,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ChannelConfig>,
    pub scoped_channels: Vec<ChannelRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ShowSnapshot {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
}

pub fn scene_id(index: i32, name: &str) -> String {
    format!("{index}::{name}")
}
```

- [ ] **Step 4: Implement `ShowState` data-only behavior**

`src/show/state.rs` should define:

```rust
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ShowState {
    pub lockout: bool,
    pub scene_configs: Vec<crate::show::types::SceneConfig>,
}
```

Add methods:

```rust
impl ShowState {
    pub fn snapshot(&self) -> crate::show::types::ShowSnapshot;
    pub fn reconcile_scene_fade_configs(&mut self, scenes: &[crate::lv1::types::SceneListEntry]);
    pub fn get_scene_config(&self, scene_id: &str) -> Option<crate::show::types::SceneConfig>;
    pub fn set_lockout(&mut self, enabled: bool) -> bool;
}
```

`set_lockout` returns `true` only when the value changed.

- [ ] **Step 5: Implement `src/show/capture.rs` mutation helpers**

Move the domain behavior from current `src-tauri/src/app_state/capture.rs` into pure helpers on `ShowState`:

```rust
impl crate::show::state::ShowState {
    pub fn store_scene_config(
        &mut self,
        scene_id: &str,
        channels: &[crate::lv1::types::ChannelInfo],
    ) -> Result<bool, String>;

    pub fn set_scene_duration_ms(
        &mut self,
        scene_id: &str,
        duration_ms: u64,
    ) -> Result<bool, String>;

    pub fn set_channel_scoped(
        &mut self,
        scene_id: &str,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<bool, String>;

    pub fn set_all_channels_scoped(
        &mut self,
        scene_id: &str,
        scoped: bool,
    ) -> Result<bool, String>;
}
```

Each method returns `Ok(true)` if domain data changed and `Ok(false)` if the request was valid but produced no change.

- [ ] **Step 6: Implement ShowState command target**

`src/show/commands.rs` should include commands and query replies:

```rust
pub enum ShowCommand {
    GetSnapshot { reply: tokio::sync::oneshot::Sender<crate::show::types::ShowSnapshot> },
    GetSceneConfig { scene_id: String, reply: tokio::sync::oneshot::Sender<Option<crate::show::types::SceneConfig>> },
    GetLockout { reply: tokio::sync::oneshot::Sender<bool> },
    SetLockout { enabled: bool, reply: tokio::sync::oneshot::Sender<Result<bool, String>> },
    SetSceneDuration { scene_id: String, duration_ms: u64, reply: tokio::sync::oneshot::Sender<Result<bool, String>> },
    SetChannelScoped { scene_id: String, group: i32, channel: i32, scoped: bool, reply: tokio::sync::oneshot::Sender<Result<bool, String>> },
    SetAllChannelsScoped { scene_id: String, scoped: bool, reply: tokio::sync::oneshot::Sender<Result<bool, String>> },
    StoreSceneConfig { scene_id: String, channels: Vec<crate::lv1::types::ChannelInfo>, reply: tokio::sync::oneshot::Sender<Result<bool, String>> },
    LoadShowData { lockout: bool, scene_configs: Vec<crate::show::types::SceneConfig>, reply: tokio::sync::oneshot::Sender<Result<(), String>> },
    ExportShowData { reply: tokio::sync::oneshot::Sender<crate::show::types::ShowSnapshot> },
    ReconcileSceneList { scenes: Vec<crate::lv1::types::SceneListEntry> },
}
```

- [ ] **Step 7: Implement `ShowStateHandle` and actor loop**

`src/show/handle.rs` should wrap a `tokio::sync::mpsc::Sender<ShowCommand>` and expose async methods matching the command names.

`src/show/actor.rs` should expose:

```rust
pub fn spawn_show_state(
    event_bus: crate::runtime::events::AppEventBus,
) -> crate::show::handle::ShowStateHandle;
```

The actor owns `ShowState`, processes commands sequentially, and publishes `AppEvent::Show(ShowEvent::StateChanged)` when data changes.

- [ ] **Step 8: Add `ShowEvent` and bus routing**

`src/show/events.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShowEvent {
    StateChanged,
    SceneConfigChanged { scene_id: String },
    LockoutChanged { enabled: bool },
}
```

Update `src/runtime/events.rs`:

```rust
pub enum AppEvent {
    Lv1(crate::lv1::events::Lv1Event),
    Fade(crate::fade::events::FadeEvent),
    Show(crate::show::events::ShowEvent),
    SceneRecall(crate::scene_recall::events::SceneRecallEvent),
    CommandFailed { command: String, message: String },
}
```

If `scene_recall` does not exist yet, create a temporary `src/scene_recall/events.rs` with the final event type in Task 5 before compiling, not a compatibility shim.

- [ ] **Step 9: Add show target to `AppCommandBus`**

`src/runtime/commands.rs` should store `show: Option<ShowStateHandle>` and expose methods:

```rust
pub async fn set_show(&self, show: Option<crate::show::handle::ShowStateHandle>);
pub async fn get_show_snapshot(&self) -> Result<crate::show::types::ShowSnapshot, AppCommandError>;
pub async fn get_scene_config(&self, scene_id: String) -> Result<Option<crate::show::types::SceneConfig>, AppCommandError>;
pub async fn get_lockout(&self) -> Result<bool, AppCommandError>;
```

Add `AppCommandError::ShowUnavailable`.

- [ ] **Step 10: Run show and runtime tests**

```bash
cargo test -p advanced-show-control show runtime
```

Expected: tests pass.

- [ ] **Step 11: Commit show actor**

```bash
git add src/show src/runtime src/lib.rs docs/superpowers/plans/2026-06-08-core-actor-refactor.md
git commit -m "feat: add core show state actor"
```

---

### Task 4: Wire ShowState Into Tauri And Remove Tauri Domain Ownership

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/app_state/view.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping.rs`
- Modify: `src-tauri/src/app_state/mod.rs`
- Delete: `src-tauri/src/app_state/capture.rs`

- [ ] **Step 1: Update Tauri view types to use core show data**

In `src-tauri/src/app_state/view.rs`, remove Tauri-local `ChannelRef`, `ChannelConfig`, and `SceneConfig` definitions. Use serializable view DTOs only if serde cannot be derived on core types.

Preferred change: derive `serde::Serialize` on core show types and import them:

```rust
pub type ChannelRef = advanced_show_control::show::types::ChannelRef;
pub type ChannelConfig = advanced_show_control::show::types::ChannelConfig;
pub type SceneConfig = advanced_show_control::show::types::SceneConfig;
```

This type alias is acceptable because it is a view alias, not an old module-path compatibility shim.

- [ ] **Step 2: Remove `scene_configs` and `lockout` from `ShellInner`**

`ShellInner` should no longer own these fields:

```rust
lockout: bool,
scene_configs: Vec<SceneConfig>,
duration_zero_skip_logs: HashSet<String>,
```

Keep `selected_scene_id`, show-file path/dirty/saved metadata, logs, discovered systems, connection identity, reconnect state, generation, and runtime handles.

- [ ] **Step 3: Add ShowState handle to runtime handles**

Runtime handles should keep the show actor handle if needed for cleanup/projection:

```rust
pub show: Option<advanced_show_control::show::handle::ShowStateHandle>,
```

Ensure `RuntimeHandles::abort_all` clears `AppCommandBus` targets. If `ShowState` is per-runtime, abort/drop it there. If show data must survive disconnect, store its handle outside per-LV1 runtime handles. Pick one model and keep it explicit.

For this app, prefer show data survives disconnect. Store `ShowStateHandle` in `ShellState` or a dedicated shell field initialized at app startup, not inside per-LV1 runtime handles.

- [ ] **Step 4: Initialize ShowState in Tauri startup**

In `src-tauri/src/main.rs` or the setup path, spawn `ShowState` once and install it into `AppCommandBus` whenever a command bus is created.

Expected connection wiring in `commands.rs` after bus creation:

```rust
let command_bus = AppCommandBus::new(event_bus.clone());
command_bus.set_show(Some(state.show_state_handle().await)).await;
```

If `ShellState` stores the handle synchronously, expose a non-async clone method.

- [ ] **Step 5: Convert Tauri show commands to `AppCommandBus` calls**

Update Tauri commands:

```rust
set_lockout
set_scene_duration_ms
store_scene_config
set_channel_scoped
set_all_channels_scoped
new_show_file
open_show_file_dialog
save_show_file
save_show_file_as_dialog
```

Each command should:

1. Use `AppCommandBus` or `ShowStateHandle` to mutate/query show data.
2. Update Tauri-only shell metadata such as dirty/path/saved timestamp.
3. Build and emit `AppViewState`.

Do not call removed `ShellState` domain methods.

- [ ] **Step 6: Update snapshot projection**

`snapshot_from_inner` cannot synchronously read `ShowState` unless it receives a `ShowSnapshot`.

Replace it with an async projection method such as:

```rust
pub async fn snapshot(&self) -> AppViewState {
    let inner = self.inner.lock().await;
    let show = self.show.get_snapshot().await.unwrap_or_default();
    snapshot_from_parts(&inner, &show)
}
```

Do not hold the shell mutex across `.await`; clone the handle first, release the lock, then query show state.

- [ ] **Step 7: Update show-file mapping**

`load_show_file_from_dto` should map `ShowFileSceneConfig` to `advanced_show_control::show::types::SceneConfig` and send `ShowCommand::LoadShowData` through the show handle.

`export_show_file_for_save` should query `ShowSnapshot` and map it to `ShowFile`.

- [ ] **Step 8: Remove `src-tauri/src/app_state/capture.rs`**

Delete the file after all domain behavior has moved to `src/show/capture.rs` and all callers use bus commands.

- [ ] **Step 9: Run Tauri app-state tests**

```bash
cargo test -p advanced-show-control-tauri app_state
```

Expected: tests pass after updating expected imports and state setup.

- [ ] **Step 10: Commit Tauri show wiring**

```bash
git add src src-tauri docs/superpowers/plans/2026-06-08-core-actor-refactor.md
git commit -m "refactor: route show state through core actor"
```

---

### Task 5: Move Scene Recall Policy And Trigger State Into Core

**Files:**
- Create: `src/scene_recall/mod.rs`
- Create: `src/scene_recall/events.rs`
- Create: `src/scene_recall/state.rs`
- Create: `src/scene_recall/policy.rs`
- Modify: `src/lib.rs`
- Modify: `src/runtime/events.rs`
- Modify: tests moved from `src-tauri/src/app_state/scene_recall_tests.rs`

- [ ] **Step 1: Write/move failing scene recall policy tests**

Move safety decision tests from `src-tauri/src/app_state/scene_recall_tests.rs` into `src/scene_recall/policy.rs` tests.

At minimum include tests for:

```rust
#[test]
fn blocks_when_lockout_enabled() { /* build RecallPolicyInput with lockout=true */ }

#[test]
fn blocks_when_scene_identity_mismatches_snapshot() { /* recalled scene != snapshot.scene */ }

#[test]
fn skips_when_scene_config_is_missing() { /* scene_config=None */ }

#[test]
fn starts_when_scene_config_and_live_topology_are_valid() { /* assert FadeConfig targets */ }
```

- [ ] **Step 2: Implement `SceneRecallEvent`**

`src/scene_recall/events.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum SceneRecallEvent {
    Skipped { scene_label: String, reason: String },
    Blocked { scene_label: String, reason: String },
    Ready { scene_label: String, target_count: usize },
    StartRequested { scene_label: String },
}
```

- [ ] **Step 3: Implement trigger state**

Move `RecallTriggerGate` into `src/scene_recall/state.rs`.

Expose:

```rust
pub struct SceneRecallState { /* trigger gate + duration zero suppressions if retained */ }

impl SceneRecallState {
    pub fn accepts(&mut self, current_scene: &crate::lv1::types::SceneState) -> bool;
    pub fn should_log_duration_zero_skip(&mut self, scene_id: &str) -> bool;
}
```

Keep tests for the 2-second arming delay and 500ms same-scene repeat suppression.

- [ ] **Step 4: Implement pure policy**

`src/scene_recall/policy.rs` should define:

```rust
pub struct RecallPolicyInput {
    pub recalled_scene: crate::lv1::types::SceneState,
    pub lv1_snapshot: crate::lv1::types::Lv1StateSnapshot,
    pub lockout: bool,
    pub scene_config: Option<crate::show::types::SceneConfig>,
}

pub enum RecallPolicyDecision {
    Start(crate::fade::types::FadeConfig),
    Skip { reason: String },
    Blocked { reason: String },
}

pub fn decide_scene_recall(input: RecallPolicyInput) -> RecallPolicyDecision;
```

This function owns exact scene identity validation, lockout decision, live topology validation, stored fader target validation, and fade config construction.

- [ ] **Step 5: Run policy tests**

```bash
cargo test -p advanced-show-control scene_recall::policy
```

Expected: pass.

- [ ] **Step 6: Commit scene recall policy**

```bash
git add src/scene_recall src/runtime src/lib.rs src-tauri/src/app_state/scene_recall_tests.rs docs/superpowers/plans/2026-06-08-core-actor-refactor.md
git commit -m "refactor: move scene recall policy to core"
```

---

### Task 6: Move SceneRecallFader Actor Into Core

**Files:**
- Create: `src/scene_recall/actor.rs`
- Modify: `src/scene_recall/mod.rs`
- Modify: `src/runtime/commands.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`
- Delete: `src-tauri/src/scene_recall_fader.rs`

- [ ] **Step 1: Move actor tests into core**

Move actor tests from `src-tauri/src/scene_recall_fader.rs` into `src/scene_recall/actor.rs` where they do not require Tauri.

Keep tests for:

- valid recall starts fade without global abort
- first sync does not trigger fade before arming
- same-scene repeat suppression
- blocked/skip recalls do not abort existing fades
- stale generation cannot start fades, or equivalent lifecycle protection if generation remains Tauri-owned

- [ ] **Step 2: Implement core actor loop**

`src/scene_recall/actor.rs` should expose:

```rust
pub fn spawn_scene_recall_fader(
    generation: u64,
    command_bus: crate::runtime::commands::AppCommandBus,
    event_bus: crate::runtime::events::AppEventBus,
) -> tokio::task::JoinHandle<()> {
    // Event-driven actor. No ShellState dependency.
}
```

The actor should:

1. Subscribe to `AppEventBus`.
2. React to `AppEvent::Lv1(Lv1Event::SceneChanged(scene))`.
3. Apply `SceneRecallState::accepts`.
4. Query `AppCommandBus::get_lv1_state`.
5. Query `AppCommandBus::get_scene_config(scene_id)` and `AppCommandBus::get_lockout`.
6. Call `decide_scene_recall`.
7. Publish `AppEvent::SceneRecall` events.
8. Call `AppCommandBus::start_fade` on `Start`.

- [ ] **Step 3: Remove all `ShellState` usage from scene recall actor**

Search:

```bash
rg "ShellState|log_scene_recall_fader|prepare_scene_recall" src/scene_recall src-tauri/src/scene_recall_fader.rs src-tauri/src/app_state
```

Expected after cleanup: no `SceneRecallFader -> ShellState` path remains. `prepare_scene_recall` names should be gone or replaced by `decide_scene_recall` in core policy.

- [ ] **Step 4: Wire Tauri connect path to core actor**

Update `src-tauri/src/commands.rs` import:

```rust
use advanced_show_control::scene_recall::actor::spawn_scene_recall_fader;
```

Remove `mod scene_recall_fader;` from `src-tauri/src/main.rs`.

- [ ] **Step 5: Delete old Tauri scene recall files**

Delete:

```text
src-tauri/src/scene_recall_fader.rs
src-tauri/src/app_state/scene_recall.rs
```

- [ ] **Step 6: Run scene recall tests**

```bash
cargo test -p advanced-show-control scene_recall
cargo test -p advanced-show-control-tauri scene_recall
```

Expected: core scene recall tests pass. Tauri command/projection tests should pass or have no remaining scene-recall-specific test module.

- [ ] **Step 7: Commit scene recall actor move**

```bash
git add src/scene_recall src/runtime src-tauri docs/superpowers/plans/2026-06-08-core-actor-refactor.md
git commit -m "refactor: move scene recall fader to core"
```

---

### Task 7: Update Tauri Projection And Logs For Show/SceneRecall Events

**Files:**
- Modify: `src-tauri/src/app_state/events.rs`
- Create or Modify: `src-tauri/src/app_state/projection.rs`
- Create or Modify: `src-tauri/src/app_state/logs.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/app_state/shell.rs`

- [ ] **Step 1: Add projection handling for `AppEvent::Show`**

The shell projector should react to `ShowEvent` by querying `ShowState` snapshot and emitting `AppViewState`.

Expected match arm shape:

```rust
AppEvent::Show(_) => {
    let snapshot = state.snapshot().await;
    emit app-status-changed snapshot;
}
```

- [ ] **Step 2: Add log/projection handling for `AppEvent::SceneRecall`**

Replace the old generic automation refresh path with scene-recall-specific handling.

Expected behavior:

```rust
AppEvent::SceneRecall(event) => {
    state.push_scene_recall_log_for_generation(generation, &event).await;
    let snapshot = state.snapshot_for_generation(generation).await;
    emit app-status-changed snapshot;
}
```

Keep logs visible for blocked/skipped/start-requested recalls.

- [ ] **Step 3: Keep `AppViewState` stable**

Run frontend typecheck after Rust changes that alter serialization:

```bash
npm run typecheck
```

Expected: TypeScript still accepts the existing `AppViewState` shape.

- [ ] **Step 4: Run Tauri command tests**

```bash
cargo test -p advanced-show-control-tauri commands::tests
```

Expected: pass.

- [ ] **Step 5: Commit projection update**

```bash
git add src-tauri src/runtime src/scene_recall src/show ui docs/superpowers/plans/2026-06-08-core-actor-refactor.md
git commit -m "refactor: project show and scene recall events"
```

---

### Task 8: Update Architecture Documentation

**Files:**
- Modify: `docs/architecture.md`
- Modify: `AGENTS.md` if architecture guidance needs a new short note

- [ ] **Step 1: Replace ShellState-centered architecture in docs**

Update `docs/architecture.md` to describe:

```text
Lv1Actor
FadeEngine
ShowState
SceneRecallFader
Tauri Shell
AppEventBus
AppCommandBus
```

Remove claims that `ShellState` owns scene configs, lockout, or scene recall validation.

- [ ] **Step 2: Document bus contracts**

Add a section explaining:

```text
AppEventBus = facts
AppCommandBus = commands and queries
No module directly reaches into another module's state
```

- [ ] **Step 3: Document scene recall ownership**

State explicitly:

```text
SceneRecallFader owns scene recall policy and decision-making.
ShowState owns show data only.
FadeEngine owns overlap behavior; no finish-now command exists.
```

- [ ] **Step 4: Document final file structure**

Include the final core module file structure from the spec.

- [ ] **Step 5: Run docs-adjacent checks**

Run a search for stale architecture claims:

```bash
rg "ShellState owns|AutomationEvent|finish_now|finish now|lv1::model|fade::engine|scene_recall_fader" docs AGENTS.md src src-tauri
```

Expected: no stale production/doc claims remain, except references in the approved spec/plan if they are describing removed old code.

- [ ] **Step 6: Commit docs update**

```bash
git add docs/architecture.md AGENTS.md docs/superpowers/plans/2026-06-08-core-actor-refactor.md
git commit -m "docs: update actor architecture"
```

---

### Task 9: Remove Compatibility Shims And Dead Code

**Files:**
- Modify/delete any stale files found by searches.

- [ ] **Step 1: Verify old module paths are gone**

Run:

```bash
rg "lv1::model|fade::engine|lv1::state::spawn_actor|fade::types::FadeCommand|fade::types::FadeEvent|messages::Lv1Event" src src-tauri tests docs
```

Expected: no old internal paths remain, except intentional historical mentions in the design spec or this plan.

- [ ] **Step 2: Verify old files are deleted**

Run:

```bash
test ! -e src/lv1/model.rs && test ! -e src/fade/engine.rs && test ! -e src-tauri/src/scene_recall_fader.rs && test ! -e src-tauri/src/app_state/scene_recall.rs && test ! -e src-tauri/src/app_state/capture.rs
```

Expected: command exits successfully.

- [ ] **Step 3: Verify no generic automation event remains for scene recall**

Run:

```bash
rg "AutomationEvent|AppEvent::Automation|RuleTriggered|scene-recall-fader" src src-tauri docs ui
```

Expected: no production code uses generic automation events for scene recall. If `scene-recall-fader` remains only as a log target string, rename it to a module-appropriate label or remove it.

- [ ] **Step 4: Run compiler to catch dead code and stale imports**

```bash
cargo build --workspace
```

Expected: build passes without dead-code warnings introduced by this refactor.

- [ ] **Step 5: Commit cleanup**

```bash
git add -A
git commit -m "refactor: remove old actor module paths"
```

---

### Task 10: Full Verification

**Files:**
- No planned edits unless verification reveals failures.

- [ ] **Step 1: Run Rust tests**

```bash
cargo test --workspace
```

Expected: all Rust tests pass.

- [ ] **Step 2: Run Rust build**

```bash
cargo build --workspace
```

Expected: workspace builds successfully.

- [ ] **Step 3: Run frontend typecheck**

```bash
npm run typecheck
```

Expected: TypeScript passes.

- [ ] **Step 4: Run frontend build**

```bash
npm run build
```

Expected: frontend builds successfully.

- [ ] **Step 5: Inspect git status and diff**

```bash
git status --short
git diff --stat
```

Expected: only intended refactor, docs, and test files are changed.

- [ ] **Step 6: Final commit if needed**

If verification required final fixes, commit them:

```bash
git add -A
git commit -m "test: verify core actor refactor"
```

If there are no changes after verification, do not create an empty commit.

---

## Self-Review Notes

- Spec coverage: the plan covers bus-connected module boundaries, no central `AppRuntime`, `ShowState` as data owner, `SceneRecallFader` as decision owner, file structure, documentation, cleanup, no compatibility shims, and no dead code.
- No backwards compatibility: tasks explicitly delete old files and reject old module-path re-exports.
- No dead code: cleanup task requires searches and workspace build.
- Risk: Task 4 must decide how `ShowState` lifetime works. The plan recommends show data survives LV1 disconnect and therefore should not be owned by per-LV1 runtime handles.
- Risk: generation guards remain safety-critical. If generation stays Tauri-owned during this refactor, scene recall actor must still receive and check the generation token before starting fades.
