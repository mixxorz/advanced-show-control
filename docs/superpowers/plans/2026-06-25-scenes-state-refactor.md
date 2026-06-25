# Scenes State Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move app-managed scene configuration state from `show` into `scenes` while preserving existing frontend command names and safety behavior.

**Architecture:** `scenes` becomes the owning actor/module for scene configuration, selection, cueing, alignment, capture/store, and recall validation. `show` keeps session metadata, lockout, connection/discovery metadata, and `.ascs` persistence orchestration, and marks dirty in response to persisted scene edit facts from `scenes`.

**Tech Stack:** Rust/Tauri backend in `src-tauri/src/`, Tokio actors with mailbox command enums, `AppEventBus` facts, `tracing` logs, `cargo nextest` tests, existing React/Tauri command names in `ui/`.

## Global Constraints

- Follow `docs/coding-conventions.md` as source of truth for implementation style.
- Prefer the smallest correct change and keep code paths explicit; this app controls live mixer faders.
- Domain state belongs to the actor/module that owns that domain.
- Actor handles remain dumb cloneable mailbox senders; do not add convenience methods hiding command construction.
- Tauri command adapters stay thin: deserialize, obtain handle, construct command, attach `oneshot`, send, await, map errors.
- Import public domain items from module roots, not private submodules.
- Use `tracing` for logs; user-facing logs need stable `event` fields and complete human-readable messages.
- Rust tests must be pure unit tests, actor tests through mailboxes/event bus/tracing, or debug smoke tests.
- Do not test side-effecting actor behavior by mutating actor internals or inspecting private state.
- Do not bypass lockout checks, exact scene identity validation, generation guards, stale-state checks, or existing fade safety behavior.
- Scene recall automation must validate before aborting an existing fade.
- Blocked, skipped, disabled, or unsafe recalls must not abort an existing fade.
- Use fresh LV1 state for recall automation where event subscriber ordering could otherwise create stale decisions.
- Keep frontend state projected from backend snapshots through `app-status-changed`.
- Do not delete unrelated unused show commands in this refactor; `ShowCommand::GetLockout` and `ShowCommand::SetDiscoveredLv1Systems` are roadmap follow-up items.
- Keep external Tauri command function names stable.
- Do not change `.ascs` scene config or cued scene schema fields.
- Commit after each task with only intended files staged.

---

## File Structure

- Create `src-tauri/src/scenes/types.rs`: scene-domain DTOs and snapshots previously in `show::types`.
- Modify `src-tauri/src/scenes/state.rs`: combine existing recall gate state with owned scene configuration state and scene edit methods.
- Create `src-tauri/src/scenes/capture.rs`: capture/store and scene scope mutation logic moved from `show::capture`.
- Create `src-tauri/src/scenes/scene_alignment.rs`: scene config alignment logic moved from `show::scene_alignment`.
- Modify `src-tauri/src/scenes/commands.rs`: add scene edit/read commands and command result types owned by `scenes`.
- Modify `src-tauri/src/scenes/events.rs`: add `ScenesProjectionState` and scene state change events.
- Modify `src-tauri/src/scenes/actor.rs`: route scene commands through `ScenesState`, publish scene facts, validate recalls from owned state, and remove `ShowStateHandle` peer dependency.
- Modify `src-tauri/src/scenes/mod.rs`: expose scene domain public interfaces from the module root.
- Modify `src-tauri/src/show/types.rs`: shrink or remove scene-bearing document types after callers move to `scenes`.
- Modify `src-tauri/src/show/state.rs`: remove scene config/cue/selection ownership; keep show metadata and dirty state.
- Modify `src-tauri/src/show/commands.rs`: remove scene commands and update persistence command result types to use `scenes` types.
- Modify `src-tauri/src/show/actor.rs`: coordinate new/open/save with `scenes`, subscribe to scene state changes, and mark dirty for persisted scene edits.
- Modify `src-tauri/src/show/show_file.rs`: keep file DTOs but convert to/from `scenes` scene document/types.
- Delete `src-tauri/src/show/capture.rs` after moved.
- Delete `src-tauri/src/show/scene_alignment.rs` after moved.
- Modify `src-tauri/src/show/events.rs`: remove scene projection fields from show projection.
- Modify `src-tauri/src/show/mod.rs`: remove scene re-exports and obsolete modules.
- Modify `src-tauri/src/projector/cache.rs`: store/apply show and scenes projection slices separately while emitting unchanged `AppViewState`.
- Modify `src-tauri/src/projector/runtime.rs`: seed and apply both show and scenes state.
- Modify `src-tauri/src/lifecycle/mod.rs`: build/wire actors without `ScenesPeers.show`, seed projector with initial scenes state, route scene handles where needed.
- Modify `src-tauri/src/ui/commands/show.rs`: route scene-related Tauri commands to `ScenesHandle` while keeping command names stable.
- Modify `src-tauri/src/ui/commands/scenes.rs`: update recall result imports to `scenes`.
- Modify `src-tauri/src/runtime/events.rs`: update tests/imports for new scene event shape if needed.
- Modify `docs/architecture.md`: update ownership table and peer wiring.
- Keep `docs/roadmap.md` dead-code note unchanged unless implementation discovers a more accurate unrelated-cleanup note.

---

### Task 1: Move Scene Types To `scenes`

**Files:**
- Create: `src-tauri/src/scenes/types.rs`
- Modify: `src-tauri/src/scenes/mod.rs`
- Modify: `src-tauri/src/show/types.rs`
- Modify: `src-tauri/src/show/show_file.rs`
- Modify: `src-tauri/src/projector/view.rs`
- Modify: compile-driven imports in `src-tauri/src/**/*.rs` and `ui` type generation consumers only if required

**Interfaces:**
- Consumes: existing `show::types::{ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles, ShowDocument}`.
- Produces: `scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles, SceneDocument}` with the same serde field names and data shapes; `ShowDocument` is temporarily retained only if needed by show persistence until Task 5.

- [ ] **Step 1: Move pure serialization tests first**

Create `src-tauri/src/scenes/types.rs` with the moved type definitions and move the serialization tests from `show/types.rs`. Name the scene snapshot `SceneDocument`:

```rust
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelRef {
    pub group: i32,
    pub channel: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelConfig {
    pub group: i32,
    pub channel: i32,
    pub fader_db: Option<f64>,
    pub pan: Option<f64>,
    pub balance: Option<f64>,
    pub width: Option<f64>,
    pub pan_mode: Option<crate::lv1::PanMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct SceneScopeToggles {
    pub faders: bool,
    pub pan: bool,
}

impl Default for SceneScopeToggles {
    fn default() -> Self {
        Self { faders: true, pan: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneConfig {
    pub internal_scene_id: Uuid,
    pub scene_index: Option<i32>,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ChannelConfig>,
    pub scoped_channels: Vec<ChannelRef>,
    pub scope_toggles: SceneScopeToggles,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneDocument {
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_internal_id: Option<Uuid>,
}

impl SceneDocument {
    pub fn empty() -> Self {
        Self { scene_configs: Vec::new(), cued_scene_internal_id: None }
    }
}
```

- [ ] **Step 2: Export scene types from the scenes module root**

In `src-tauri/src/scenes/mod.rs`, add:

```rust
mod types;

pub use types::{ChannelConfig, ChannelRef, SceneConfig, SceneDocument, SceneScopeToggles};
```

Keep existing module declarations and exports intact.

- [ ] **Step 3: Run moved type tests and verify compile failure scope**

Run: `cargo nextest run -p advanced-show-control scenes::types`

Expected: fail with unresolved imports/usages still pointing at `crate::show::{SceneConfig, ...}` or duplicate type definitions. This is the red phase proving callers have not migrated.

- [ ] **Step 4: Update imports to use `crate::scenes` for scene types**

Replace imports of scene-domain types from `crate::show` or `super::types` with `crate::scenes` in affected files. Typical replacements:

```rust
use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneDocument, SceneScopeToggles};
```

In `show/show_file.rs`, keep file DTOs in `show`, but change conversion signatures to use scenes types:

```rust
use crate::scenes::{ChannelConfig, ChannelRef, SceneConfig, SceneDocument, SceneScopeToggles};

pub struct ImportedShowFile {
    pub snapshot: SceneDocument,
    pub selected_scene_internal_id: Option<String>,
    pub report: LoadValidationReport,
    pub generated_internal_scene_ids: bool,
}

pub fn export_show_file(
    scene_document: SceneDocument,
    lockout: bool,
    saved_at: String,
) -> ShowFile {
    ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        saved_at,
        safety: ShowFileSafety { lockout },
        cued_scene_internal_id: scene_document.cued_scene_internal_id,
        scene_configs: scene_document
            .scene_configs
            .into_iter()
            .map(show_scene_to_file_scene)
            .collect(),
    }
}
```

- [ ] **Step 5: Remove scene type definitions from `show::types` only after imports compile**

Leave `show::types` with only show-owned data if any remains. If no show-owned types remain, remove `mod types;` and `pub use types::...` from `show/mod.rs` in the same commit.

- [ ] **Step 6: Run type tests**

Run: `cargo nextest run -p advanced-show-control scenes::types show::show_file`

Expected: PASS for moved serialization tests and show-file conversion tests.

- [ ] **Step 7: Commit**

```bash
git status --short
git diff -- src-tauri/src/scenes src-tauri/src/show src-tauri/src/projector src-tauri/src/runtime
git add src-tauri/src
git commit -m "refactor: move scene types to scenes"
```

---

### Task 2: Move Alignment And Capture Logic Into `scenes`

**Files:**
- Create: `src-tauri/src/scenes/scene_alignment.rs`
- Create: `src-tauri/src/scenes/capture.rs`
- Modify: `src-tauri/src/scenes/mod.rs`
- Modify: `src-tauri/src/scenes/state.rs`
- Delete later in this task: `src-tauri/src/show/scene_alignment.rs`, `src-tauri/src/show/capture.rs`
- Modify: `src-tauri/src/show/mod.rs`

**Interfaces:**
- Consumes: `scenes::{SceneConfig, SceneScopeToggles, SceneDocument}` from Task 1.
- Produces: `scenes::scene_alignment::align_scene_configs`, `scenes::scene_alignment::scene_alignment_diagnostic`, and `ScenesState` methods for scene edit/capture behavior.

- [ ] **Step 1: Move alignment tests and module**

Move `show/scene_alignment.rs` to `scenes/scene_alignment.rs`. Update imports inside the file to use `crate::scenes::SceneConfig`.

In `scenes/mod.rs`, add:

```rust
mod scene_alignment;

pub(crate) use scene_alignment::{align_scene_configs, scene_alignment_diagnostic};
```

- [ ] **Step 2: Run alignment tests before updating callers**

Run: `cargo nextest run -p advanced-show-control scenes::scene_alignment`

Expected: PASS if imports are complete, or FAIL only for stale module paths. Fix stale paths before continuing.

- [ ] **Step 3: Move capture/store methods onto `ScenesState`**

Move the methods from `show/capture.rs` into `scenes/capture.rs` as an `impl ScenesState` block. Preserve method names so later command routing remains mechanical:

```rust
impl ScenesState {
    pub fn store_scene_config(
        &mut self,
        internal_scene_id: uuid::Uuid,
        channels: &[crate::lv1::ChannelInfo],
    ) -> Result<bool, String> {
        // moved body from show::capture, updated to access self.scene_configs
    }

    pub fn set_scene_duration_ms(
        &mut self,
        internal_scene_id: uuid::Uuid,
        duration_ms: u64,
    ) -> Result<bool, String> {
        // moved body from show::capture
    }
}
```

Do not change behavior or error strings during the move.

- [ ] **Step 4: Expand `ScenesState` with scene fields and accessors**

In `scenes/state.rs`, add fields alongside existing recall gate fields:

```rust
#[derive(Debug, Default)]
pub struct ScenesState {
    scene_configs: Vec<SceneConfig>,
    cued_scene_internal_id: Option<uuid::Uuid>,
    selected_scene_internal_id: Option<String>,
    gate: RecallGate,
    last_scene_list: Option<Vec<SceneListEntry>>,
    scene_list_edit_suppressed_until: Option<Instant>,
}
```

Add pure accessors/mutators moved from `ShowState`, using `SceneDocument` for snapshots:

```rust
impl ScenesState {
    pub fn snapshot(&self) -> SceneDocument {
        SceneDocument {
            scene_configs: self.scene_configs.clone(),
            cued_scene_internal_id: self.cued_scene_internal_id,
        }
    }

    pub fn replace_snapshot(&mut self, snapshot: SceneDocument) {
        self.scene_configs = snapshot.scene_configs;
        self.cued_scene_internal_id = snapshot.cued_scene_internal_id;
        self.clear_missing_cue();
    }

    pub fn get_scene_config(&self, internal_scene_id: uuid::Uuid) -> Option<SceneConfig> {
        self.scene_configs
            .iter()
            .find(|scene| scene.internal_scene_id == internal_scene_id)
            .cloned()
    }
}
```

- [ ] **Step 5: Delete old show modules from `show/mod.rs`**

Remove:

```rust
mod capture;
mod scene_alignment;
```

Do not delete files until all imports are updated; then delete `src-tauri/src/show/capture.rs` and `src-tauri/src/show/scene_alignment.rs` in the same commit.

- [ ] **Step 6: Run moved pure tests**

Run: `cargo nextest run -p advanced-show-control scenes::scene_alignment scenes::state scenes::capture`

Expected: PASS for moved unit tests. If no `scenes::capture` tests exist by name, run `cargo nextest run -p advanced-show-control scenes` and confirm no capture-related failures.

- [ ] **Step 7: Commit**

```bash
git status --short
git diff -- src-tauri/src/scenes src-tauri/src/show
git add src-tauri/src/scenes src-tauri/src/show
git commit -m "refactor: move scene state helpers to scenes"
```

---

### Task 3: Add Scenes Projection Events And Scene Commands

**Files:**
- Modify: `src-tauri/src/scenes/events.rs`
- Modify: `src-tauri/src/scenes/commands.rs`
- Modify: `src-tauri/src/scenes/actor.rs`
- Modify: `src-tauri/src/scenes/handle.rs` tests if present
- Modify: `src-tauri/src/runtime/events.rs` tests if needed

**Interfaces:**
- Consumes: `ScenesState` scene methods from Task 2.
- Produces: `ScenesProjectionState`, `ScenesProjectionReason`, scene edit command variants, and state-change publication for scene edits.

- [ ] **Step 1: Define scene projection event types**

In `scenes/events.rs`, extend the enum without removing recall-only variants:

```rust
use crate::scenes::SceneConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScenesProjectionReason {
    SceneState,
    FileReplacement,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScenesProjectionState {
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_internal_id: Option<String>,
    pub selected_scene_internal_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScenesEvent {
    StateChanged {
        reason: ScenesProjectionReason,
        state: ScenesProjectionState,
        persisted_scene_edit: bool,
    },
    Skipped { scene_label: String, reason: String },
    Blocked { scene_label: String, reason: String },
    Ready { scene_label: String, target_count: usize },
    StartRequested { scene_label: String },
}
```

- [ ] **Step 2: Add projection state method to `ScenesState`**

In `scenes/state.rs`:

```rust
pub fn projection_state(&self) -> crate::scenes::ScenesProjectionState {
    crate::scenes::ScenesProjectionState {
        scene_configs: self.scene_configs.clone(),
        cued_scene_internal_id: self.cued_scene_internal_id.map(|id| id.to_string()),
        selected_scene_internal_id: self.selected_scene_internal_id.clone(),
    }
}
```

- [ ] **Step 3: Add command variants and result types**

In `scenes/commands.rs`, add scene command variants using explicit `oneshot` replies:

```rust
pub enum ScenesCommand {
    GetSceneDocument { reply: oneshot::Sender<SceneDocument> },
    GetSceneConfig { internal_scene_id: Uuid, reply: oneshot::Sender<Option<SceneConfig>> },
    InitialProjectionState { reply: oneshot::Sender<ScenesProjectionState> },
    SetSceneDuration { internal_scene_id: Uuid, duration_ms: u64, reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>> },
    SetSceneScopeFadersEnabled { internal_scene_id: Uuid, enabled: bool, reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>> },
    SetSceneScopePanEnabled { internal_scene_id: Uuid, enabled: bool, reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>> },
    LinkSceneConfig { source_internal_scene_id: Uuid, target_scene_index: i32, overwrite_existing: bool, reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>> },
    DeleteSceneConfig { internal_scene_id: Uuid, reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>> },
    SetChannelScoped { internal_scene_id: Uuid, group: i32, channel: i32, scoped: bool, reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>> },
    SetAllChannelsScoped { internal_scene_id: Uuid, scoped: bool, reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>> },
    CueScene { internal_scene_id: Uuid, reply: Option<oneshot::Sender<Result<CueSceneResult, String>>> },
    SelectSceneConfig { internal_scene_id: Uuid, reply: Option<oneshot::Sender<Result<SelectedSceneResult, String>>> },
    StoreSceneConfigFromCurrentLv1 { internal_scene_id: Uuid, reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>> },
    ReplaceSceneDocument { document: SceneDocument, selected_scene_internal_id: Option<String>, reason: ScenesProjectionReason, persisted_scene_edit: bool, reply: Option<oneshot::Sender<ScenesCommandResult>> },
    RecallScene { internal_scene_id: Uuid, reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>> },
    Shutdown,
}
```

Define result types in `scenes::commands`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScenesCommandResult { pub changed: bool }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueSceneResult { pub changed: bool, pub scene: SceneConfig }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedSceneResult { pub scene: SceneConfig }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallSceneResult { pub scene: SceneConfig, pub lv1_scene_index: i32 }
```

- [ ] **Step 4: Add actor test for scene edit publication**

In `scenes/actor.rs` tests, add an actor test that uses the mailbox and event bus. Use whatever fake LV1 handle pattern already exists in this file; do not mutate private actor state directly.

```rust
#[tokio::test]
async fn set_scene_duration_publishes_persisted_scene_state_change() {
    let event_bus = AppEventBus::default();
    let (handle, task, peers) = build_scenes_actor(1, RuntimeGeneration::new(), event_bus.clone());
    // Install fake LV1/fade peers using existing test helpers in this module.
    task.spawn();

    let id = uuid::Uuid::from_u128(0x11111111111141118111111111111111);
    replace_scene_document_for_test(&handle, scene_document_with_one_scene(id)).await;

    let mut events = event_bus.subscribe();
    let (reply, rx) = oneshot::channel();
    handle.send(ScenesCommand::SetSceneDuration {
        internal_scene_id: id,
        duration_ms: 2_000,
        reply: Some(reply),
    }).await.unwrap();

    assert_eq!(rx.await.unwrap().unwrap(), ScenesCommandResult { changed: true });
    let event = recv_scenes_state_event(&mut events).await;
    assert!(event.persisted_scene_edit);
    assert_eq!(event.state.scene_configs[0].duration_ms, 2_000);
}
```

- [ ] **Step 5: Implement command handling minimally**

In `scenes/actor.rs`, add a helper to publish state changes:

```rust
fn publish_scene_state_changed(
    event_bus: &AppEventBus,
    generation: u64,
    reason: ScenesProjectionReason,
    state: &ScenesState,
    persisted_scene_edit: bool,
) {
    event_bus.publish_scenes(
        generation,
        ScenesEvent::StateChanged {
            reason,
            state: state.projection_state(),
            persisted_scene_edit,
        },
    );
}
```

For each scene edit command, call the moved `ScenesState` method, publish only when `changed`, and reply with the relevant result. Keep error strings unchanged.

- [ ] **Step 6: Run scenes actor tests**

Run: `cargo nextest run -p advanced-show-control scenes`

Expected: PASS for scenes tests, including the new publication actor test.

- [ ] **Step 7: Commit**

```bash
git status --short
git diff -- src-tauri/src/scenes src-tauri/src/runtime/events.rs
git add src-tauri/src/scenes src-tauri/src/runtime/events.rs
git commit -m "feat: add scenes-owned scene commands"
```

---

### Task 4: Move Recall Validation Fully Into `scenes`

**Files:**
- Modify: `src-tauri/src/scenes/actor.rs`
- Modify: `src-tauri/src/scenes/policy.rs` only if needed for type imports
- Modify: `src-tauri/src/scenes/commands.rs`
- Modify: `src-tauri/src/scenes/mod.rs`
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/show/mod.rs`

**Interfaces:**
- Consumes: `RecallSceneResult`, `SceneDocument`, and scene state commands from Task 3.
- Produces: recall validation functions owned by `scenes`; no `crate::show::validate_recall_scene_request` dependency remains.

- [ ] **Step 1: Move recall validation tests**

Move tests currently under `show::commands::validate_recall_scene_request_*` into `scenes::commands` or `scenes::actor` as pure unit tests. Use `SceneDocument` without lockout, and pass lockout separately:

```rust
fn validate_recall_scene_request(
    lockout: bool,
    scene_document: &SceneDocument,
    lv1: &Lv1StateSnapshot,
    internal_scene_id: Uuid,
) -> Result<RecallSceneResult, String>
```

- [ ] **Step 2: Run moved tests before implementation**

Run: `cargo nextest run -p advanced-show-control validate_recall_scene_request`

Expected: FAIL with missing function in `scenes` or stale imports.

- [ ] **Step 3: Implement recall validation in `scenes`**

Add the function to `scenes/commands.rs` or a small `scenes/validation.rs` if the file grows too large. Keep exact safety error strings:

```rust
pub fn validate_recall_scene_request(
    lockout: bool,
    scenes: &SceneDocument,
    lv1: &Lv1StateSnapshot,
    internal_scene_id: Uuid,
) -> Result<RecallSceneResult, String> {
    if lockout {
        return Err("Recall blocked: lockout is enabled".to_string());
    }
    // Preserve existing unlinked, disconnected, and identity mismatch checks.
}
```

- [ ] **Step 4: Remove show peer from scenes recall actor flow**

In `ScenesPeers`, remove `show: ShowStateHandle`. Update `set_peers` signature to:

```rust
pub fn set_peers(&self, lv1: Lv1ActorHandle, fade: FadeEngineHandle) {
    *self.peers.lock().expect("scene recall peer lock poisoned") =
        Some(ScenesPeerHandles { lv1, fade });
}
```

For explicit recall, use `ScenesState::snapshot()` directly and fetch lockout via a show-owned path only if still needed. If lockout is not yet available inside `scenes`, keep a temporary `ShowStateHandle` dependency only for `GetShowMetadata` and remove it in Task 5. Do not fetch scene configs from `show`.

- [ ] **Step 5: Delete show-owned validation export**

Remove `validate_recall_scene_request` and `RecallSceneResult` from `show/commands.rs` and `show/mod.rs` after all imports use `crate::scenes`.

- [ ] **Step 6: Run recall safety tests**

Run: `cargo nextest run -p advanced-show-control scene_recall`

Expected: PASS. Confirm existing manual override, blocked/skipped, overlap, and generation behavior tests still pass.

- [ ] **Step 7: Commit**

```bash
git status --short
git diff -- src-tauri/src/scenes src-tauri/src/show
git add src-tauri/src/scenes src-tauri/src/show
git commit -m "refactor: own recall validation in scenes"
```

---

### Task 5: Coordinate Persistence Between `show` And `scenes`

**Files:**
- Modify: `src-tauri/src/show/events.rs`
- Modify: `src-tauri/src/show/state.rs`
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/show/actor.rs`
- Modify: `src-tauri/src/show/show_file.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/scenes/commands.rs` if persistence-specific scene replacement replies need details

**Interfaces:**
- Consumes: `ScenesCommand::GetSceneDocument`, `ScenesCommand::ReplaceSceneDocument`, and `ScenesEvent::StateChanged` from Task 3.
- Produces: show actor persistence orchestration without owning scene configs; dirty state changes caused by persisted scene edits.

- [ ] **Step 1: Shrink show projection state**

In `show/events.rs`, remove scene fields:

```rust
pub struct ShowProjectionState {
    pub lockout: bool,
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

- [ ] **Step 2: Remove scene fields from `ShowState`**

In `show/state.rs`, remove `scene_configs`, `cued_scene_internal_id`, and `selected_scene_internal_id`. Replace `snapshot()`/`export_show_file()` with show-owned metadata methods:

```rust
pub(crate) fn export_show_file(&self, scenes: SceneDocument, saved_at: String) -> ShowFile {
    export_show_file(scenes, self.lockout, saved_at)
}

pub(crate) fn mark_dirty_if_clean(&mut self) -> bool {
    if self.show_file_dirty { false } else { self.show_file_dirty = true; true }
}
```

- [ ] **Step 3: Add show actor test for dirty on scene edit**

Use an actor test through `AppEventBus`, not direct private state mutation:

```rust
#[tokio::test]
async fn show_marks_dirty_when_persisted_scene_edit_event_arrives() {
    let event_bus = AppEventBus::default();
    let show = ShowStateHandle::new_empty(event_bus.clone());
    let mut events = event_bus.subscribe();

    event_bus.publish_scenes(1, ScenesEvent::StateChanged {
        reason: ScenesProjectionReason::SceneState,
        state: ScenesProjectionState {
            scene_configs: Vec::new(),
            cued_scene_internal_id: None,
            selected_scene_internal_id: None,
        },
        persisted_scene_edit: true,
    });

    let state = recv_show_state_event(&mut events).await;
    assert!(state.show_file_dirty);
}
```

Adapt construction to the existing show actor test helpers.

- [ ] **Step 4: Add show actor test for file replacement not dirtying automatically**

Publish a scenes event with `persisted_scene_edit: false` and assert no dirty change is emitted unless the open/import command explicitly marks dirty.

- [ ] **Step 5: Implement event subscription in show actor**

In `show/actor.rs`, ensure the show task listens to `AppEvent::Scenes { event: ScenesEvent::StateChanged { persisted_scene_edit: true, .. }, .. }` and calls `mark_dirty_if_clean()`, then publishes show metadata if changed.

- [ ] **Step 6: Update new/save/open commands**

Add helper functions in `show/actor.rs` to query/replace scenes explicitly:

```rust
async fn current_scene_document(scenes: &ScenesHandle) -> Result<SceneDocument, String> {
    let (reply, rx) = oneshot::channel();
    scenes.send(ScenesCommand::GetSceneDocument { reply })
        .await
        .map_err(|_| "Scene state is unavailable".to_string())?;
    rx.await.map_err(|_| "Scene state reply channel closed".to_string())
}
```

Update:

- `NewShowFileFromCurrentLv1`: ask scenes to reset/align using current LV1, then reset show file metadata and mark clean.
- `SaveShowFileAs`: query `SceneDocument`, export with show lockout, write file, mark saved.
- `LoadShowFileFromPath`: read/import DTO, align via scenes, replace scene state with `persisted_scene_edit: false`, then set show file metadata and dirty flag based on import report.

- [ ] **Step 7: Remove scene commands from `ShowCommand`**

Delete scene edit/read variants from `show/commands.rs`: `GetShowDocument`, `GetSceneConfig`, `SetSceneDuration`, `SetSceneScope*`, `LinkSceneConfig`, `DeleteSceneConfig`, `SetChannelScoped`, `SetAllChannelsScoped`, `CueScene`, `SelectSceneConfig`, `StoreSceneConfigFromCurrentLv1`, and scene snapshot test commands. Keep show-owned commands.

- [ ] **Step 8: Run show tests**

Run: `cargo nextest run -p advanced-show-control show`

Expected: PASS for show persistence, file, connection metadata, and dirty-state actor tests.

- [ ] **Step 9: Commit**

```bash
git status --short
git diff -- src-tauri/src/show src-tauri/src/scenes src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/show src-tauri/src/scenes src-tauri/src/lifecycle/mod.rs
git commit -m "refactor: coordinate scene persistence from show"
```

---

### Task 6: Split Projector Seeding And Event Application

**Files:**
- Modify: `src-tauri/src/projector/cache.rs`
- Modify: `src-tauri/src/projector/runtime.rs`
- Modify: `src-tauri/src/lifecycle/mod.rs`
- Modify: `src-tauri/src/projector/view.rs` imports only if needed

**Interfaces:**
- Consumes: `ShowProjectionState` without scene fields and `ScenesProjectionState` from previous tasks.
- Produces: unchanged external `AppViewState` containing scene fields assembled from scenes projection state.

- [ ] **Step 1: Update projector input struct**

In `projector/runtime.rs`:

```rust
pub struct ProjectorInputs<R: Runtime> {
    pub app: AppHandle<R>,
    pub generation: u64,
    pub initial_show_state: ShowProjectionState,
    pub initial_scenes_state: ScenesProjectionState,
    pub initial_settings: AppSettings,
    pub events: broadcast::Receiver<AppEvent>,
    pub logs: broadcast::Receiver<UiLogEvent>,
}
```

- [ ] **Step 2: Update cache apply methods**

In `projector/cache.rs`, keep scene fields in `ProjectionCache`, but split methods:

```rust
pub fn apply_show_state(&mut self, state: ShowProjectionState) {
    self.lockout = state.lockout;
    self.show_file_path = state.show_file_path;
    self.show_file_dirty = state.show_file_dirty;
    self.show_file_last_saved_at = state.show_file_last_saved_at;
    self.discovered_lv1_systems = state.discovered_lv1_systems;
    self.connected_lv1_identity = state.connected_lv1_identity;
    self.pending_lv1_identity = state.pending_lv1_identity;
    self.reconnect_state = state.reconnect;
    self.last_event_at = state.last_event_at;
}

pub fn apply_scenes_state(&mut self, state: ScenesProjectionState) {
    self.scene_configs = state.scene_configs;
    self.cued_scene_internal_id = state.cued_scene_internal_id;
    self.selected_scene_internal_id = state.selected_scene_internal_id;
}
```

- [ ] **Step 3: Add/update projector test for scenes event**

Add a test that publishes `AppEvent::Scenes { event: ScenesEvent::StateChanged { ... } }` and asserts the emitted `AppViewState` includes the scene config. This is a projector test, not a scenes actor test.

- [ ] **Step 4: Apply scenes events in projector runtime**

Update `apply_projector_event`:

```rust
AppEvent::Scenes { generation, event: ScenesEvent::StateChanged { state, .. } } => {
    if *generation != cache.active_generation() { return false; }
    cache.apply_scenes_state(state.clone());
    true
}
AppEvent::Scenes { .. } => false,
```

If `active_generation` is private, add a small `is_active_generation(&self, generation: u64) -> bool` method rather than exposing mutable state.

- [ ] **Step 5: Update lifecycle projector seeding**

In `lifecycle/mod.rs`, request both initial projections before spawning projector:

```rust
let initial_show_state = request_show_initial_projection(&self.show).await?;
let initial_scenes_state = request_scenes_initial_projection(&scene_recall).await?;
```

Use explicit mailbox commands and `oneshot` replies.

- [ ] **Step 6: Run projector tests**

Run: `cargo nextest run -p advanced-show-control projector`

Expected: PASS and emitted frontend snapshots retain existing scene fields.

- [ ] **Step 7: Commit**

```bash
git status --short
git diff -- src-tauri/src/projector src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/projector src-tauri/src/lifecycle/mod.rs
git commit -m "refactor: project scenes state separately"
```

---

### Task 7: Route Tauri Scene Commands To `ScenesHandle`

**Files:**
- Modify: `src-tauri/src/ui/commands/show.rs`
- Modify: `src-tauri/src/ui/commands/scenes.rs`
- Modify: `src-tauri/src/ui/debug/commands.rs` only if imports depend on moved result types
- Modify: `src-tauri/src/ui/menu.rs` only if new/open/save result imports changed
- Modify: `src-tauri/src/lifecycle/mod.rs` if a clearer accessor name is needed, without adding domain helpers to handles

**Interfaces:**
- Consumes: scene command variants from `ScenesCommand`.
- Produces: unchanged Tauri command function names and frontend API surface.

- [ ] **Step 1: Update imports for result types**

In `ui/commands/show.rs`, import scene results from `crate::scenes` and show results from `crate::show`:

```rust
use crate::scenes::{CueSceneResult, ScenesCommand, ScenesCommandResult, SelectedSceneResult};
use crate::show::{LoadShowFileResult, NewShowFileResult, ShowCommand, ShowCommandResult};
```

- [ ] **Step 2: Route scene edit commands to scenes handle**

For `set_scene_duration_ms`, replace show handle usage with scenes handle usage while keeping function name unchanged:

```rust
let scenes = lifecycle
    .current_scene_recall_fader()
    .await
    .ok_or(AppCommandError::Lv1Unavailable)
    .map_err(map_app_command_error)?;
let (reply, rx) = oneshot::channel();
scenes.send(ScenesCommand::SetSceneDuration {
    internal_scene_id,
    duration_ms,
    reply: Some(reply),
}).await
    .map_err(|_| AppCommandError::Lv1Unavailable)
    .map_err(map_app_command_error)?;
rx.await
    .map_err(|_| AppCommandError::ReplyChannelClosed)
    .map_err(map_app_command_error)?
```

Repeat the same adapter pattern for link/delete/select/cue/store/channel scope/scope toggles.

- [ ] **Step 3: Update `recall_scene` result import**

In `ui/commands/scenes.rs`, import `RecallSceneResult` from `crate::scenes`.

- [ ] **Step 4: Run backend command tests/check**

Run: `cargo nextest run -p advanced-show-control commands::tests ui::commands`

Expected: PASS if matching tests exist. If the filter matches no tests, run `cargo nextest run -p advanced-show-control ui`.

- [ ] **Step 5: Run UI typecheck only if TypeScript-facing result shapes changed**

Run: `make ui-typecheck`

Expected: PASS. If no TypeScript types changed and the backend still serializes the same command payloads, this can be deferred to final verification.

- [ ] **Step 6: Commit**

```bash
git status --short
git diff -- src-tauri/src/ui src-tauri/src/lifecycle/mod.rs
git add src-tauri/src/ui src-tauri/src/lifecycle/mod.rs
git commit -m "refactor: route scene commands to scenes actor"
```

---

### Task 8: Remove Obsolete Show Scene Ownership And Update Docs

**Files:**
- Modify: `src-tauri/src/show/state.rs`
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/show/mod.rs`
- Modify: `src-tauri/src/show/events.rs`
- Modify: `src-tauri/src/scenes/mod.rs`
- Modify: `docs/architecture.md`
- Modify: `docs/superpowers/specs/2026-06-25-scenes-state-refactor-design.md` only if implementation intentionally differs from spec

**Interfaces:**
- Consumes: completed scene ownership from previous tasks.
- Produces: no remaining scene state ownership in `show`, no obsolete show scene exports, updated architecture docs.

- [ ] **Step 1: Search for stale show scene imports**

Run: `rg "crate::show::\{.*Scene|show::Scene|ShowDocument|GetShowDocument|GetSceneConfig|ShowProjectionReason::ShowState|ScenesPeers.*show|ShowStateHandle" src-tauri/src`

Expected: only legitimate `ShowStateHandle` usages in show/lifecycle remain; no scene-domain imports from `show` remain.

- [ ] **Step 2: Remove stale exports and modules**

In `show/mod.rs`, remove scene-domain re-exports. The module must no longer export `SceneConfig`, `ChannelConfig`, `ChannelRef`, `SceneScopeToggles`, `ShowDocument`, `RecallSceneResult`, `CueSceneResult`, or `SelectedSceneResult`.

- [ ] **Step 3: Update architecture ownership table**

In `docs/architecture.md`, update component responsibilities:

```markdown
| `scenes` | Owns application-managed scene configuration state, scene selection/cueing, scene recall automation, and recall policy enforcement. |
| `show` | Maintains session file metadata, show-file input/output orchestration, discovery state, lockout state, and connection metadata. |
```

Update peer relationships so `scenes` no longer lists `ShowStateHandle` for scene config lookup. If `show` now needs `ScenesHandle` for persistence orchestration, document that peer relationship explicitly.

- [ ] **Step 4: Run focused compile/lint checks**

Run: `cargo fmt --all -- --check`

Expected: PASS.

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS. Fix unused imports, obsolete exports, and dead branches surfaced by the refactor.

- [ ] **Step 5: Commit**

```bash
git status --short
git diff -- src-tauri/src docs/architecture.md docs/superpowers/specs/2026-06-25-scenes-state-refactor-design.md
git add src-tauri/src docs/architecture.md docs/superpowers/specs/2026-06-25-scenes-state-refactor-design.md
git commit -m "refactor: remove show scene ownership"
```

---

### Task 9: Final Verification

**Files:**
- No intended source edits unless verification finds failures.

**Interfaces:**
- Consumes: all previous tasks.
- Produces: verified branch ready for review or merge decision.

- [ ] **Step 1: Run targeted Rust tests**

Run: `cargo nextest run -p advanced-show-control scenes show projector scene_recall`

Expected: PASS. Read the output and record any failures before fixing.

- [ ] **Step 2: Run required Rust verification**

Run: `make rust-fmt`

Expected: PASS.

Run: `make rust-lint`

Expected: PASS.

Run: `make rust-test`

Expected: PASS.

- [ ] **Step 3: Run frontend typecheck if any command payload/result type moved across the Tauri boundary**

Run: `make ui-typecheck`

Expected: PASS.

- [ ] **Step 4: Inspect final diff and status**

Run: `git status --short`

Expected: no unstaged unrelated changes; only intended files if verification fixes were made.

Run: `git diff --stat HEAD`

Expected: reviewable summary of only intended changes since the last commit.

- [ ] **Step 5: Commit verification fixes if needed**

If verification required fixes:

```bash
git add <fixed-files>
git commit -m "fix: complete scenes state refactor"
```

If no fixes were needed, do not create an empty commit.
