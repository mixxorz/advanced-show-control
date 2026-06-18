# Cue Recall Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the existing Cue, Recall, and Go controls functional while preserving LV1 as the scene-recall source of truth.

**Architecture:** Store the cued scene id in `ShowState` and project it through `AppViewState`. Route recall through `ShellState -> AppCommandBus -> Lv1Actor`, where `Lv1Actor` sends `/Set/CurSceneIndex`; `SceneRecallFader` remains the only path that starts app-managed fades after LV1 confirms scene identity.

**Tech Stack:** Rust, Tokio, Tauri commands, serde, tracing, React/TypeScript, Tauri `invoke`.

## Global Constraints

- Cueing is allowed while lockout is active because it only changes app show state and sends no LV1 command.
- Recall must respect lockout and must not send `/Set/CurSceneIndex` when lockout is active.
- Requests are DEBUG level. Facts are INFO. Blocked safety outcomes and command failures are WARN unless existing shared infrastructure logs them differently.
- Do not add duplicate logging for the same fact at multiple layers; follow existing logging patterns.
- The cued scene stays set after Go/Recall; auto-next and cue lists are out of scope.
- Do not bypass exact scene identity validation, stale-state checks, generation guards, or existing scene recall fade safety.
- Use TDD for behavior changes and safety logic.

---

## File Structure

- Modify `src/show/types.rs`: add `ShowSnapshot.cued_scene_id: Option<String>` and serialization/default tests.
- Modify `src/show/state.rs`: store `ShowState.cued_scene_id`, add cue mutation helpers, include cue in snapshots/replacement/clear, and preserve/drop cue during scene reconciliation based on scene id validity.
- Modify `src/show/handle.rs`: expose async cue helper methods to shell code.
- Modify `src-tauri/src/app_state/view.rs`: add `AppViewState.cued_scene_id: Option<String>` projected as `cuedSceneId`.
- Modify `src-tauri/src/app_state/shell.rs`: project cue from `ShowSnapshot`; add tests for default and cued snapshots.
- Modify `src-tauri/src/show_file.rs`: add optional `cued_scene_id` to `ShowFile` with serde default so old files load.
- Modify `src-tauri/src/app_state/show_file_mapping.rs`: save/load `cued_scene_id`, clear it if the referenced scene is pruned.
- Modify `src/lv1/commands.rs`: add `Lv1Command::RecallScene { scene_index, reply }`.
- Modify `src/lv1/handle.rs`: add `Lv1ActorHandle::recall_scene(scene_index: i32)`.
- Modify `src/lv1/actor.rs`: handle disconnected recall and encode `/Set/CurSceneIndex` while connected.
- Modify `src/runtime/commands.rs`: add `AppCommandBus::recall_scene(scene_index: i32)`.
- Modify `src-tauri/src/commands.rs`: add Tauri `cue_scene` and `recall_scene` commands with validation and logging.
- Modify `src-tauri/src/main.rs`: register the new Tauri commands.
- Modify `ui/src/types.ts`: add `cuedSceneId` to the concrete app state type/default if not already present in all required definitions.
- Modify `ui/src/AppRuntime.tsx`: add cue/recall service methods and command bindings.
- Modify `ui/src/App.tsx`: wire services to `invoke("cue_scene")` and `invoke("recall_scene")`.

---

### Task 1: Store And Project Cued Scene State

**Files:**
- Modify: `src/show/types.rs`
- Modify: `src/show/state.rs`
- Modify: `src/show/handle.rs`
- Modify: `src-tauri/src/app_state/view.rs`
- Modify: `src-tauri/src/app_state/shell.rs`
- Modify: `src-tauri/src/show_file.rs`
- Modify: `src-tauri/src/app_state/show_file_mapping.rs`

**Interfaces:**
- Produces: `ShowSnapshot { lockout: bool, scene_configs: Vec<SceneConfig>, cued_scene_id: Option<String> }`
- Produces: `ShowState::cue_scene(&mut self, scene_id: &str) -> Result<bool, String>`
- Produces: `ShowStateHandle::cue_scene(&self, scene_id: String) -> Result<bool, String>`
- Produces: `ShowStateHandle::get_scene_config(&self, scene_id: String) -> Option<SceneConfig>` remains unchanged and is used by later recall validation.
- Produces: `AppViewState.cued_scene_id: Option<String>` serialized to frontend as `cuedSceneId`.

- [ ] **Step 1: Add failing show snapshot serialization/default tests**

Add these tests to `src/show/types.rs` inside the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn show_snapshot_serializes_cued_scene_id_for_frontend_camel_case() {
        let snapshot = ShowSnapshot {
            lockout: false,
            scene_configs: Vec::new(),
            cued_scene_id: Some("1::Verse".to_string()),
        };

        let json = serde_json::to_value(snapshot).unwrap();

        assert_eq!(json["cuedSceneId"], "1::Verse");
    }

    #[test]
    fn empty_show_snapshot_has_no_cued_scene() {
        let snapshot = ShowSnapshot::empty();

        assert_eq!(snapshot.cued_scene_id, None);
    }
```

- [ ] **Step 2: Run failing show type tests**

Run: `cargo nextest run -p advanced-show-control show::types::tests::show_snapshot_serializes_cued_scene_id_for_frontend_camel_case show::types::tests::empty_show_snapshot_has_no_cued_scene`

Expected: FAIL because `ShowSnapshot` has no `cued_scene_id` field.

- [ ] **Step 3: Implement `ShowSnapshot.cued_scene_id`**

In `src/show/types.rs`, change `ShowSnapshot` to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShowSnapshot {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_id: Option<String>,
}

impl ShowSnapshot {
    pub fn empty() -> Self {
        Self {
            lockout: false,
            scene_configs: Vec::new(),
            cued_scene_id: None,
        }
    }
}
```

Then update every existing `ShowSnapshot { ... }` literal compiler error by adding `cued_scene_id: None` unless the test intentionally needs a cued scene.

- [ ] **Step 4: Run show type tests again**

Run: `cargo nextest run -p advanced-show-control show::types::tests::show_snapshot_serializes_cued_scene_id_for_frontend_camel_case show::types::tests::empty_show_snapshot_has_no_cued_scene`

Expected: PASS.

- [ ] **Step 5: Add failing `ShowState` cue tests**

Add these tests to `src/show/state.rs` inside the existing `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn cue_scene_stores_existing_scene_id_in_snapshot() {
        let mut state = ShowState::default();
        state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "Verse".to_string(),
        }]);

        let changed = state.cue_scene("1::Verse").unwrap();

        assert!(changed);
        assert_eq!(state.snapshot().cued_scene_id, Some("1::Verse".to_string()));
    }

    #[test]
    fn cue_scene_rejects_unknown_scene_id() {
        let mut state = ShowState::default();
        state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "Verse".to_string(),
        }]);

        let err = state.cue_scene("2::Chorus").unwrap_err();

        assert_eq!(err, "Scene config not found");
        assert_eq!(state.snapshot().cued_scene_id, None);
    }

    #[test]
    fn cue_scene_noops_when_scene_is_already_cued() {
        let mut state = ShowState::default();
        state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "Verse".to_string(),
        }]);
        assert!(state.cue_scene("1::Verse").unwrap());

        let changed = state.cue_scene("1::Verse").unwrap();

        assert!(!changed);
        assert_eq!(state.snapshot().cued_scene_id, Some("1::Verse".to_string()));
    }

    #[test]
    fn clear_removes_cued_scene_id() {
        let mut state = ShowState::default();
        state.reconcile_scene_fade_configs(&[SceneListEntry {
            index: 1,
            name: "Verse".to_string(),
        }]);
        state.cue_scene("1::Verse").unwrap();

        state.clear();

        assert_eq!(state.snapshot().cued_scene_id, None);
    }
```

- [ ] **Step 6: Run failing `ShowState` cue tests**

Run: `cargo nextest run -p advanced-show-control cue_scene clear_removes_cued_scene_id`

Expected: FAIL because `ShowState::cue_scene` and `ShowState.cued_scene_id` do not exist.

- [ ] **Step 7: Implement `ShowState` cue storage**

In `src/show/state.rs`, change `ShowState` to:

```rust
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShowState {
    pub lockout: bool,
    pub scene_configs: Vec<SceneConfig>,
    pub cued_scene_id: Option<String>,
}
```

Update `snapshot()`:

```rust
    pub fn snapshot(&self) -> ShowSnapshot {
        ShowSnapshot {
            lockout: self.lockout,
            scene_configs: self.scene_configs.clone(),
            cued_scene_id: self.cued_scene_id.clone(),
        }
    }
```

Add this method in `impl ShowState` near the other mutators:

```rust
    pub fn cue_scene(&mut self, scene_id: &str) -> Result<bool, String> {
        if !self.scene_configs.iter().any(|scene| scene.scene_id == scene_id) {
            return Err("Scene config not found".to_string());
        }

        let next = Some(scene_id.to_string());
        if self.cued_scene_id == next {
            return Ok(false);
        }

        self.cued_scene_id = next;
        Ok(true)
    }
```

Update `replace_snapshot` to assign `self.cued_scene_id = snapshot.cued_scene_id;`.

Update `clear` to assign `self.cued_scene_id = None;`.

If a scene reconciliation removes or renames the currently cued scene id, use this helper after any reconciliation that changes `scene_configs`:

```rust
    fn clear_missing_cue(&mut self) {
        if let Some(cued_scene_id) = &self.cued_scene_id
            && !self.scene_configs.iter().any(|scene| &scene.scene_id == cued_scene_id)
        {
            self.cued_scene_id = None;
        }
    }
```

Call `self.clear_missing_cue();` before returning from `reconcile_scene_fade_configs` when the reconciliation result is `true`.

Update all direct `ShowState { ... }` literals in tests with `cued_scene_id: None` unless the test needs a cue.

- [ ] **Step 8: Run `ShowState` cue tests again**

Run: `cargo nextest run -p advanced-show-control cue_scene clear_removes_cued_scene_id`

Expected: PASS.

- [ ] **Step 9: Add `ShowStateHandle` cue method**

In `src/show/handle.rs`, add:

```rust
    pub async fn cue_scene(&self, scene_id: String) -> Result<bool, String> {
        self.state.lock().await.cue_scene(&scene_id)
    }
```

- [ ] **Step 10: Add failing Tauri projection/show-file tests**

Add this test to `src-tauri/src/app_state/shell.rs` tests:

```rust
    #[tokio::test]
    async fn snapshot_exposes_cued_scene_id() {
        let state = ShellState::default();
        state
            .show
            .replace_snapshot(ShowSnapshot {
                lockout: false,
                scene_configs: vec![advanced_show_control::show::types::SceneConfig {
                    scene_id: "1::Verse".to_string(),
                    scene_index: 1,
                    scene_name: "Verse".to_string(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: Default::default(),
                }],
                cued_scene_id: Some("1::Verse".to_string()),
            })
            .await;

        let snapshot = state.snapshot().await;

        assert_eq!(snapshot.cued_scene_id, Some("1::Verse".to_string()));
    }
```

Add this test to `src-tauri/src/show_file.rs` tests:

```rust
    #[test]
    fn show_file_deserializes_missing_cued_scene_id_as_none() {
        let json = r#"{
            "schemaVersion": 1,
            "appVersion": "0.1.0",
            "savedAt": "2026-06-18T00:00:00Z",
            "safety": { "lockout": false },
            "sceneConfigs": []
        }"#;

        let file: ShowFile = serde_json::from_str(json).unwrap();

        assert_eq!(file.cued_scene_id, None);
    }
```

- [ ] **Step 11: Run failing Tauri projection/show-file tests**

Run: `cargo nextest run -p advanced-show-control-tauri snapshot_exposes_cued_scene_id show_file_deserializes_missing_cued_scene_id_as_none`

Expected: FAIL because `AppViewState` and `ShowFile` do not expose cue state.

- [ ] **Step 12: Implement projection and show-file cue field**

In `src-tauri/src/app_state/view.rs`, add to `AppViewState`:

```rust
    pub cued_scene_id: Option<String>,
```

In `src-tauri/src/app_state/shell.rs`, add `cued_scene_id: show.cued_scene_id,` in `snapshot_from_parts`.

In `src-tauri/src/show_file.rs`, add to `ShowFile`:

```rust
    #[serde(default)]
    pub cued_scene_id: Option<String>,
```

In `src-tauri/src/app_state/show_file_mapping.rs`, add `cued_scene_id: show.cued_scene_id,` to `export_show_file_for_save` and add this in `load_show_file_from_dto` before `replace_snapshot`:

```rust
        let cued_scene_id = file.cued_scene_id.clone().filter(|scene_id| {
            file.scene_configs
                .iter()
                .any(|config| scene_id == &advanced_show_control::show::types::scene_id(config.scene_index, &config.scene_name))
        });
```

Then include `cued_scene_id,` in the replacement `ShowSnapshot`.

Update all `AppViewState { ... }` and `ShowFile { ... }` literals that fail compilation with `cued_scene_id: None`.

- [ ] **Step 13: Run Task 1 tests**

Run: `cargo nextest run -p advanced-show-control cue_scene clear_removes_cued_scene_id show::types::tests::show_snapshot_serializes_cued_scene_id_for_frontend_camel_case show::types::tests::empty_show_snapshot_has_no_cued_scene`

Expected: PASS.

Run: `cargo nextest run -p advanced-show-control-tauri snapshot_exposes_cued_scene_id show_file_deserializes_missing_cued_scene_id_as_none`

Expected: PASS.

- [ ] **Step 14: Commit Task 1**

Run: `git status --short`

Stage only Task 1 files:

```bash
git add src/show/types.rs src/show/state.rs src/show/handle.rs src-tauri/src/app_state/view.rs src-tauri/src/app_state/shell.rs src-tauri/src/show_file.rs src-tauri/src/app_state/show_file_mapping.rs
git commit -m "feat: store cued scene state"
```

---

### Task 2: Add LV1 Scene Recall Command Path

**Files:**
- Modify: `src/lv1/commands.rs`
- Modify: `src/lv1/handle.rs`
- Modify: `src/lv1/actor.rs`
- Modify: `src/runtime/commands.rs`

**Interfaces:**
- Consumes: `Lv1ActorHandle` command channel pattern from existing `set_gain`, `set_pan`, and `set_mute` methods.
- Produces: `Lv1ActorHandle::recall_scene(&self, scene_index: i32) -> Result<(), Lv1ActorError>`
- Produces: `AppCommandBus::recall_scene(&self, scene_index: i32) -> Result<(), AppCommandError>`

- [ ] **Step 1: Add failing LV1 handle test**

Add this test to `src/lv1/handle.rs` tests:

```rust
    #[tokio::test]
    async fn handle_sends_recall_scene_command() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let handle = Lv1ActorHandle::new(tx);

        let recall = tokio::spawn(async move { handle.recall_scene(4).await });

        if let Some(Lv1Command::RecallScene { scene_index, reply }) = rx.recv().await {
            assert_eq!(scene_index, 4);
            reply.send(Ok(())).unwrap();
        } else {
            panic!("expected RecallScene command");
        }

        assert!(recall.await.unwrap().is_ok());
    }
```

- [ ] **Step 2: Run failing LV1 handle test**

Run: `cargo nextest run -p advanced-show-control lv1::handle::tests::handle_sends_recall_scene_command`

Expected: FAIL because `RecallScene` and `recall_scene` do not exist.

- [ ] **Step 3: Add LV1 command enum variant and handle method**

In `src/lv1/commands.rs`, add:

```rust
    RecallScene {
        scene_index: i32,
        reply: oneshot::Sender<Result<(), Lv1ActorError>>,
    },
```

In `src/lv1/handle.rs`, add:

```rust
    /// Send a `/Set/CurSceneIndex` command to LV1.
    pub async fn recall_scene(&self, scene_index: i32) -> Result<(), Lv1ActorError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(Lv1Command::RecallScene {
                scene_index,
                reply: reply_tx,
            })
            .await
            .map_err(|_| Lv1ActorError::CommandChannelClosed)?;

        reply_rx
            .await
            .map_err(|_| Lv1ActorError::ReplyChannelClosed)?
    }
```

- [ ] **Step 4: Run LV1 handle test again**

Run: `cargo nextest run -p advanced-show-control lv1::handle::tests::handle_sends_recall_scene_command`

Expected: PASS after handling compiler errors in exhaustive matches by adding disconnected handling in the next step if needed.

- [ ] **Step 5: Add actor tests for disconnected and encoding behavior**

Add this test to `src/lv1/actor.rs` tests:

```rust
    #[test]
    fn drain_disconnected_command_rejects_recall_scene_when_not_connected() {
        let state = ActorState::new(AppEventBus::default());
        let (reply, rx) = oneshot::channel();

        drain_disconnected_command(
            Lv1Command::RecallScene {
                scene_index: 4,
                reply,
            },
            &state,
            Err(Lv1ActorError::NotConnected),
        );

        assert_eq!(rx.blocking_recv().unwrap(), Err(Lv1ActorError::NotConnected));
    }

    #[test]
    fn recall_scene_frame_uses_set_cur_scene_index() {
        let bytes = encode_frame("/Set/CurSceneIndex", &[OscArg::Int(4)]).unwrap();
        let mut decoder = FrameDecoder::default();
        let frames = decoder.push(&bytes).unwrap();
        let msg = decode_frame_payload(&frames[0]).unwrap();

        assert_eq!(msg.address, "/Set/CurSceneIndex");
        assert_eq!(msg.args, vec![OscArg::Int(4)]);
    }
```

- [ ] **Step 6: Run actor tests**

Run: `cargo nextest run -p advanced-show-control drain_disconnected_command_rejects_recall_scene_when_not_connected recall_scene_frame_uses_set_cur_scene_index`

Expected: FAIL until actor disconnected handling is added; the frame encoding test may already pass because it uses existing `encode_frame`.

- [ ] **Step 7: Implement actor recall handling**

In `src/lv1/actor.rs`, update the disconnected command comment to mention `RecallScene`, then add to `drain_disconnected_command`:

```rust
        Lv1Command::RecallScene { reply, .. } => {
            let _ = reply.send(Err(Lv1ActorError::NotConnected));
        }
```

Add this branch in `run_connected` near other set commands:

```rust
                    Some(Lv1Command::RecallScene { scene_index, reply }) => {
                        let result = encode_frame(
                            "/Set/CurSceneIndex",
                            &[OscArg::Int(scene_index)],
                        )
                        .map_err(|_| Lv1ActorError::CommandSendFailed)
                        .and_then(|bytes| {
                            enqueue_writer_bytes(&writer_tx, bytes)
                                .map_err(|_| Lv1ActorError::CommandSendFailed)
                        });

                        let failed = result.is_err();
                        let _ = reply.send(result);
                        if failed {
                            return DisconnectReason::TcpError(
                                "RecallScene send failed (encode or writer queue)".to_string(),
                            );
                        }
                    }
```

- [ ] **Step 8: Run LV1 actor/handle tests**

Run: `cargo nextest run -p advanced-show-control handle_sends_recall_scene_command drain_disconnected_command_rejects_recall_scene_when_not_connected recall_scene_frame_uses_set_cur_scene_index`

Expected: PASS.

- [ ] **Step 9: Add failing command bus test**

Add this test to `src/runtime/commands.rs` tests:

```rust
    #[tokio::test]
    async fn recall_scene_sends_index_to_lv1_actor() {
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;

        let recall = tokio::spawn({
            let bus = bus.clone();
            async move { bus.recall_scene(4).await }
        });

        if let Some(crate::lv1::commands::Lv1Command::RecallScene { scene_index, reply }) = lv1_rx.recv().await {
            assert_eq!(scene_index, 4);
            reply.send(Ok(())).unwrap();
        } else {
            panic!("expected RecallScene command");
        }

        assert!(recall.await.unwrap().is_ok());
    }
```

- [ ] **Step 10: Run failing command bus test**

Run: `cargo nextest run -p advanced-show-control runtime::commands::tests::recall_scene_sends_index_to_lv1_actor`

Expected: FAIL because `AppCommandBus::recall_scene` does not exist.

- [ ] **Step 11: Implement `AppCommandBus::recall_scene`**

In `src/runtime/commands.rs`, add to `impl AppCommandBus` near other LV1 methods:

```rust
    pub async fn recall_scene(&self, scene_index: i32) -> Result<(), AppCommandError> {
        let lv1 = self.targets.lock().await.lv1.clone();
        let result = match lv1 {
            Some(lv1) => lv1.recall_scene(scene_index).await.map_err(map_lv1_error),
            None => Err(AppCommandError::Lv1Unavailable),
        };
        log_failure("recall_scene", &result);
        result
    }
```

- [ ] **Step 12: Run Task 2 tests**

Run: `cargo nextest run -p advanced-show-control recall_scene`

Expected: PASS.

- [ ] **Step 13: Commit Task 2**

Run: `git status --short`

Stage only Task 2 files:

```bash
git add src/lv1/commands.rs src/lv1/handle.rs src/lv1/actor.rs src/runtime/commands.rs
git commit -m "feat: add lv1 scene recall command"
```

---

### Task 3: Add Tauri Commands, Safety Validation, Logging, And UI Wiring

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `ui/src/types.ts`
- Modify: `ui/src/AppRuntime.tsx`
- Modify: `ui/src/App.tsx`

**Interfaces:**
- Consumes: `ShowStateHandle::cue_scene(scene_id: String) -> Result<bool, String>` from Task 1.
- Consumes: `AppCommandBus::recall_scene(scene_index: i32) -> Result<(), AppCommandError>` from Task 2.
- Produces: Tauri command `cue_scene(app, state, scene_id) -> Result<AppViewState, String>`.
- Produces: Tauri command `recall_scene(app, state, active_command_bus, scene_id) -> Result<AppViewState, String>`.
- Produces: UI service methods `cueScene(sceneId)` and `recallScene(sceneId)`.

- [ ] **Step 1: Add failing Tauri command tests for cue behavior**

Add these tests to `src-tauri/src/commands.rs` tests:

```rust
    #[tokio::test]
    async fn cue_scene_updates_show_state_even_when_lockout_is_enabled() {
        let state = ShellState::default();
        state
            .show
            .replace_snapshot(advanced_show_control::show::types::ShowSnapshot {
                lockout: true,
                scene_configs: vec![advanced_show_control::show::types::SceneConfig {
                    scene_id: "1::Verse".to_string(),
                    scene_index: 1,
                    scene_name: "Verse".to_string(),
                    duration_ms: 0,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: Default::default(),
                }],
                cued_scene_id: None,
            })
            .await;

        let snapshot = cue_scene_snapshot(state.clone(), "1::Verse".to_string()).await.unwrap();

        assert_eq!(snapshot.cued_scene_id, Some("1::Verse".to_string()));
        assert!(snapshot.lockout);
        assert!(snapshot.show_file_dirty);
    }

    #[tokio::test]
    async fn cue_scene_rejects_unknown_scene_id() {
        let state = ShellState::default();

        let err = cue_scene_snapshot(state.clone(), "99::Missing".to_string()).await.unwrap_err();

        assert_eq!(err, "Scene config not found");
        assert_eq!(state.snapshot().await.cued_scene_id, None);
    }
```

- [ ] **Step 2: Run failing cue command tests**

Run: `cargo nextest run -p advanced-show-control-tauri cue_scene_updates_show_state_even_when_lockout_is_enabled cue_scene_rejects_unknown_scene_id`

Expected: FAIL because `cue_scene_snapshot` does not exist.

- [ ] **Step 3: Implement `cue_scene` Tauri command**

In `src-tauri/src/commands.rs`, add a helper and command near `select_scene_config`:

```rust
#[tauri::command]
pub async fn cue_scene(
    app: AppHandle,
    state: State<'_, ShellState>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot = cue_scene_snapshot((*state).clone(), scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

async fn cue_scene_snapshot(state: ShellState, scene_id: String) -> Result<AppViewState, String> {
    tracing::debug!(
        event = "scene_cue_requested",
        scene_id = %scene_id,
        "Scene cue requested"
    );

    let scene = state
        .show
        .get_scene_config(scene_id.clone())
        .await
        .ok_or_else(|| {
            tracing::warn!(
                event = "scene_cue_blocked",
                scene_id = %scene_id,
                reason = "scene config not found",
                "Scene cue blocked: scene config not found"
            );
            "Scene config not found".to_string()
        })?;

    let changed = state.show.cue_scene(scene_id.clone()).await?;
    if changed {
        state.inner.lock().await.show_file_dirty = true;
    }

    tracing::info!(
        event = "scene_cued",
        scene_id = %scene.scene_id,
        scene_index = scene.scene_index,
        scene_name = %scene.scene_name,
        "Scene cued: {}",
        scene.scene_name
    );

    Ok(state.snapshot().await)
}
```

This logs one debug request and one info fact. It does not check lockout.

- [ ] **Step 4: Run cue command tests again**

Run: `cargo nextest run -p advanced-show-control-tauri cue_scene_updates_show_state_even_when_lockout_is_enabled cue_scene_rejects_unknown_scene_id`

Expected: PASS.

- [ ] **Step 5: Add failing Tauri recall validation tests**

Add these helper/test snippets to `src-tauri/src/commands.rs` tests. If similar helpers already exist, reuse their setup instead of duplicating names.

```rust
    fn scene_config(scene_index: i32, scene_name: &str) -> advanced_show_control::show::types::SceneConfig {
        advanced_show_control::show::types::SceneConfig {
            scene_id: advanced_show_control::show::types::scene_id(scene_index, scene_name),
            scene_index,
            scene_name: scene_name.to_string(),
            duration_ms: 0,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: Default::default(),
        }
    }

    async fn state_with_scene(lockout: bool) -> ShellState {
        let state = ShellState::default();
        state
            .show
            .replace_snapshot(advanced_show_control::show::types::ShowSnapshot {
                lockout,
                scene_configs: vec![scene_config(1, "Verse")],
                cued_scene_id: Some("1::Verse".to_string()),
            })
            .await;
        state
    }

    #[tokio::test]
    async fn recall_scene_blocks_when_lockout_is_enabled() {
        let state = state_with_scene(true).await;
        let active_command_bus = ActiveCommandBus::default();

        let err = recall_scene_snapshot(
            state,
            active_command_bus,
            "1::Verse".to_string(),
        )
        .await
        .unwrap_err();

        assert_eq!(err, "Recall blocked: lockout is enabled");
    }

    #[tokio::test]
    async fn recall_scene_blocks_without_lv1_state() {
        let state = state_with_scene(false).await;
        let active_command_bus = ActiveCommandBus::default();

        let err = recall_scene_snapshot(
            state,
            active_command_bus,
            "1::Verse".to_string(),
        )
        .await
        .unwrap_err();

        assert_eq!(err, "Recall blocked: LV1 state is unavailable");
    }
```

- [ ] **Step 6: Run failing recall validation tests**

Run: `cargo nextest run -p advanced-show-control-tauri recall_scene_blocks_when_lockout_is_enabled recall_scene_blocks_without_lv1_state`

Expected: FAIL because `recall_scene_snapshot` does not exist.

- [ ] **Step 7: Implement recall validation helper skeleton**

In `src-tauri/src/commands.rs`, add this near `cue_scene_snapshot`:

```rust
#[tauri::command]
pub async fn recall_scene(
    app: AppHandle,
    state: State<'_, ShellState>,
    active_command_bus: State<'_, ActiveCommandBus>,
    scene_id: String,
) -> Result<AppViewState, String> {
    let snapshot = recall_scene_snapshot((*state).clone(), (*active_command_bus).clone(), scene_id).await?;
    emit_snapshot(&app, &snapshot);
    Ok(snapshot)
}

async fn recall_scene_snapshot(
    state: ShellState,
    active_command_bus: ActiveCommandBus,
    scene_id: String,
) -> Result<AppViewState, String> {
    tracing::debug!(
        event = "scene_recall_requested",
        scene_id = %scene_id,
        "Scene recall requested"
    );

    let show = state.show.get_snapshot().await;
    if show.lockout {
        return block_scene_recall(&scene_id, "lockout is enabled", "Recall blocked: lockout is enabled");
    }

    let scene = show
        .scene_configs
        .iter()
        .find(|scene| scene.scene_id == scene_id)
        .cloned()
        .ok_or_else(|| {
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = "scene config not found",
                "Scene recall blocked: scene config not found"
            );
            "Scene config not found".to_string()
        })?;

    let lv1 = state
        .inner
        .lock()
        .await
        .lv1_snapshot
        .clone()
        .ok_or_else(|| {
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = "LV1 state is unavailable",
                "Scene recall blocked: LV1 state is unavailable"
            );
            "Recall blocked: LV1 state is unavailable".to_string()
        })?;

    if lv1.connection != advanced_show_control::lv1::types::ConnectionStatus::Connected {
        return block_scene_recall(&scene_id, "LV1 is disconnected", "Recall blocked: LV1 is disconnected");
    }

    let Some(lv1_scene) = lv1.scene_list.iter().find(|candidate| {
        candidate.index == scene.scene_index && candidate.name == scene.scene_name
    }) else {
        return block_scene_recall(&scene_id, "scene identity mismatch", "Recall blocked: scene identity mismatch");
    };

    let command_bus = active_command_bus
        .current()
        .await
        .ok_or_else(|| {
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = "LV1 command target is unavailable",
                "Scene recall blocked: LV1 command target is unavailable"
            );
            "Recall blocked: LV1 command target is unavailable".to_string()
        })?;

    if let Err(error) = command_bus.recall_scene(lv1_scene.index).await {
        tracing::warn!(
            event = "scene_recall_command_failed",
            scene_id = %scene.scene_id,
            scene_index = scene.scene_index,
            scene_name = %scene.scene_name,
            error = %error,
            "Scene recall command failed: {error}"
        );
        return Err(error.to_string());
    }

    tracing::info!(
        event = "scene_recall_command_sent",
        scene_id = %scene.scene_id,
        scene_index = scene.scene_index,
        scene_name = %scene.scene_name,
        "Scene recall command sent: {}",
        scene.scene_name
    );

    Ok(state.snapshot().await)
}

fn block_scene_recall<T>(scene_id: &str, reason: &'static str, message: &'static str) -> Result<T, String> {
    tracing::warn!(
        event = "scene_recall_blocked",
        scene_id = %scene_id,
        reason,
        "{message}"
    );
    Err(message.to_string())
}
```

This intentionally keeps the request log in the Tauri command layer and the generic `command_failed` log in `AppCommandBus`; do not add another duplicate debug/info log in `AppCommandBus`.

- [ ] **Step 8: Run recall validation tests again**

Run: `cargo nextest run -p advanced-show-control-tauri recall_scene_blocks_when_lockout_is_enabled recall_scene_blocks_without_lv1_state`

Expected: PASS.

- [ ] **Step 9: Add failing successful recall command test**

Add this test to `src-tauri/src/commands.rs` tests:

```rust
    #[tokio::test]
    async fn recall_scene_sends_lv1_command_and_keeps_cue_set() {
        let state = state_with_scene(false).await;
        {
            let mut inner = state.inner.lock().await;
            inner.lv1_snapshot = Some(advanced_show_control::lv1::types::Lv1StateSnapshot {
                connection: advanced_show_control::lv1::types::ConnectionStatus::Connected,
                scene: None,
                scene_list: vec![advanced_show_control::lv1::types::SceneListEntry {
                    index: 1,
                    name: "Verse".to_string(),
                }],
                channels: Vec::new(),
            });
        }
        let active_command_bus = ActiveCommandBus::default();
        let command_bus = AppCommandBus::new(AppEventBus::default());
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        command_bus.set_lv1(Some(advanced_show_control::lv1::handle::Lv1ActorHandle::new(lv1_tx))).await;
        active_command_bus.set(Some(command_bus)).await;

        let recall = tokio::spawn({
            let state = state.clone();
            let active_command_bus = active_command_bus.clone();
            async move {
                recall_scene_snapshot(state, active_command_bus, "1::Verse".to_string()).await
            }
        });

        if let Some(advanced_show_control::lv1::commands::Lv1Command::RecallScene { scene_index, reply }) = lv1_rx.recv().await {
            assert_eq!(scene_index, 1);
            reply.send(Ok(())).unwrap();
        } else {
            panic!("expected RecallScene command");
        }

        let snapshot = recall.await.unwrap().unwrap();
        assert_eq!(snapshot.cued_scene_id, Some("1::Verse".to_string()));
    }
```

- [ ] **Step 10: Run successful recall command test**

Run: `cargo nextest run -p advanced-show-control-tauri recall_scene_sends_lv1_command_and_keeps_cue_set`

Expected: PASS after imports are fixed. If the `Lv1StateSnapshot` literal has new fields, fill them from `Lv1StateSnapshot::default()` if available or add the missing fields with empty/default values.

- [ ] **Step 11: Register Tauri commands**

In `src-tauri/src/main.rs`, add to `tauri::generate_handler![...]`:

```rust
            commands::cue_scene,
            commands::recall_scene,
```

Place them near `select_scene_config`.

- [ ] **Step 12: Wire frontend service and command bindings**

In `ui/src/AppRuntime.tsx`, add to `AppRuntimeServices`:

```ts
  cueScene: (sceneId: string) => Promise<AppViewState>;
  recallScene: (sceneId: string) => Promise<AppViewState>;
```

In the `commands: AppCommands` object, add:

```ts
    cueScene: (sceneId) => runSnapshot(() => services.cueScene(sceneId)),
    recallScene: (sceneId) => runSnapshot(() => services.recallScene(sceneId)),
```

In `ui/src/App.tsx`, add to `services`:

```ts
  cueScene: (sceneId) => invoke<AppViewState>("cue_scene", { sceneId }),
  recallScene: (sceneId) => invoke<AppViewState>("recall_scene", { sceneId }),
```

In `ui/src/types.ts`, ensure `AppViewState` includes:

```ts
  cuedSceneId: string | null;
```

and ensure `disconnectedAppViewState` includes:

```ts
  cuedSceneId: null,
```

- [ ] **Step 13: Run targeted Rust and UI type checks**

Run: `cargo nextest run -p advanced-show-control-tauri cue_scene recall_scene`

Expected: PASS.

Run from `ui/`: `npm run typecheck`

Expected: PASS.

- [ ] **Step 14: Commit Task 3**

Run: `git status --short`

Stage only Task 3 files:

```bash
git add src-tauri/src/commands.rs src-tauri/src/main.rs ui/src/types.ts ui/src/AppRuntime.tsx ui/src/App.tsx
git commit -m "feat: wire cue and recall commands"
```

---

### Task 4: Final Verification And Documentation Check

**Files:**
- Modify: `docs/architecture.md` only if implementation changes ownership beyond the design above.
- Modify: `docs/roadmap.md` only if scope changes from the approved spec.

**Interfaces:**
- Consumes all previous task outputs.
- Produces verified cue/recall behavior ready for review.

- [ ] **Step 1: Run formatting checks**

Run: `cargo fmt --all -- --check`

Expected: PASS. If it fails, run `cargo fmt --all`, then rerun `cargo fmt --all -- --check`.

- [ ] **Step 2: Run targeted Rust tests**

Run: `cargo nextest run -p advanced-show-control recall_scene cue_scene`

Expected: PASS.

Run: `cargo nextest run -p advanced-show-control-tauri recall_scene cue_scene snapshot_exposes_cued_scene_id show_file_deserializes_missing_cued_scene_id_as_none`

Expected: PASS.

- [ ] **Step 3: Run frontend checks affected by command wiring**

Run from `ui/`: `npm run typecheck`

Expected: PASS.

Run from `ui/`: `npm run test -- BottomStatusBar`

Expected: PASS.

- [ ] **Step 4: Run broader Rust safety check if targeted tests pass**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 5: Inspect final diff**

Run: `git status --short`

Run: `git diff --stat`

Run: `git diff -- src/show/types.rs src/show/state.rs src/show/handle.rs src/lv1/commands.rs src/lv1/handle.rs src/lv1/actor.rs src/runtime/commands.rs src-tauri/src/commands.rs src-tauri/src/main.rs src-tauri/src/app_state/view.rs src-tauri/src/app_state/shell.rs src-tauri/src/show_file.rs src-tauri/src/app_state/show_file_mapping.rs ui/src/types.ts ui/src/AppRuntime.tsx ui/src/App.tsx`

Expected: Only cue/recall-related changes are present. No unrelated user changes are staged or modified by this work.

- [ ] **Step 6: Commit verification/doc adjustments if needed**

If Task 4 required docs changes, stage only those docs and commit:

```bash
git add docs/architecture.md docs/roadmap.md
git commit -m "docs: update cue recall architecture notes"
```

If no docs changes are needed, do not create an empty commit.

---

## Self-Review

Spec coverage:

- Stored cued scene state: Task 1.
- Backend cue and recall commands: Task 3.
- LV1 `/Set/CurSceneIndex`: Task 2.
- Existing `SceneRecallFader` remains fade starter: Tasks 2 and 3 send only LV1 scene recall.
- Lockout allows cue but blocks recall: Task 3 tests and implementation.
- Logging levels and no duplicate facts: Task 3 explicitly places debug request logs in Tauri commands, info facts in Tauri commands, and avoids extra AppCommandBus fact logs.
- Frontend wiring: Task 3.
- Verification: Task 4.

Placeholder scan: no `TBD`, `TODO`, or unspecified implementation steps remain.

Type consistency: `cued_scene_id` is Rust-side snake_case and serializes to frontend `cuedSceneId`; `cue_scene` and `recall_scene` are Tauri command names; `cueScene` and `recallScene` are frontend command names.
