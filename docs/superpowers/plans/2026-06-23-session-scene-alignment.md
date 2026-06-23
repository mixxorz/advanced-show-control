# Session Scene Alignment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace derived scene IDs with internal UUIDs, add nullable linked/unlinked scene configs, and align saved app scene configs to LV1 scene-list changes without dropping saved fade work.

**Architecture:** Make the change easy first: introduce internal scene UUID identity and nullable LV1 locators across backend, persistence, projection, and frontend before changing alignment behavior. Then make the easy change: move scene-list matching into a pure aligner, call it directly from the show actor, add unlinked/link/delete behavior, and update UI affordances.

**Tech Stack:** Rust 2024, Tauri 2, Tokio actor mailboxes, Serde, `uuid` crate, React 19, TypeScript, Vitest, Storybook tests, Cargo nextest.

## Global Constraints

- Delete `scene_id` from the Rust model, TypeScript model, projection, commands, components, tests, and show-file import/export. Do not leave it as unused compatibility state.
- Every scene config has `internal_scene_id: Uuid` / `internalSceneId: string`; this is app tracking only and must not be used for LV1 matching.
- `scene_index` is nullable: Rust `Option<i32>`, frontend `number | null`.
- Unlinked configs have `scene_index = None`, remain editable, can be linked or deleted, can be saved/reopened, and cannot be stored, cued, recalled, or used by recall automation.
- Run the same pure aligner every time the active LV1 scene list changes.
- Keep classifier shape: `Noop`, `Rename`, `Move`, `Insert`, `Delete`, `Ambiguous`.
- Keep deterministic FIFO behavior for classified `Move`, `Insert`, and `Delete`; preserve deleted configs as unlinked; do not FIFO-guess for `Ambiguous`.
- Log one human-readable `INFO` trace only when alignment changes configs; no report object, projection, or UI state.
- Use `Link to LV1 Scene` language, not merge/remap.
- Link overwrite behavior is replace-only: if overwrite is true, delete the existing target config and link the source config. No field-level merge.
- Preserve existing lockout, exact scene identity, generation, stale-state, manual override, abort, overlap, and disconnect safety behavior.
- Commit after each task. Do not stage unrelated files.

---

## File Structure

- Modify `src-tauri/src/show/types.rs`: add `internal_scene_id`, make `scene_index` nullable, remove `scene_id` and parsing helpers that only exist for `scene_id`.
- Modify `src-tauri/src/show/show_file.rs`: persist `internalSceneId`, import legacy files with generated UUIDs, serialize `sceneIndex: null`, stop serializing `sceneId`.
- Modify `src-tauri/src/show/state.rs`: selection/cue state moves to UUIDs; remove embedded reconciliation algorithm; keep state mutation helpers only.
- Modify `src-tauri/src/show/capture.rs`: store/duration/scope mutations target internal UUID and reject unlinked store.
- Modify `src-tauri/src/show/commands.rs`: command structs/results target UUIDs and recall validation rejects unlinked configs.
- Modify `src-tauri/src/show/actor.rs`: call pure aligner directly on active `SceneListChanged`; mark dirty on changed alignment; log INFO diagnostic only when changed; handle link/delete commands.
- Create `src-tauri/src/show/scene_alignment.rs`: pure classifier, aligner, diagnostic string, and alignment tests.
- Modify `src-tauri/src/scenes/actor.rs` and `src-tauri/src/ui/commands/scenes.rs`: recall paths target internal UUID and continue exact LV1 identity validation.
- Modify `src-tauri/src/ui/commands/show.rs`, `src-tauri/src/ui/commands.rs`, `src-tauri/src/ui/mod.rs`, and `src-tauri/src/ui/debug.rs`: command adapter argument names and new link/delete command registration.
- Modify `src-tauri/src/projector/view.rs` only if exported type paths require updates.
- Modify `ui/src/types.ts`: replace `sceneId` with `internalSceneId`, make `sceneIndex` nullable.
- Modify `ui/src/AppRuntime.tsx`, `ui/src/appContext.tsx`, `ui/src/commands.ts`, and `ui/src/App.tsx`: command signatures and service wiring use internal UUIDs; add link/delete services.
- Modify `ui/src/components/SceneList.tsx`, `SceneListRow.tsx`, `SceneEditor.tsx`, `SelectedSceneHeader.tsx`, `SelectedSceneActions.tsx`, `DurationInput.tsx`, scope components, and stories/tests: unlinked visuals, disabled Store/Cue/Recall, link/delete section, duplicate-name warning.
- Modify fixtures under `ui/src/storybook/mockAppState.ts` and affected tests to include `internalSceneId` and nullable `sceneIndex`.
- Delete `research.md`: temporary research artifact is not part of implementation docs.

---

## Phase 1: Make The Change Easy

### Task 1: Introduce Internal Scene UUID And Nullable Locator In Rust Types

**Files:**
- Modify: `src-tauri/src/show/types.rs`
- Modify: `src-tauri/src/show/state.rs`
- Modify: `src-tauri/src/show/capture.rs`
- Modify: `src-tauri/src/show/commands.rs`
- Modify tests in the same files

**Interfaces:**
- Produces: `SceneConfig { internal_scene_id: uuid::Uuid, scene_index: Option<i32>, scene_name: String, ... }`
- Produces: `ShowDocument { cued_scene_id: Option<Uuid> }` or renamed `cued_scene_internal_id: Option<Uuid>` if that reads clearer in implementation.
- Removes: `scene_id`, `scene_id(index, name)`, and `parse_scene_id` from production use.

- [ ] **Step 1: Write failing Rust serialization tests**

In `src-tauri/src/show/types.rs`, replace the scene serialization expectations with tests asserting `internalSceneId` is present, `sceneId` is absent, and `sceneIndex` can be `null`:

```rust
#[test]
fn scene_config_serializes_internal_id_and_nullable_scene_index() {
    let internal_scene_id = uuid::Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let config = SceneConfig {
        internal_scene_id,
        scene_index: Some(0),
        scene_name: "Intro".to_string(),
        duration_ms: 1000,
        channel_configs: Vec::new(),
        scoped_channels: Vec::new(),
        scope_toggles: SceneScopeToggles::default(),
    };

    let json = serde_json::to_value(config).unwrap();

    assert_eq!(json["internalSceneId"], internal_scene_id.to_string());
    assert_eq!(json["sceneIndex"], 0);
    assert!(json.get("sceneId").is_none());
}

#[test]
fn scene_config_serializes_unlinked_scene_index_as_null() {
    let config = SceneConfig {
        internal_scene_id: uuid::Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
        scene_index: None,
        scene_name: "Deleted Verse".to_string(),
        duration_ms: 1000,
        channel_configs: Vec::new(),
        scoped_channels: Vec::new(),
        scope_toggles: SceneScopeToggles::default(),
    };

    let json = serde_json::to_value(config).unwrap();

    assert_eq!(json["sceneIndex"], serde_json::Value::Null);
    assert!(json.get("sceneId").is_none());
}
```

- [ ] **Step 2: Run focused Rust type tests and verify failure**

Run: `cargo nextest run -p advanced-show-control show::types`

Expected: FAIL because `SceneConfig` still has `scene_id` and non-null `scene_index`.

- [ ] **Step 3: Update Rust scene types minimally**

In `src-tauri/src/show/types.rs`, change `SceneConfig` to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneConfig {
    pub internal_scene_id: uuid::Uuid,
    pub scene_index: Option<i32>,
    pub scene_name: String,
    pub duration_ms: u64,
    pub channel_configs: Vec<ChannelConfig>,
    pub scoped_channels: Vec<ChannelRef>,
    pub scope_toggles: SceneScopeToggles,
}
```

Update `ShowDocument` cue field to target the internal UUID. Prefer this explicit name if it does not cause excessive churn:

```rust
pub struct ShowDocument {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_internal_id: Option<uuid::Uuid>,
}
```

Delete `scene_id` and `parse_scene_id` helpers from production types after all call sites are migrated in this task. If tests still need local fixture helpers, define them inside the test module only.

- [ ] **Step 4: Fix backend compile errors by migrating local helpers to UUIDs**

Update tests and helper constructors in `src-tauri/src/show/state.rs`, `capture.rs`, and `commands.rs` to provide deterministic UUIDs. Use a small test helper pattern:

```rust
fn test_uuid(n: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(n)
}
```

For linked fixtures use `scene_index: Some(index)`. For unlinked fixtures use `scene_index: None`.

- [ ] **Step 5: Run focused backend tests**

Run: `cargo nextest run -p advanced-show-control show`

Expected: PASS after local compile fixes.

- [ ] **Step 6: Commit**

```bash
git status --short
git add src-tauri/src/show/types.rs src-tauri/src/show/state.rs src-tauri/src/show/capture.rs src-tauri/src/show/commands.rs
git commit -m "refactor: add internal scene identifiers"
```

### Task 2: Migrate Show File Persistence To Internal UUIDs

**Files:**
- Modify: `src-tauri/src/show/show_file.rs`
- Modify: `src-tauri/src/show_file.rs` tests if re-export tests need updates

**Interfaces:**
- Consumes: `SceneConfig.internal_scene_id`, `SceneConfig.scene_index: Option<i32>`.
- Produces: `ShowFileSceneConfig.internal_scene_id: Option<Uuid>` for import compatibility, serialized as `internalSceneId` when present.
- Produces: imported legacy scene configs with generated UUIDs and dirty/report state indicating generated IDs.

- [ ] **Step 1: Write failing persistence tests**

Add tests in `src-tauri/src/show/show_file.rs`:

```rust
#[test]
fn export_show_file_writes_internal_scene_id_and_nullable_index() {
    let id = uuid::Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap();
    let snapshot = ShowDocument {
        lockout: false,
        cued_scene_internal_id: Some(id),
        scene_configs: vec![SceneConfig {
            internal_scene_id: id,
            scene_index: None,
            scene_name: "Old Verse".to_string(),
            duration_ms: 5_000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }],
    };

    let file = export_show_file(snapshot, "saved".to_string());
    let json = serde_json::to_value(&file).unwrap();

    assert_eq!(json["sceneConfigs"][0]["internalSceneId"], id.to_string());
    assert_eq!(json["sceneConfigs"][0]["sceneIndex"], serde_json::Value::Null);
    assert_eq!(json["cuedSceneInternalId"], id.to_string());
    assert!(json["sceneConfigs"][0].get("sceneId").is_none());
}

#[test]
fn import_legacy_show_file_generates_internal_scene_ids() {
    let mut file = ShowFile {
        schema_version: SHOW_FILE_SCHEMA_VERSION,
        app_version: "0.1.0".to_string(),
        saved_at: "123".to_string(),
        safety: ShowFileSafety { lockout: false },
        cued_scene_internal_id: None,
        scene_configs: vec![ShowFileSceneConfig {
            internal_scene_id: None,
            scene_index: Some(1),
            scene_name: "Intro".to_string(),
            duration_ms: 1_000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: ShowFileSceneScopeToggles::default(),
        }],
    };
    let lv1 = Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry { index: 1, name: "Intro".to_string() }],
        channels: Vec::new(),
    };

    let imported = import_show_file(&mut file, &lv1).unwrap();

    assert_ne!(imported.snapshot.scene_configs[0].internal_scene_id, uuid::Uuid::nil());
    assert!(imported.generated_internal_scene_ids);
}
```

- [ ] **Step 2: Run focused persistence tests and verify failure**

Run: `cargo nextest run -p advanced-show-control show::show_file`

Expected: FAIL because show file DTOs still lack `internalSceneId` and nullable `sceneIndex`.

- [ ] **Step 3: Update show file DTOs and import/export**

In `ShowFile`, replace `cued_scene_id` with:

```rust
#[serde(default)]
pub cued_scene_internal_id: Option<uuid::Uuid>,
```

In `ShowFileSceneConfig`, add:

```rust
#[serde(default)]
pub internal_scene_id: Option<uuid::Uuid>,
pub scene_index: Option<i32>,
```

Update `show_scene_to_file_scene` to write `Some(config.internal_scene_id)` and nullable `scene_index`.

Update `file_scene_to_show_scene` to accept or generate a UUID:

```rust
let internal_scene_id = config.internal_scene_id.unwrap_or_else(uuid::Uuid::new_v4);
```

Extend `ImportedShowFile` with:

```rust
pub generated_internal_scene_ids: bool,
```

Set it when any file scene lacks `internal_scene_id`.

- [ ] **Step 4: Remove exact-match pruning as destructive import behavior**

Remove or stop using `prune_show_file_to_lv1_scenes` for normal import. The alignment task will handle matching/preservation. Keep schema-version and non-empty LV1 validation only where still needed by connected load.

If public re-export compatibility causes broad churn, keep a private validation helper named for what it does, not pruning. Do not retain a function that removes missing scenes.

- [ ] **Step 5: Run focused persistence tests**

Run: `cargo nextest run -p advanced-show-control show::show_file`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git status --short
git add src-tauri/src/show/show_file.rs src-tauri/src/show_file.rs
git commit -m "refactor: persist internal scene identifiers"
```

### Task 3: Migrate Backend Commands To Internal UUIDs

**Files:**
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/show/actor.rs`
- Modify: `src-tauri/src/show/capture.rs`
- Modify: `src-tauri/src/ui/commands/show.rs`
- Modify: `src-tauri/src/ui/commands/scenes.rs`
- Modify: `src-tauri/src/scenes/actor.rs`

**Interfaces:**
- Consumes: `internal_scene_id: Uuid` on `SceneConfig`.
- Produces: command variants using `internal_scene_id: Uuid` instead of `scene_id: String`.
- Produces: unlinked rejection from Store/Cue/Recall once unlinked configs exist.

- [ ] **Step 1: Write failing command tests for UUID targeting**

In show/scenes command tests, add targeted cases:

```rust
#[test]
fn validate_recall_scene_request_uses_internal_scene_id() {
    let id = uuid::Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
    let show = ShowDocument {
        lockout: false,
        cued_scene_internal_id: None,
        scene_configs: vec![SceneConfig {
            internal_scene_id: id,
            scene_index: Some(1),
            scene_name: "Intro".to_string(),
            duration_ms: 1000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }],
    };
    let lv1 = Lv1StateSnapshot {
        connection: ConnectionStatus::Connected,
        scene: None,
        scene_list: vec![SceneListEntry { index: 1, name: "Intro".to_string() }],
        channels: Vec::new(),
    };

    let result = validate_recall_scene_request(&show, &lv1, id).unwrap();

    assert_eq!(result.lv1_scene_index, 1);
}
```

Add store rejection test in `capture.rs`:

```rust
#[test]
fn store_scene_config_rejects_unlinked_scene() {
    let id = uuid::Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
    let mut state = ShowState::default();
    state.replace_snapshot(ShowDocument {
        lockout: false,
        cued_scene_internal_id: None,
        scene_configs: vec![SceneConfig {
            internal_scene_id: id,
            scene_index: None,
            scene_name: "Old Verse".to_string(),
            duration_ms: 1000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }],
    });

    let err = state.store_scene_config(id, &[channel(0, 1, "Lead", -6.0)]).unwrap_err();

    assert_eq!(err, "Store blocked: scene is unlinked");
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo nextest run -p advanced-show-control show scenes::actor`

Expected: FAIL because command signatures still use `scene_id`.

- [ ] **Step 3: Change command variants and state lookup helpers**

Change show command variants to UUID arguments, for example:

```rust
GetSceneConfig { internal_scene_id: uuid::Uuid, reply: oneshot::Sender<Option<SceneConfig>> }
SetSceneDuration { internal_scene_id: uuid::Uuid, duration_ms: u64, reply: ... }
StoreSceneConfigFromCurrentLv1 { internal_scene_id: uuid::Uuid, reply: ... }
CueScene { internal_scene_id: uuid::Uuid, reply: ... }
SelectSceneConfig { internal_scene_id: uuid::Uuid, reply: ... }
```

Replace state lookup helpers with UUID lookup:

```rust
pub fn get_scene_config(&self, internal_scene_id: uuid::Uuid) -> Option<SceneConfig> {
    self.scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id == internal_scene_id)
        .cloned()
}
```

- [ ] **Step 4: Update capture/store validation**

Change `store_scene_config` to accept `internal_scene_id: Uuid`. It must fetch the existing config, reject missing configs, reject `scene_index: None`, and build a snapshot preserving the existing UUID and linked locator.

Minimal behavior:

```rust
let existing = self
    .get_scene_config(internal_scene_id)
    .ok_or_else(|| "Scene config not found".to_string())?;
let Some(scene_index) = existing.scene_index else {
    return Err("Store blocked: scene is unlinked".to_string());
};
```

- [ ] **Step 5: Update recall validation**

Change `validate_recall_scene_request` signature to:

```rust
pub fn validate_recall_scene_request(
    show: &ShowDocument,
    lv1: &Lv1StateSnapshot,
    internal_scene_id: uuid::Uuid,
) -> Result<RecallSceneResult, String>
```

Reject unlinked configs before LV1 scene-list lookup:

```rust
let Some(scene_index) = scene.scene_index else {
    return Err("Recall blocked: scene is unlinked".to_string());
};
```

- [ ] **Step 6: Update Tauri command adapters**

Change Tauri command arguments from `scene_id: String` to `internal_scene_id: uuid::Uuid` for scene config commands. Serde will parse UUID strings from the frontend.

- [ ] **Step 7: Run focused backend checks**

Run: `cargo nextest run -p advanced-show-control show scenes::actor commands::tests`

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git status --short
git add src-tauri/src/show src-tauri/src/scenes src-tauri/src/ui
git commit -m "refactor: target scene commands by internal id"
```

---

## Phase 2: Make The Easy Change

### Task 4: Extract Pure Scene Aligner

**Files:**
- Create: `src-tauri/src/show/scene_alignment.rs`
- Modify: `src-tauri/src/show/mod.rs`
- Modify: `src-tauri/src/show/state.rs`
- Modify: `src-tauri/src/show/actor.rs`

**Interfaces:**
- Produces: `pub(crate) fn align_scene_configs(configs: Vec<SceneConfig>, lv1_scenes: &[SceneListEntry]) -> Vec<SceneConfig>`.
- Produces: `pub(crate) fn scene_alignment_diagnostic(old: &[SceneConfig], new: &[SceneConfig], lv1_scenes: &[SceneListEntry]) -> String`.
- Removes: `ShowState::reconcile_scene_fade_configs` algorithm and tests.

- [ ] **Step 1: Write aligner tests first**

Create `src-tauri/src/show/scene_alignment.rs` with tests for every spec rule. Start with these test names and expected assertions:

```rust
#[test]
fn exact_index_name_match_preserves_uuid_and_fade_data() { /* assert UUID/duration preserved */ }

#[test]
fn single_same_index_rename_preserves_uuid_and_updates_name() { /* old Some(1) Verse -> new 1 Verse Big */ }

#[test]
fn single_move_preserves_uuid_and_updates_index() { /* Intro moves later by FIFO */ }

#[test]
fn single_insert_creates_one_default_linked_config() { /* inserted scene gets duration 0 and empty targets */ }

#[test]
fn single_delete_preserves_deleted_config_as_unlinked() { /* deleted config scene_index None */ }

#[test]
fn ambiguous_multi_rename_unlinks_old_and_defaults_new() { /* no FIFO guessing */ }

#[test]
fn existing_unlinked_config_remains_unlinked() { /* carried through after linked rows */ }

#[test]
fn duplicate_names_keep_fifo_for_classified_move() { /* current deterministic behavior */ }
```

Use deterministic UUIDs with `Uuid::from_u128`.

- [ ] **Step 2: Run aligner tests and verify failure**

Run: `cargo nextest run -p advanced-show-control show::scene_alignment`

Expected: FAIL because module/function does not exist.

- [ ] **Step 3: Move classifier and implement aligner**

Move these concepts from `show/state.rs` into `show/scene_alignment.rs`:

- `SceneEntry`
- `entries_from_configs` adjusted to skip unlinked configs
- `entries_from_scene_list`
- `SceneListChange`
- `move_candidates`
- name/FIFO helpers
- diagnostic string builder

Implement:

```rust
pub(crate) fn align_scene_configs(
    configs: Vec<SceneConfig>,
    lv1_scenes: &[SceneListEntry],
) -> Vec<SceneConfig> {
    // classify linked configs only
    // apply spec rules
}
```

For default linked configs use:

```rust
SceneConfig {
    internal_scene_id: uuid::Uuid::new_v4(),
    scene_index: Some(entry.index),
    scene_name: entry.name.clone(),
    duration_ms: 0,
    channel_configs: Vec::new(),
    scoped_channels: Vec::new(),
    scope_toggles: SceneScopeToggles::default(),
}
```

- [ ] **Step 4: Wire show actor directly to aligner**

In `show/actor.rs`, replace the old reconciliation call with direct alignment:

```rust
let before = state.scene_configs().to_vec();
let after = crate::show::scene_alignment::align_scene_configs(before.clone(), &scenes);
let changed = state.replace_scene_configs_if_changed(after);
if changed {
    tracing::info!(
        event = "session_scene_alignment",
        "{}",
        crate::show::scene_alignment::scene_alignment_diagnostic(&before, state.scene_configs(), &scenes)
    );
}
publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
```

Add minimal `ShowState` accessors/mutators only if needed:

```rust
pub(crate) fn scene_configs(&self) -> &[SceneConfig] { &self.scene_configs }
pub(crate) fn replace_scene_configs_if_changed(&mut self, next: Vec<SceneConfig>) -> bool { ... }
```

Do not keep `ShowState::reconcile_scene_fade_configs`.

- [ ] **Step 5: Remove old reconciliation tests or move equivalents**

Delete obsolete tests in `show/state.rs` that directly exercise `reconcile_scene_fade_configs`, after equivalent aligner tests pass.

- [ ] **Step 6: Run focused alignment/show tests**

Run: `cargo nextest run -p advanced-show-control show::scene_alignment show::handle show::state`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git status --short
git add src-tauri/src/show/scene_alignment.rs src-tauri/src/show/mod.rs src-tauri/src/show/state.rs src-tauri/src/show/actor.rs
git commit -m "feat: align scene configs from lv1 scene lists"
```

### Task 5: Add Link And Delete Commands

**Files:**
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/show/state.rs`
- Modify: `src-tauri/src/show/actor.rs`
- Modify: `src-tauri/src/ui/commands/show.rs`
- Modify: `src-tauri/src/ui/mod.rs`
- Modify: `src-tauri/src/ui/debug.rs`

**Interfaces:**
- Produces: `ShowCommand::LinkSceneConfig { source_internal_scene_id, target_scene_index, overwrite_existing, reply }`.
- Produces: `ShowCommand::DeleteSceneConfig { internal_scene_id, reply }`.
- Produces Tauri commands `link_scene_config` and `delete_scene_config`.

- [ ] **Step 1: Write failing command tests**

Add tests in `show/state.rs` or `show/actor.rs` for:

```rust
#[test]
fn link_unlinked_config_to_empty_lv1_scene_preserves_fade_data() { /* source None -> target Some(index) */ }

#[test]
fn link_requires_overwrite_when_target_has_existing_config() { /* overwrite false returns exact error */ }

#[test]
fn link_with_overwrite_deletes_existing_target_and_links_source() { /* only source UUID remains for target */ }

#[test]
fn delete_scene_config_removes_config_and_clears_selected_and_cued() { /* selection/cue None */ }
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo nextest run -p advanced-show-control show::state show::handle`

Expected: FAIL because link/delete functions/commands do not exist.

- [ ] **Step 3: Implement state methods**

Add methods on `ShowState`:

```rust
pub(crate) fn link_scene_config(
    &mut self,
    source_internal_scene_id: uuid::Uuid,
    target: &SceneListEntry,
    overwrite_existing: bool,
) -> Result<bool, String>
```

```rust
pub(crate) fn delete_scene_config(
    &mut self,
    internal_scene_id: uuid::Uuid,
) -> Result<bool, String>
```

Implement exactly per spec error strings, including:

```rust
return Err("Link blocked: target scene already has a config".to_string());
```

- [ ] **Step 4: Implement actor command handling**

For link, get fresh current LV1 snapshot through existing `current_lv1_snapshot(peers).await`, find target scene by index, call state method, mark dirty on changed, publish state.

For delete, call state method, mark dirty on changed, publish state.

- [ ] **Step 5: Add Tauri command adapters and registration**

Add in `ui/commands/show.rs`:

```rust
#[tauri::command]
pub async fn link_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    source_internal_scene_id: uuid::Uuid,
    target_scene_index: i32,
    overwrite_existing: bool,
) -> Result<ShowCommandResult, String> { ... }

#[tauri::command]
pub async fn delete_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<ShowCommandResult, String> { ... }
```

Register both in `ui/mod.rs` and `ui/debug.rs` handlers.

- [ ] **Step 6: Run focused command tests**

Run: `cargo nextest run -p advanced-show-control show::state show::handle commands::tests`

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git status --short
git add src-tauri/src/show src-tauri/src/ui
git commit -m "feat: link and delete scene configs"
```

---

## Phase 3: Frontend Migration And UX

### Task 6: Migrate Frontend Types And Command Wiring

**Files:**
- Modify: `ui/src/types.ts`
- Modify: `ui/src/appContext.tsx`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/App.tsx`
- Modify: `ui/src/commands.ts`
- Modify: `ui/src/storybook/mockAppState.ts`

**Interfaces:**
- Consumes backend projection with `internalSceneId` and nullable `sceneIndex`.
- Produces frontend command methods targeting internal scene IDs.
- Produces `linkSceneConfig(sourceInternalSceneId, targetSceneIndex, overwriteExisting)` and `deleteSceneConfig(internalSceneId)`.

- [ ] **Step 1: Update TypeScript types first**

In `ui/src/types.ts`, change `SceneConfig` to:

```ts
export type SceneConfig = {
  internalSceneId: string;
  sceneIndex: number | null;
  sceneName: string;
  durationMs: number;
  scopeToggles: SceneScopeToggles;
  channelConfigs: ChannelConfig[];
  scopedChannels: ChannelRef[];
};
```

Replace `cuedSceneId` and `selectedSceneId` with UUID-based names if the backend projection changed names. Prefer:

```ts
cuedSceneInternalId: string | null;
selectedSceneInternalId: string | null;
```

- [ ] **Step 2: Run typecheck and collect failures**

Run: `npm run typecheck`

Expected: FAIL with remaining `sceneId` and nullable-index errors.

- [ ] **Step 3: Update command service interfaces**

In `AppRuntimeServices` and `AppCommands`, replace scene ID arguments with internal scene ID strings. Add:

```ts
linkSceneConfig: (
  sourceInternalSceneId: string,
  targetSceneIndex: number,
  overwriteExisting: boolean,
) => Promise<unknown>;
deleteSceneConfig: (internalSceneId: string) => Promise<unknown>;
```

In `ui/src/commands.ts`, add invoke wrappers:

```ts
export async function linkSceneConfig(
  sourceInternalSceneId: string,
  targetSceneIndex: number,
  overwriteExisting: boolean,
) {
  return invoke<void>("link_scene_config", {
    sourceInternalSceneId,
    targetSceneIndex,
    overwriteExisting,
  });
}

export async function deleteSceneConfig(internalSceneId: string) {
  return invoke<void>("delete_scene_config", { internalSceneId });
}
```

- [ ] **Step 4: Update fixtures and existing components to compile**

Replace fixture fields:

- `sceneId` -> `internalSceneId`
- `sceneIndex: number` remains for linked scenes
- add at least one unlinked fixture with `sceneIndex: null`

Replace selected/cued comparisons to use internal UUID fields.

- [ ] **Step 5: Run frontend typecheck**

Run: `npm run typecheck`

Expected: PASS.

- [ ] **Step 6: Run frontend unit tests**

Run: `npm run test -- --run`

Expected: PASS after fixture updates.

- [ ] **Step 7: Commit**

```bash
git status --short
git add ui/src
git commit -m "refactor: use internal scene ids in ui"
```

### Task 7: Add Unlinked Scene List And Editor UX

**Files:**
- Modify: `ui/src/components/SceneListRow.tsx`
- Modify: `ui/src/components/SceneList.tsx`
- Modify: `ui/src/components/SceneEditor.tsx`
- Modify: `ui/src/components/SelectedSceneHeader.tsx`
- Modify: `ui/src/components/SelectedSceneActions.tsx`
- Create: `ui/src/components/UnlinkedSceneControls.tsx`
- Modify related stories/tests

**Interfaces:**
- Consumes: `scene.sceneIndex === null` means unlinked.
- Consumes: `appState.scenes` as current LV1 scenes for link dropdown.
- Produces: disabled Store/Cue/Recall for unlinked configs.
- Produces: Link/Delete controls for unlinked configs.

- [ ] **Step 1: Write failing UI tests**

Add tests for:

```tsx
it("renders unlinked scene rows with warning styling", () => { /* expect -- and alert label/icon */ });
it("disables Store Cue and Recall for unlinked scenes", () => { /* selected unlinked */ });
it("keeps duration and scope controls enabled for unlinked scenes", () => { /* selected unlinked */ });
it("shows link and delete controls for unlinked scenes", () => { /* dropdown + buttons */ });
it("shows overwrite confirmation when linking to a scene with an existing config", async () => { /* click link target */ });
```

- [ ] **Step 2: Run focused UI tests and verify failure**

Run: `npm run test -- --run ui/src/components/SceneListRow.test.tsx ui/src/components/SceneEditor.test.tsx`

Expected: FAIL because unlinked UI does not exist.

- [ ] **Step 3: Update scene row display**

In `SceneListRow.tsx`, derive:

```ts
const unlinked = props.scene.sceneIndex === null;
```

Render scene number as:

```tsx
{unlinked ? "--" : formatSceneNumber(props.scene.sceneIndex)}
```

Apply warning text classes for unlinked rows. Add an accessible label or title for the alert marker, such as `aria-label="Unlinked scene"`.

- [ ] **Step 4: Disable Store/Cue/Recall for unlinked selected scene**

Pass `scene` into `SelectedSceneActions` instead of only ID. Disable Store when `scene.sceneIndex === null`.

In `SelectedSceneHeader`, disable Recall/Cue buttons when unlinked.

- [ ] **Step 5: Add unlinked controls component**

Create `UnlinkedSceneControls.tsx` with props:

```ts
export function UnlinkedSceneControls(props: {
  scene: SceneConfig;
  lv1Scenes: SceneSummary[];
  existingConfigs: SceneConfig[];
})
```

It should render a select/dropdown, `Link to LV1 Scene`, and `Delete`. On link, check whether any existing linked config has the chosen `sceneIndex`. If yes, show confirmation before calling `commands.linkSceneConfig(scene.internalSceneId, targetIndex, true)`. If no, call with `overwriteExisting = false`.

- [ ] **Step 6: Render unlinked controls in `SceneEditor`**

Render normal header/grid and add `UnlinkedSceneControls` when selected scene is unlinked:

```tsx
{selected.sceneIndex === null ? (
  <UnlinkedSceneControls
    scene={selected}
    lv1Scenes={appState.scenes}
    existingConfigs={appState.sceneConfigs}
  />
) : null}
```

- [ ] **Step 7: Preserve duplicate-name warning**

Keep or update the current duplicate scene names warning in `SceneList.tsx`. Ensure nullable `sceneIndex` does not break it.

- [ ] **Step 8: Run focused UI tests**

Run: `npm run test -- --run ui/src/components/SceneListRow.test.tsx ui/src/components/SceneEditor.test.tsx`

Expected: PASS.

- [ ] **Step 9: Run Storybook tests if stories changed**

Run: `npm run test:storybook -- --run`

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git status --short
git add ui/src/components ui/src/storybook
git commit -m "feat: show and link unlinked scenes"
```

---

## Phase 4: Verification And Cleanup

### Task 8: Remove Temporary Research File And Final Verification

**Files:**
- Delete: `research.md`
- Modify docs only if implementation changed the approved design in a user-visible way.

**Interfaces:**
- Produces: clean worktree with no temporary research artifact.
- Produces: full verification evidence.

- [ ] **Step 1: Ensure `research.md` is deleted**

Run: `git status --short`

Expected: includes `D research.md` if not already committed as deleted.

- [ ] **Step 2: Search for forbidden legacy fields**

Run: `rg "sceneId|scene_id|parse_scene_id|reconcile_scene_fade_configs" src-tauri ui docs/superpowers/specs docs/superpowers/plans`

Expected: no production references to `sceneId`, `scene_id`, `parse_scene_id`, or `reconcile_scene_fade_configs`. Mentions in old committed historical docs are acceptable only if outside current spec/plan scope; do not edit unrelated historical docs just to remove old wording.

- [ ] **Step 3: Run Rust verification**

Run: `make rust-fmt`

Expected: PASS.

Run: `make rust-lint`

Expected: PASS.

Run: `make rust-test`

Expected: PASS.

- [ ] **Step 4: Run frontend verification**

Run: `make ui-fmt`

Expected: PASS.

Run: `make ui-lint`

Expected: PASS.

Run: `make ui-typecheck`

Expected: PASS.

Run: `make ui-test`

Expected: PASS.

Run: `make ui-storybook-test`

Expected: PASS.

- [ ] **Step 5: Run broader verification**

Run: `make check`

Expected: PASS.

- [ ] **Step 6: Commit final cleanup**

```bash
git status --short
git add -A
git commit -m "chore: verify session scene alignment"
```

Only commit if there are actual cleanup/doc changes. Do not create an empty commit.

---

## Self-Review Notes

- Spec coverage: internal UUIDs, deletion of `scene_id`, nullable scene index, pure aligner, direct actor call, no report object, deterministic classifier, delete/ambiguous unlink behavior, duplicate warning, link/delete commands, disabled unlinked actions, persistence, and tests are covered.
- The plan is phased as “make the change easy, then make the easy change”: identity/persistence/commands first, aligner/linking/UI second.
- Type names are consistent: Rust `internal_scene_id`, serialized/frontend `internalSceneId`, Rust nullable `scene_index: Option<i32>`, frontend nullable `sceneIndex: number | null`.
- `research.md` deletion is included as a final cleanup task and has already been applied in the current worktree.
