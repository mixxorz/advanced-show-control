# Cue List Review Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore safe, predictable cue-list behavior during scene mutation, session loading, runtime reconnects, and live GO operation.

**Architecture:** Keep cue documents and cue position in the app-lifetime `cue_lists` actor. Reconcile them against active-generation scene projection facts and explicit post-import scene IDs, install runtime peers only after generation acceptance, and derive frontend arming/status from valid projected cue state.

**Tech Stack:** Rust 2024, Tokio actors and broadcast events, Tauri 2, tracing, React, TypeScript, Vitest, Testing Library, Storybook.

## Global Constraints

- Successful GO advancement remains persisted session state and marks the show dirty.
- Preserve missing-scene cue entries; clear only invalid active/cued pointers.
- Keep recall safety in `scenes`; cue recall must continue through `ScenesCommand::RecallScene`.
- Ignore stale-generation scene events and stale runtime candidates.
- Use `tracing` with stable event fields and complete user-facing warning messages.
- Follow TDD: add one failing behavior test, run it and observe the expected failure, then write minimal production code.
- Rust tests must be pure unit tests or actor tests through mailboxes, `AppEventBus`, and tracing when logging is asserted.

---

### Task 1: Normalize Cue Documents And Active-List No-Ops

**Files:**
- Modify: `src-tauri/src/cue_lists/state.rs:10-220`
- Test: `src-tauri/src/cue_lists/state.rs:230-392`

**Interfaces:**
- Consumes: `CueListDocument`, `Uuid`, and an iterator of valid scene IDs.
- Produces: `CueListsState::replace_document(document, valid_scene_ids) -> CueListReconciliation` and no-op-aware `set_active_cue_list` behavior.

- [ ] **Step 1: Add failing pure unit tests for structural and scene reconciliation**

Add tests using fixed UUIDs:

```rust
#[test]
fn replacing_document_clears_invalid_active_and_cued_ids() {
    let mut state = CueListsState::default();
    let document = CueListDocument {
        cue_lists: vec![],
        active_cue_list_id: Some(id(1)),
        cued_cue_entry_id: Some(id(2)),
    };

    let result = state.replace_document(document, [id(10)]);

    assert!(result.active_cue_list_cleared);
    assert!(result.cued_entry_cleared.is_some());
    assert_eq!(state.document().active_cue_list_id, None);
    assert_eq!(state.document().cued_cue_entry_id, None);
}

#[test]
fn reconciliation_preserves_missing_entry_but_clears_current_cue() {
    let list_id = id(1);
    let entry_id = id(2);
    let missing_scene_id = id(3);
    let mut state = CueListsState::default();
    let document = CueListDocument {
        cue_lists: vec![CueList {
            id: list_id,
            name: "Main".to_string(),
            entries: vec![CueEntry { id: entry_id, scene_internal_id: missing_scene_id }],
        }],
        active_cue_list_id: Some(list_id),
        cued_cue_entry_id: Some(entry_id),
    };

    let result = state.replace_document(document, [id(99)]);

    assert_eq!(result.cued_entry_cleared.unwrap().scene_internal_id, missing_scene_id);
    assert_eq!(state.document().cue_lists[0].entries.len(), 1);
    assert_eq!(state.document().cued_cue_entry_id, None);
}

#[test]
fn setting_already_active_list_preserves_cue_and_reports_unchanged() {
    let mut state = state_with_two_entries();
    let active = state.document().active_cue_list_id.unwrap();
    let cued = state.document().cued_cue_entry_id;

    let changed = state.set_active_cue_list(Some(active)).unwrap();

    assert!(!changed);
    assert_eq!(state.document().cued_cue_entry_id, cued);
}
```

- [ ] **Step 2: Run the targeted tests and verify RED**

Run:

```bash
cargo nextest run -p advanced-show-control cue_lists::state::tests
```

Expected: compilation or assertion failures because `replace_document` does not accept valid scene IDs, no reconciliation result exists, and `set_active_cue_list` does not return change state.

- [ ] **Step 3: Implement minimal reconciliation state**

Add these domain types and behavior in `state.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearedCueEntry {
    pub cue_list_id: Uuid,
    pub cue_entry_id: Uuid,
    pub scene_internal_id: Uuid,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CueListReconciliation {
    pub active_cue_list_cleared: bool,
    pub cued_entry_cleared: Option<ClearedCueEntry>,
}

pub fn replace_document(
    &mut self,
    document: CueListDocument,
    valid_scene_ids: impl IntoIterator<Item = Uuid>,
) -> CueListReconciliation {
    self.document = document;
    self.reconcile(valid_scene_ids)
}

pub fn reconcile(
    &mut self,
    valid_scene_ids: impl IntoIterator<Item = Uuid>,
) -> CueListReconciliation {
    let valid_scene_ids = valid_scene_ids.into_iter().collect::<std::collections::HashSet<_>>();
    let mut result = CueListReconciliation::default();
    let active_id = self.document.active_cue_list_id;
    let active = active_id.and_then(|id| self.document.cue_lists.iter().find(|list| list.id == id));
    if active_id.is_some() && active.is_none() {
        self.document.active_cue_list_id = None;
        self.document.cued_cue_entry_id = None;
        result.active_cue_list_cleared = true;
        return result;
    }
    if let (Some(list), Some(cued_id)) = (active, self.document.cued_cue_entry_id)
        && let Some(entry) = list.entries.iter().find(|entry| entry.id == cued_id)
        && !valid_scene_ids.contains(&entry.scene_internal_id)
    {
        result.cued_entry_cleared = Some(ClearedCueEntry {
            cue_list_id: list.id,
            cue_entry_id: entry.id,
            scene_internal_id: entry.scene_internal_id,
        });
        self.document.cued_cue_entry_id = None;
    } else {
        self.clear_invalid_cue();
    }
    result
}
```

Change `set_active_cue_list` to return `Result<bool, String>`, returning `Ok(false)` before mutation when the requested ID is already active and `Ok(true)` after a real change. Update its actor call site to set `CueListsCommandResult.changed` from the returned bool and publish only when changed.

- [ ] **Step 4: Run targeted tests and verify GREEN**

Run:

```bash
cargo nextest run -p advanced-show-control cue_lists::state::tests
```

Expected: all cue-list state tests pass.

- [ ] **Step 5: Commit the state invariant**

```bash
git add src-tauri/src/cue_lists/state.rs src-tauri/src/cue_lists/actor.rs
git commit -m "fix: normalize cue list documents"
```

---

### Task 2: Reconcile Cue State From Scene Events

**Files:**
- Modify: `src-tauri/src/cue_lists/actor.rs:14-352`
- Modify: `src-tauri/src/cue_lists/commands.rs`
- Modify: `src-tauri/src/cue_lists/mod.rs`
- Test: `src-tauri/src/cue_lists/actor.rs:354-494`

**Interfaces:**
- Consumes: `AppEvent::Runtime(ActiveGenerationChanged)` and generation-bearing `AppEvent::Scenes(StateChanged)`.
- Produces: actor-owned `active_generation`, latest valid scene IDs, persisted cue cleanup events, and `cue_cleared_missing_scene` warning.

- [ ] **Step 1: Add failing actor tests for valid, invalid, and stale scene facts**

Build the actor with an event subscription, create/cue an entry through mailbox commands, and publish scene facts. Assert observable events rather than private state:

```rust
#[tokio::test]
async fn active_scene_fact_clears_missing_current_cue_and_publishes_persisted_edit() {
    let event_bus = AppEventBus::default();
    let mut events = event_bus.subscribe();
    let (handle, task, _) = build_cue_lists_actor(event_bus.clone());
    task.spawn();
    let fixture = create_and_cue_entry(&handle, id(10)).await;
    event_bus.publish_runtime_generation_changed(7);
    event_bus.publish(AppEvent::Scenes {
        generation: 7,
        event: ScenesEvent::StateChanged {
            reason: ScenesProjectionReason::SceneState,
            state: ScenesProjectionState { scene_configs: vec![], selected_scene_internal_id: None },
            persisted_scene_edit: true,
        },
    });

    let projected = recv_cue_lists_state(&mut events).await;
    assert!(projected.persisted_cue_list_edit);
    assert_eq!(projected.state.document.cued_cue_entry_id, None);
    assert_eq!(projected.state.document.cue_lists[0].entries[0].id, fixture.entry_id);
}

#[tokio::test]
async fn stale_scene_fact_does_not_clear_current_cue() {
    let event_bus = AppEventBus::default();
    let (handle, task, _) = build_cue_lists_actor(event_bus.clone());
    task.spawn();
    let fixture = create_and_cue_entry(&handle, id(10)).await;
    event_bus.publish_runtime_generation_changed(8);
    event_bus.publish(AppEvent::Scenes {
        generation: 7,
        event: ScenesEvent::StateChanged {
            reason: ScenesProjectionReason::SceneState,
            state: ScenesProjectionState { scene_configs: vec![], selected_scene_internal_id: None },
            persisted_scene_edit: true,
        },
    });
    tokio::task::yield_now().await;
    let (reply, rx) = oneshot::channel();
    handle.send(CueListsCommand::InitialProjectionState { reply }).await.unwrap();
    assert_eq!(rx.await.unwrap().document.cued_cue_entry_id, Some(fixture.entry_id));
}
```

Add a tracing-listener assertion that the invalid active-generation case emits one warning with event `cue_cleared_missing_scene` and message `Cued entry cleared because its scene is unavailable.` Follow the existing tracing listener patterns in `src-tauri/src/logging.rs` tests.

- [ ] **Step 2: Run actor tests and verify RED**

Run:

```bash
cargo nextest run -p advanced-show-control cue_lists::actor::tests
```

Expected: the invalid cue remains because the actor receives commands only and never consumes scene/runtime events.

- [ ] **Step 3: Add event consumption and reconciliation**

Add `event_rx: broadcast::Receiver<AppEvent>` to `CueListsTask`, initialize it with `event_bus.subscribe()`, and replace the command-only loop with `tokio::select!` over commands and events. Track `active_generation: u64` and `valid_scene_ids: HashSet<Uuid>` in the actor task.

On active-generation `ScenesEvent::StateChanged`, collect `state.scene_configs[*].internal_scene_id`, call `CueListsState::reconcile`, and only when it clears the current cue:

```rust
tracing::warn!(
    event = "cue_cleared_missing_scene",
    cue_list_id = %cleared.cue_list_id,
    cue_entry_id = %cleared.cue_entry_id,
    scene_internal_id = %cleared.scene_internal_id,
    "Cued entry cleared because its scene is unavailable."
);
publish_state(
    &event_bus,
    &state,
    CueListsProjectionReason::CueListState,
    true,
);
```

Ignore scene events whose generation differs from `active_generation`. Continue logging lagged subscriber errors at `DEBUG` with a stable event field, matching other actor subscribers.

Extend `CueListsCommand::ReplaceCueListDocument` with `valid_scene_ids: Vec<Uuid>` so file replacement is deterministic and does not depend on event subscriber ordering. Pass those IDs to `replace_document`.

- [ ] **Step 4: Run actor and show tests and verify GREEN**

Run:

```bash
cargo nextest run -p advanced-show-control cue_lists
cargo nextest run -p advanced-show-control show::actor::tests
```

Expected: all targeted tests pass; unchanged scene facts publish no cue-list mutation event.

- [ ] **Step 5: Commit event-driven reconciliation**

```bash
git add src-tauri/src/cue_lists
git commit -m "fix: reconcile cues with scene state"
```

---

### Task 3: Normalize Session Loads

**Files:**
- Modify: `src-tauri/src/show/show_file.rs:7-175`
- Modify: `src-tauri/src/show/actor.rs:400-550`
- Modify: relevant `ShowFile` literals in Rust tests/debug fixtures
- Test: `src-tauri/src/show/show_file.rs:252-440`
- Test: `src-tauri/src/show/actor.rs:553-1122`

**Interfaces:**
- Consumes: aligned `SceneDocument` and imported `CueListDocument`.
- Produces: `ReplaceCueListDocument { valid_scene_ids }`.

- [ ] **Step 1: Add failing import and actor tests**

Verify schema-v1 scene-level cue state is ignored without producing cue-list state:

```rust
#[test]
fn import_schema_v1_silently_ignores_legacy_cued_scene() {
    let json = r#"{
      "schemaVersion":1,
      "appVersion":"0.1.0",
      "savedAt":"123",
      "safety":{"lockout":false},
      "sceneConfigs":[],
      "cuedSceneInternalId":"00000000-0000-0000-0000-000000000001"
    }"#;
    let mut file: ShowFile = serde_json::from_str(json).unwrap();
    let imported = import_show_file(&mut file, &lv1_with_scene()).unwrap();

    assert!(!imported.report.removed_anything());
    assert!(imported.cue_list_snapshot.cue_lists.is_empty());
}
```

Add an actor load test where the imported cue document contains a current cue referencing a scene ID absent from the final aligned scene document. Assert the replacement cue projection has no cued ID but retains the entry.

- [ ] **Step 2: Run show-file and actor tests and verify RED**

Run:

```bash
cargo nextest run -p advanced-show-control show::show_file::tests
cargo nextest run -p advanced-show-control show::actor::tests
```

Expected: cue replacement does not receive final valid scene IDs.

- [ ] **Step 3: Implement deterministic replacement**

After scene alignment in `load_show_file_from_dto`, derive:

```rust
let valid_scene_ids = aligned_scene_configs
    .iter()
    .map(|scene| scene.internal_scene_id)
    .collect();
replace_cue_list_document(peers, cue_list_snapshot, valid_scene_ids, false).await?;
```

Update `replace_cue_list_document` and every command constructor to pass the vector. Do not deserialize or report the former schema-v1 scene-level cue field.

- [ ] **Step 4: Run targeted tests and verify GREEN**

Run:

```bash
cargo nextest run -p advanced-show-control show
cargo nextest run -p advanced-show-control cue_lists
```

Expected: all show and cue-list tests pass, including silent schema-v1 cue discard coverage.

- [ ] **Step 5: Commit session reconciliation**

```bash
git add src-tauri/src/show src-tauri/src/cue_lists src-tauri/src/ui/debug
git commit -m "fix: reconcile cues when loading sessions"
```

---

### Task 4: Install Cue Recall Peers Only For Accepted Generations

**Files:**
- Modify: `src-tauri/src/lifecycle/mod.rs:75-114,199-217,257-310`
- Test: `src-tauri/src/lifecycle/mod.rs` test module

**Interfaces:**
- Consumes: candidate `BuiltConnectedRuntime.scene_recall_fader` and lifecycle generation.
- Produces: accepted-generation-only installation into `CueListsPeers`.

- [ ] **Step 1: Add failing lifecycle boundary tests**

Replace the existing build-time peer assertion with explicit pre-acceptance and accepted-connect tests. The first test fails on the current branch because `build_connected_runtime` mutates `CueListsPeers` immediately:

```rust
#[tokio::test]
async fn building_runtime_does_not_install_cue_list_peer_before_acceptance() {
    let event_bus = AppEventBus::default();
    let lifecycle = lifecycle_for_test(event_bus.clone());
    let identity = Lv1SystemIdentity {
        uuid: Some("uuid-1".to_string()),
        address: "127.0.0.1".parse().unwrap(),
        host: Some("localhost".to_string()),
        port: 9000,
    };
    let generation = lifecycle.begin_connecting().await.unwrap();

    let _candidate = build_connected_runtime(
        generation,
        lifecycle.current_runtime_generation().await,
        &identity,
        lifecycle.show.clone(),
        lifecycle.show_peers.clone(),
        lifecycle.cue_lists_peers.clone(),
        event_bus,
    );

    assert!(lifecycle.cue_lists_peers.scenes().is_none());
}
```

Rename `connected_runtime_installs_scenes_peer_on_show` to `accepted_connected_runtime_installs_scene_peers` and drive it through the existing mocked successful connect helper. Assert both `lifecycle.show_peers.scenes().is_some()` and `lifecycle.cue_lists_peers.scenes().is_some()` after the accepted connect returns.

- [ ] **Step 2: Run the lifecycle test and verify RED**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle::tests::building_runtime_does_not_install_cue_list_peer_before_acceptance
```

Expected: stale peer receives the recall or the current peer no longer does.

- [ ] **Step 3: Move peer mutation after transaction acceptance**

Remove `cue_lists_peers.set_scenes` from `build_connected_runtime`. Carry the candidate scenes handle through `BuiltConnectedRuntime`. In `connect_to_identity`, call `install_runtime_transaction` first, then install the accepted handle:

```rust
self.install_runtime_transaction(generation, handles).await.map_err(|rejection| {
    let mut handles = rejection.into_handles();
    handles.abort_all();
    self.show_peers.clear_lv1(generation);
    "generation is stale".to_string()
})?;
self.cue_lists_peers
    .set_scenes(built_runtime.scene_recall_fader.clone());
```

Ensure all early failures after accepted installation call the existing generation-checked clear path, which clears `CueListsPeers`.

- [ ] **Step 4: Run lifecycle and safety tests and verify GREEN**

Run:

```bash
cargo nextest run -p advanced-show-control lifecycle
cargo nextest run -p advanced-show-control scene_recall
```

Expected: lifecycle and scene recall safety tests pass.

- [ ] **Step 5: Commit generation-safe peer installation**

```bash
git add src-tauri/src/lifecycle/mod.rs
git commit -m "fix: guard cue recall peer installation"
```

---

### Task 5: Derive Footer Arming And Guard In-Flight GO

**Files:**
- Modify: `ui/src/components/BottomStatusBar.tsx:15-94`
- Test: `ui/src/components/BottomStatusBar.test.tsx`

**Interfaces:**
- Consumes: projected `AppViewState` and `AppCommands.recallCuedCue() -> void | Promise<void>`.
- Produces: one resolved cue helper, truthful Cued display, validity-based GO enablement, and an in-flight submission guard.

- [ ] **Step 1: Add failing frontend tests**

Update the render helper to accept command overrides. Add tests:

```tsx
it("does not use the selected scene as the cued fallback", () => {
  renderBottomStatusBar({
    ...connectedAppState,
    activeCueListId: null,
    cuedCueEntryId: null,
    selectedSceneInternalId: connectedAppState.sceneConfigs[0].internalSceneId,
  });
  expect(screen.getByText("---")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "GO" })).toBeDisabled();
});

it("dispatches only one GO while recall is unresolved", async () => {
  const user = userEvent.setup();
  let resolveRecall!: () => void;
  const recallCuedCue = vi.fn(() => new Promise<void>((resolve) => { resolveRecall = resolve; }));
  renderBottomStatusBar(validCuedAppState, { recallCuedCue });

  const go = screen.getByRole("button", { name: "GO" });
  await user.dblClick(go);
  expect(recallCuedCue).toHaveBeenCalledTimes(1);
  expect(go).toBeDisabled();
  resolveRecall();
  await waitFor(() => expect(go).toBeEnabled());
});
```

Also test missing active list, missing entry, missing scene config, and rejected promise recovery.

- [ ] **Step 2: Run the component test and verify RED**

Run:

```bash
npm --prefix ui run test -- src/components/BottomStatusBar.test.tsx
```

Expected: GO remains enabled without a cue, selected scene appears as Cued, and double click invokes twice.

- [ ] **Step 3: Implement resolved cue and in-flight state**

Replace `cuedSceneLabel` with one helper:

```tsx
function resolveCuedScene(appState: AppViewState) {
  const activeList = appState.cueLists.find((list) => list.id === appState.activeCueListId);
  const entry = activeList?.entries.find((candidate) => candidate.id === appState.cuedCueEntryId);
  return entry
    ? appState.sceneConfigs.find((scene) => scene.internalSceneId === entry.sceneInternalId) ?? null
    : null;
}
```

Inside the component:

```tsx
const [goPending, setGoPending] = useState(false);
const cuedScene = resolveCuedScene(props.appState);
const canGo = cuedScene !== null && !goPending;

async function handleGo() {
  if (!canGo) return;
  setGoPending(true);
  try {
    await commands.recallCuedCue();
  } finally {
    setGoPending(false);
  }
}
```

Render `cuedScene?.sceneName ?? "---"`, use default tone when null, and call `void handleGo()`.

- [ ] **Step 4: Run component tests and verify GREEN**

Run:

```bash
npm --prefix ui run test -- src/components/BottomStatusBar.test.tsx
```

Expected: all footer tests pass without unhandled promise warnings.

- [ ] **Step 5: Commit truthful GO behavior**

```bash
git add ui/src/components/BottomStatusBar.tsx ui/src/components/BottomStatusBar.test.tsx
git commit -m "fix: guard cue list GO operation"
```

---

### Task 6: Reconcile Cue-List UI Selection

**Files:**
- Modify: `ui/src/components/CueListsTab.tsx:21-225`
- Modify: `ui/src/components/CueListManageModal.tsx:94-106`
- Test: `ui/src/components/CueListsTab.test.tsx`
- Test: `ui/src/components/CueListManageModal.test.tsx`

**Interfaces:**
- Consumes: active projected cue list and local selected entry ID.
- Produces: valid-only Cue action and no activation command when clicking the active list.

- [ ] **Step 1: Add failing rerender and active-list tests**

Use Testing Library `rerender` with providers or the existing mutable fixture pattern:

```tsx
it("disables Cue when the selected entry disappears", async () => {
  const user = userEvent.setup();
  const rendered = renderCueListsTab(validState);
  await user.click(screen.getByText("Intro"));
  expect(screen.getByRole("button", { name: "Cue" })).toBeEnabled();

  rendered.rerender(tree(stateWithoutSelectedEntry));
  expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();
});

it("does not reactivate the already-active cue list", async () => {
  const user = userEvent.setup();
  const setActiveCueList = vi.fn();
  renderManageModal({ setActiveCueList });
  await user.click(screen.getByText("Main"));
  expect(setActiveCueList).not.toHaveBeenCalled();
});
```

Add this explicit active-list switch test:

```tsx
it("disables Cue when the active list changes", async () => {
  const user = userEvent.setup();
  const rendered = renderCueListsTab(validState);
  await user.click(screen.getByText("Intro"));
  rendered.rerender(tree({
    ...validState,
    activeCueListId: "encore-list",
  }));
  expect(screen.getByRole("button", { name: "Cue" })).toBeDisabled();
});
```

- [ ] **Step 2: Run both component tests and verify RED**

Run:

```bash
npm --prefix ui run test -- src/components/CueListsTab.test.tsx src/components/CueListManageModal.test.tsx
```

Expected: stale Cue remains enabled and active-list click invokes `setActiveCueList`.

- [ ] **Step 3: Implement projection reconciliation and same-list guard**

Import `useEffect` and reconcile local state:

```tsx
const selectedCueEntry = activeCueList?.entries.find(
  (entry) => entry.id === selectedCueEntryId,
) ?? null;

useEffect(() => {
  if (selectedCueEntryId !== null && selectedCueEntry === null) {
    setSelectedCueEntryId(null);
  }
}, [selectedCueEntry, selectedCueEntryId]);
```

Enable Cue from `selectedCueEntry !== null` and send `selectedCueEntry?.id ?? null`, never the unreconciled state value. In the manage modal:

```tsx
onSelect={() => {
  if (cueList.id !== appState.activeCueListId) {
    void commands.setActiveCueList?.(cueList.id);
  }
  props.onClose();
}}
```

- [ ] **Step 4: Run both component tests and verify GREEN**

Run:

```bash
npm --prefix ui run test -- src/components/CueListsTab.test.tsx src/components/CueListManageModal.test.tsx
```

Expected: all cue-list interaction tests pass.

- [ ] **Step 5: Commit UI selection fixes**

```bash
git add ui/src/components/CueListsTab.tsx ui/src/components/CueListsTab.test.tsx ui/src/components/CueListManageModal.tsx ui/src/components/CueListManageModal.test.tsx
git commit -m "fix: reconcile cue list UI selection"
```

---

### Task 7: Preserve Native Button Events And Correct Scene Cue Styling

**Files:**
- Modify: `ui/src/components/ConsoleIconButton.tsx`
- Create: `ui/src/components/ConsoleIconButton.test.tsx`
- Modify: `ui/src/components/SceneEditor.tsx:7-33`
- Test: `ui/src/components/SceneEditor.test.tsx`

**Interfaces:**
- Consumes: standard `ButtonHTMLAttributes<HTMLButtonElement>` and projected cue-list state.
- Produces: transparent native button prop forwarding and actual-cue-only editor styling.

- [ ] **Step 1: Add failing component tests**

```tsx
it("forwards pointer handlers to the native button", () => {
  const onPointerDown = vi.fn();
  render(
    <ConsoleIconButton aria-label="Delete" onPointerDown={onPointerDown}>
      X
    </ConsoleIconButton>,
  );
  fireEvent.pointerDown(screen.getByRole("button", { name: "Delete" }));
  expect(onPointerDown).toHaveBeenCalledTimes(1);
});
```

For `SceneEditor`, assert the selected header does not have the cued treatment when another/no entry is cued, then does when the active cued entry references the selected scene. Assert through the visible status text/classes already exposed by `SelectedSceneHeader`, not component internals.

- [ ] **Step 2: Run tests and verify RED**

Run:

```bash
npm --prefix ui run test -- src/components/ConsoleIconButton.test.tsx src/components/SceneEditor.test.tsx
```

Expected: pointer handler is not called and a merely selected scene renders as cued.

- [ ] **Step 3: Forward props and derive the actual cued scene**

Destructure custom props and spread the rest:

```tsx
export function ConsoleIconButton({
  size = "default",
  variant = "secondary",
  className,
  type = "button",
  children,
  ...buttonProps
}: ConsoleIconButtonProps) {
  return (
    <button
      {...buttonProps}
      className={`inline-grid place-items-center rounded-console-control bg-transparent disabled:text-console-disabled ${sizeClass} ${variantClass} ${className ?? ""}`}
      type={type}
    >
      {children}
    </button>
  );
}
```

In `SceneEditor`, resolve the active list, cued entry, and selected match:

```tsx
const activeCueList = appState.cueLists.find((list) => list.id === appState.activeCueListId);
const cuedEntry = activeCueList?.entries.find((entry) => entry.id === appState.cuedCueEntryId);
const selectedIsCued = cuedEntry?.sceneInternalId === selected.internalSceneId;
```

Pass `cued={selectedIsCued}`.

- [ ] **Step 4: Run tests and verify GREEN**

Run:

```bash
npm --prefix ui run test -- src/components/ConsoleIconButton.test.tsx src/components/SceneEditor.test.tsx
```

Expected: both component suites pass.

- [ ] **Step 5: Commit component corrections**

```bash
git add ui/src/components/ConsoleIconButton.tsx ui/src/components/ConsoleIconButton.test.tsx ui/src/components/SceneEditor.tsx ui/src/components/SceneEditor.test.tsx
git commit -m "fix: preserve cue component interactions"
```

---

### Task 8: Update Stories, Documentation, And Run Full Verification

**Files:**
- Modify: `ui/src/components/BottomStatusBar.stories.tsx`
- Modify: `ui/src/components/CueListsTab.stories.tsx`
- Modify: `docs/architecture.md:325-339`

**Interfaces:**
- Consumes: completed backend and frontend behavior from Tasks 1-7.
- Produces: representative stories, current architecture docs, and CI-equivalent verification evidence.

- [ ] **Step 1: Add or update representative Storybook states**

Ensure stories cover these projected states using backend-shaped fixtures:

```tsx
export const NoValidCue: Story = {
  args: {
    appState: {
      ...connectedAppState,
      cuedCueEntryId: null,
      selectedSceneInternalId: connectedAppState.sceneConfigs[0].internalSceneId,
    },
  },
};

export const MissingCuedScene: Story = {
  args: {
    appState: missingSceneCueState,
  },
};
```

Do not create a frontend-only blocked state that the backend cannot project.

- [ ] **Step 2: Run formatting and targeted complete suites**

Run:

```bash
make fmt
cargo nextest run -p advanced-show-control cue_lists
cargo nextest run -p advanced-show-control lifecycle
cargo nextest run -p advanced-show-control show
npm --prefix ui run test
npm --prefix ui run test:storybook
```

Expected: all commands exit 0 with no warnings or failed tests.

- [ ] **Step 3: Run full non-visual verification**

Run:

```bash
make check
```

Expected: Rust formatting, clippy, tests, and build plus frontend formatting, lint, typecheck, build, unit tests, and Storybook tests all pass.

- [ ] **Step 4: Evaluate visual snapshot impact**

If corrected footer/editor states change existing screenshots, run:

```bash
make visual-test
```

Expected: either snapshots pass unchanged or failures correspond only to intentional Cued/GO/editor-state corrections. If intentional, regenerate with `make visual-update`, inspect the changed PNGs, and rerun `make visual-test`.

- [ ] **Step 5: Commit documentation and intentional story/snapshot updates**

```bash
git add docs/architecture.md ui/src/components ui/tests/visual/storybook.visual.spec.ts-snapshots
git commit -m "test: cover cue list remediation states"
```

- [ ] **Step 6: Confirm clean intended worktree state**

Run:

```bash
git status --short
git log --oneline -10
```

Expected: no uncommitted remediation files remain; unrelated concurrent changes, if any, are left untouched and reported.
