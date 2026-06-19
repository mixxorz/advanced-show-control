# Show-Owned UI Recall Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move UI-requested scene recall validation and LV1 dispatch out of the Tauri command adapter and into `show/`, reached through `AppCommandBus`.

**Architecture:** This implements phase 11 of `docs/superpowers/specs/2026-06-19-single-crate-command-projection-architecture-design.md`. `show/` owns the UI recall use case, `AppCommandBus` supplies the command boundary and LV1 query/dispatch target, and `src-tauri/src/commands.rs` remains a temporary Tauri adapter that still returns and directly emits `AppViewState` until later projector/frontend-contract phases.

**Tech Stack:** Rust, Tokio, Tauri command handlers, `AppCommandBus`, `ShowStateHandle`, `Lv1ActorHandle`, `cargo nextest`.

## Global Constraints

- Preserve all current Tauri command names and frontend command return payloads in this phase.
- Preserve temporary direct `AppViewState` returns and direct `app-status-changed` emits until projector-only and frontend command-contract phases.
- Do not change LV1 protocol behavior; UI recall must still send `/Set/CurrSceneIndex` through `Lv1ActorHandle::recall_scene`.
- Do not weaken lockout checks, exact scene identity validation, disconnected/unavailable checks, generation guards, disconnect behavior, manual override, abort, or overlap safety behavior.
- UI-requested recall validation must no longer live in `ui/` or the Tauri command adapter after this plan.
- LV1-observed recall fade automation remains in `scene_recall/`; do not move `SceneRecallFader` policy.
- Do not route logs through `AppEventBus`.
- Do not remove `ShellState`, `ActiveCommandBus`, command-return snapshots, direct emits, or `app-status-changed` behavior in this phase.
- Use the smallest correct change and keep behavior test-covered.

---

## File Structure

- Modify `src-tauri/src/show/commands.rs`: add show-owned UI recall request validation and result types.
- Modify `src-tauri/src/runtime/commands.rs`: add `AppCommandBus::recall_scene_by_id(scene_id: String)` and keep low-level `recall_scene(scene_index: i32)` for LV1 index dispatch.
- Modify `src-tauri/src/commands.rs`: simplify Tauri `recall_scene_snapshot` to call the new command-bus method, log based on the returned result/error, then return the existing shell snapshot.
- Modify `docs/architecture.md`: record that UI-requested recall now routes through `show/` and `AppCommandBus`; projector/cache/frontend cleanup remains pending.

## Task 1: Show-Owned Recall Validation

**Files:**
- Modify: `src-tauri/src/show/commands.rs`

**Interfaces:**
- Consumes: `ShowStateHandle::get_snapshot() -> ShowSnapshot` and `Lv1StateSnapshot`.
- Produces: `pub struct RecallSceneResult { pub scene: SceneConfig, pub lv1_scene_index: i32 }`.
- Produces: `pub fn validate_recall_scene_request(show: &ShowSnapshot, lv1: &Lv1StateSnapshot, scene_id: &str) -> Result<RecallSceneResult, String>`.

- [ ] **Step 1: Write failing tests for show-owned recall validation**

Add these tests to the existing `#[cfg(test)]` module at the bottom of `src-tauri/src/show/commands.rs`. If the file has no test module, add one after the command functions.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::{ConnectionStatus, Lv1StateSnapshot, SceneListEntry};
    use crate::show::types::{SceneConfig, ShowSnapshot};

    fn recall_show(lockout: bool) -> ShowSnapshot {
        ShowSnapshot {
            lockout,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: Default::default(),
            }],
            cued_scene_id: Some("1::Verse".to_string()),
        }
    }

    fn recall_lv1(connection: ConnectionStatus, name: &str) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: name.to_string(),
            }],
            channels: Vec::new(),
        }
    }

    #[test]
    fn validate_recall_scene_request_blocks_lockout_before_lv1_identity() {
        let show = recall_show(true);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Different");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: lockout is enabled");
    }

    #[test]
    fn validate_recall_scene_request_blocks_missing_scene_config() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Verse");

        let err = validate_recall_scene_request(&show, &lv1, "2::Chorus").unwrap_err();

        assert_eq!(err, "Scene config not found");
    }

    #[test]
    fn validate_recall_scene_request_blocks_disconnected_lv1() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Disconnected, "Verse");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: LV1 is disconnected");
    }

    #[test]
    fn validate_recall_scene_request_blocks_scene_identity_mismatch() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Different");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: scene identity mismatch");
    }

    #[test]
    fn validate_recall_scene_request_returns_matching_lv1_scene_index() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Verse");

        let result = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap();

        assert_eq!(result.scene.scene_id, "1::Verse");
        assert_eq!(result.lv1_scene_index, 1);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p advanced-show-control show::commands`

Expected: FAIL because `RecallSceneResult` and `validate_recall_scene_request` are not defined.

- [ ] **Step 3: Implement minimal show-owned validation**

Add `ConnectionStatus` and `Lv1StateSnapshot` imports at the top of `src-tauri/src/show/commands.rs`:

```rust
use crate::lv1::types::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot};
```

Replace the existing `use crate::lv1::types::ChannelInfo;` line with that import.

Add this result type near the other command result structs:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RecallSceneResult {
    pub scene: SceneConfig,
    pub lv1_scene_index: i32,
}
```

Add this function after `select_scene_config`:

```rust
pub fn validate_recall_scene_request(
    show: &super::types::ShowSnapshot,
    lv1: &Lv1StateSnapshot,
    scene_id: &str,
) -> Result<RecallSceneResult, String> {
    if show.lockout {
        return Err("Recall blocked: lockout is enabled".to_string());
    }

    let scene = show
        .scene_configs
        .iter()
        .find(|scene| scene.scene_id == scene_id)
        .cloned()
        .ok_or_else(|| "Scene config not found".to_string())?;

    if lv1.connection != ConnectionStatus::Connected {
        return Err("Recall blocked: LV1 is disconnected".to_string());
    }

    let lv1_scene = lv1
        .scene_list
        .iter()
        .find(|candidate| {
            candidate.index == scene.scene_index && candidate.name == scene.scene_name
        })
        .ok_or_else(|| "Recall blocked: scene identity mismatch".to_string())?;

    Ok(RecallSceneResult {
        scene,
        lv1_scene_index: lv1_scene.index,
    })
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo nextest run -p advanced-show-control show::commands`

Expected: PASS for the new validation tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/show/commands.rs
git commit -m "refactor: add show-owned recall validation"
```

## Task 2: Command-Bus Recall By Scene Id

**Files:**
- Modify: `src-tauri/src/runtime/commands.rs`
- Test: `src-tauri/src/runtime/commands.rs`

**Interfaces:**
- Consumes: `show::commands::validate_recall_scene_request(&ShowSnapshot, &Lv1StateSnapshot, &str)` from Task 1.
- Produces: `pub async fn AppCommandBus::recall_scene_by_id(&self, scene_id: String) -> Result<RecallSceneResult, AppCommandError>`.
- Keeps: `pub async fn AppCommandBus::recall_scene(&self, scene_index: i32) -> Result<(), AppCommandError>` as the low-level LV1 index command for existing fade/recall internals.

- [ ] **Step 1: Write failing command-bus tests**

Add these tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/src/runtime/commands.rs`, near `recall_scene_sends_index_to_lv1_actor`.

```rust
    async fn bus_with_recall_show_and_lv1(
        lockout: bool,
        lv1_connection: crate::lv1::types::ConnectionStatus,
        lv1_name: &str,
        recall_reply: Result<(), crate::lv1::events::Lv1ActorError>,
    ) -> (AppCommandBus, tokio::sync::mpsc::Receiver<i32>) {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);
        show.replace_snapshot(crate::show::types::ShowSnapshot {
            lockout,
            scene_configs: vec![crate::show::types::SceneConfig {
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
        bus.set_show(Some(show)).await;

        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(2);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        let lv1 = Lv1StateSnapshot {
            connection: lv1_connection,
            scene: None,
            scene_list: vec![crate::lv1::types::SceneListEntry {
                index: 1,
                name: lv1_name.to_string(),
            }],
            channels: Vec::new(),
        };

        let (recall_tx, recall_rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let mut recall_reply = Some(recall_reply);
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::commands::Lv1Command::GetState { reply } => {
                        let _ = reply.send(Ok(lv1.clone()));
                    }
                    crate::lv1::commands::Lv1Command::RecallScene { scene_index, reply } => {
                        let _ = recall_tx.send(scene_index).await;
                        let _ = reply.send(recall_reply.take().unwrap_or(Ok(())));
                    }
                    other => panic!("unexpected LV1 command: {other:?}"),
                }
            }
        });

        (bus, recall_rx)
    }

    #[tokio::test]
    async fn recall_scene_by_id_validates_show_and_sends_matching_lv1_index() {
        let (bus, mut recall_rx) = bus_with_recall_show_and_lv1(
            false,
            crate::lv1::types::ConnectionStatus::Connected,
            "Verse",
            Ok(()),
        )
        .await;

        let result = bus.recall_scene_by_id("1::Verse".to_string()).await.unwrap();

        assert_eq!(recall_rx.recv().await, Some(1));
        assert_eq!(result.scene.scene_id, "1::Verse");
        assert_eq!(result.lv1_scene_index, 1);
    }

    #[tokio::test]
    async fn recall_scene_by_id_blocks_lockout_before_sending_to_lv1() {
        let (bus, mut recall_rx) = bus_with_recall_show_and_lv1(
            true,
            crate::lv1::types::ConnectionStatus::Connected,
            "Verse",
            Ok(()),
        )
        .await;

        let err = bus
            .recall_scene_by_id("1::Verse".to_string())
            .await
            .unwrap_err();

        assert_eq!(err, AppCommandError::CommandFailed("Recall blocked: lockout is enabled".to_string()));
        assert!(recall_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn recall_scene_by_id_blocks_identity_mismatch_before_sending_to_lv1() {
        let (bus, mut recall_rx) = bus_with_recall_show_and_lv1(
            false,
            crate::lv1::types::ConnectionStatus::Connected,
            "Different",
            Ok(()),
        )
        .await;

        let err = bus
            .recall_scene_by_id("1::Verse".to_string())
            .await
            .unwrap_err();

        assert_eq!(err, AppCommandError::CommandFailed("Recall blocked: scene identity mismatch".to_string()));
        assert!(recall_rx.try_recv().is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo nextest run -p advanced-show-control runtime::commands::tests::recall_scene_by_id`

Expected: FAIL because `recall_scene_by_id` is not implemented.

- [ ] **Step 3: Implement `AppCommandBus::recall_scene_by_id`**

Update the show command imports at the top of `src-tauri/src/runtime/commands.rs`:

```rust
use crate::show::commands::{
    CueSceneResult, LoadShowFileResult, NewShowFileResult, RecallSceneResult,
    SelectedSceneResult, ShowCommandResult,
};
```

Add this method before the existing low-level `recall_scene(&self, scene_index: i32)` method:

```rust
pub async fn recall_scene_by_id(
    &self,
    scene_id: String,
) -> Result<RecallSceneResult, AppCommandError> {
    let show = self.show_target().await?;
    let lv1 = self.get_lv1_state().await.map_err(|error| match error {
        AppCommandError::Lv1Unavailable => {
            AppCommandError::CommandFailed("Recall blocked: LV1 state is unavailable".to_string())
        }
        other => other,
    })?;
    let show_snapshot = show.get_snapshot().await;
    let result = crate::show::commands::validate_recall_scene_request(
        &show_snapshot,
        &lv1,
        &scene_id,
    )
    .map_err(AppCommandError::CommandFailed)?;

    self.recall_scene(result.lv1_scene_index).await?;
    Ok(result)
}
```

- [ ] **Step 4: Run targeted tests**

Run: `cargo nextest run -p advanced-show-control runtime::commands::tests::recall_scene`

Expected: PASS for both the new `recall_scene_by_id...` tests and the existing low-level `recall_scene_sends_index_to_lv1_actor` test.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/runtime/commands.rs
git commit -m "refactor: route ui recall through command bus"
```

## Task 3: Thin Tauri Recall Adapter

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Test: `src-tauri/src/commands.rs`

**Interfaces:**
- Consumes: `AppCommandBus::recall_scene_by_id(scene_id: String) -> Result<RecallSceneResult, AppCommandError>` from Task 2.
- Produces: Tauri `recall_scene` command behavior with unchanged signature and unchanged `AppViewState` return for this transition phase.

- [ ] **Step 1: Update characterization tests for adapter routing**

Replace the two existing tests `recall_scene_blocks_when_lockout_is_enabled` and `recall_scene_blocks_without_lv1_state` in `src-tauri/src/commands.rs` with tests that install `ShowStateHandle` through `AppCommandBus`, because Tauri should no longer inspect `state.show` directly for validation.

```rust
    async fn lifecycle_with_recall_show(state: &ShellState) -> AppLifecycle {
        let lifecycle = AppLifecycle::default();
        let bus = AppCommandBus::new();
        bus.set_show(Some(state.show.clone())).await;
        lifecycle.set_command_bus(Some(bus)).await;
        lifecycle
    }

    #[tokio::test]
    async fn recall_scene_routes_lockout_block_through_command_bus() {
        let state = recall_state_with_unstored_scene(true).await;
        let lifecycle = lifecycle_with_recall_show(&state).await;

        let err = recall_scene_snapshot(
            state,
            lifecycle.command_bus_holder(),
            "1::Verse".to_string(),
        )
        .await
        .unwrap_err();

        assert_eq!(err, "Recall blocked: lockout is enabled");
    }

    #[tokio::test]
    async fn recall_scene_routes_missing_lv1_state_through_command_bus() {
        let state = recall_state_with_unstored_scene(false).await;
        let lifecycle = lifecycle_with_recall_show(&state).await;

        let err = recall_scene_snapshot(
            state,
            lifecycle.command_bus_holder(),
            "1::Verse".to_string(),
        )
        .await
        .unwrap_err();

        assert_eq!(err, "Recall blocked: LV1 state is unavailable");
    }
```

- [ ] **Step 2: Run tests to verify current adapter still fails the new routing expectation if command bus is not complete**

Run: `cargo nextest run -p advanced-show-control commands::tests::recall_scene_routes`

Expected before implementation: FAIL if `recall_scene_snapshot` has not been changed to use `recall_scene_by_id`; PASS after Task 2 may still be impossible until Step 3 because old code reaches directly into `state.show` and maps missing command bus differently.

- [ ] **Step 3: Replace Tauri-owned validation with command-bus call**

In `src-tauri/src/commands.rs`, replace the body of `recall_scene_snapshot` after the initial `tracing::debug!` block with this:

```rust
    let command_bus = current_command_bus(active_command_bus, "recall_scene").await?;
    let result = command_bus
        .recall_scene_by_id(scene_id.clone())
        .await
        .map_err(|error| {
            let message = map_app_command_error(error);
            tracing::warn!(
                event = "scene_recall_blocked",
                scene_id = %scene_id,
                reason = %message,
                "Scene recall blocked: {message}"
            );
            message
        })?;

    tracing::debug!(
        event = "scene_recall_command_sent",
        scene_id = %result.scene.scene_id,
        scene_index = result.scene.scene_index,
        scene_name = %result.scene.scene_name,
        "Scene recall command sent: {}",
        result.scene.scene_name
    );

    Ok(state.snapshot().await)
```

The resulting function must not call `state.show.get_snapshot()`, inspect `show.lockout`, search `show.scene_configs`, call `command_bus.get_lv1_state()`, inspect `lv1.connection`, search `lv1.scene_list`, or call `command_bus.recall_scene(...)` directly.

- [ ] **Step 4: Run targeted adapter tests**

Run: `cargo nextest run -p advanced-show-control commands::tests::recall_scene`

Expected: PASS for recall adapter tests.

- [ ] **Step 5: Search for forbidden phase-11 logic in Tauri adapter**

Run: `rg "show\.lockout|scene identity mismatch|command_bus\.get_lv1_state\(\)|command_bus\.recall_scene\(" src-tauri/src/commands.rs`

Expected: no matches in `recall_scene_snapshot`. Matches elsewhere must be reviewed; do not remove unrelated logic.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands.rs
git commit -m "refactor: make tauri recall command a thin adapter"
```

## Task 4: Regression Coverage And Docs

**Files:**
- Modify: `src-tauri/src/show/commands.rs`
- Modify: `src-tauri/src/runtime/commands.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `docs/architecture.md`

**Interfaces:**
- Consumes: completed Tasks 1-3.
- Produces: documented phase-11 boundary and verification evidence.

- [ ] **Step 1: Add a command-bus failure test for LV1 recall command failure mapping**

In `src-tauri/src/runtime/commands.rs`, add a test beside the other `recall_scene_by_id` tests. Use the same helper style from Task 2, but make the LV1 mock reply to `RecallScene` with `Err(crate::lv1::events::Lv1ActorError::Disconnected)`.

```rust
    #[tokio::test]
    async fn recall_scene_by_id_returns_lv1_recall_failure_without_show_mutation() {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);
        show.replace_snapshot(crate::show::types::ShowSnapshot {
            lockout: false,
            scene_configs: vec![crate::show::types::SceneConfig {
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
        bus.set_show(Some(show)).await;

        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(2);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::commands::Lv1Command::GetState { reply } => {
                        let _ = reply.send(Ok(Lv1StateSnapshot {
                            connection: crate::lv1::types::ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![crate::lv1::types::SceneListEntry {
                                index: 1,
                                name: "Verse".to_string(),
                            }],
                            channels: Vec::new(),
                        }));
                    }
                    crate::lv1::commands::Lv1Command::RecallScene { reply, .. } => {
                        let _ = reply.send(Err(crate::lv1::events::Lv1ActorError::Disconnected));
                    }
                    other => panic!("unexpected LV1 command: {other:?}"),
                }
            }
        });

        let err = bus
            .recall_scene_by_id("1::Verse".to_string())
            .await
            .unwrap_err();

        assert_eq!(err, AppCommandError::Lv1Unavailable);
    }
```

- [ ] **Step 2: Run targeted regression tests**

Run: `cargo nextest run -p advanced-show-control show::commands runtime::commands::tests::recall_scene commands::tests::recall_scene`

Expected: PASS.

- [ ] **Step 3: Update architecture docs**

In `docs/architecture.md`, update the pending-work sentences on lines currently saying UI-requested recall is pending. Replace them with text equivalent to:

```markdown
Low-risk show/app mutations, show-file import/export mapping, and UI-requested recall validation/dispatch route through `AppCommandBus`. The `show/` module owns show-file DTOs, schema version, import/export mapping, pruning, validation against LV1 scene snapshots, and the UI-requested recall use case. The Tauri adapter still owns native dialogs and filesystem read/write plumbing, and it still returns/directly emits `AppViewState` snapshots until the projector-only and frontend command-contract phases remove that temporary behavior.

Projector cache, logging projection, React command-result cleanup, `ShellState` removal, and `ActiveCommandBus` removal are still pending later phases.
```

Apply equivalent updates to repeated transitional-summary paragraphs in `docs/architecture.md` without changing unrelated architecture text.

- [ ] **Step 4: Run final verification for the phase**

Run the smallest full-backend verification plus frontend typecheck because command wiring affects Tauri command exports:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run -p advanced-show-control show::commands runtime::commands::tests::recall_scene commands::tests::recall_scene
npm --prefix ui run typecheck
```

Expected: all commands pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/show/commands.rs src-tauri/src/runtime/commands.rs src-tauri/src/commands.rs docs/architecture.md
git commit -m "docs: describe show-owned ui recall boundary"
```

## Task 5: Final Phase Review

**Files:**
- Inspect: `src-tauri/src/show/commands.rs`
- Inspect: `src-tauri/src/runtime/commands.rs`
- Inspect: `src-tauri/src/commands.rs`
- Inspect: `docs/architecture.md`

**Interfaces:**
- Consumes: Tasks 1-4.
- Produces: verified phase-11 completion ready for merge.

- [ ] **Step 1: Check final diff**

Run: `git diff --stat main...HEAD && git diff main...HEAD -- src-tauri/src/show/commands.rs src-tauri/src/runtime/commands.rs src-tauri/src/commands.rs docs/architecture.md`

Expected: changes are limited to show-owned recall validation, command-bus routing, Tauri adapter thinning, tests, and docs.

- [ ] **Step 2: Check guardrails with search**

Run: `rg "state\.show\.get_snapshot\(\)|show\.lockout|scene identity mismatch|command_bus\.get_lv1_state\(\)|command_bus\.recall_scene\(" src-tauri/src/commands.rs`

Expected: no phase-11 recall validation logic remains in `recall_scene_snapshot`. If matches appear outside `recall_scene_snapshot`, inspect before changing; do not remove unrelated code.

- [ ] **Step 3: Run broad Rust verification**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo build --workspace
cargo build -p advanced-show-control --bin lv1-probe
```

Expected: all commands pass.

- [ ] **Step 4: Run frontend/Tauri verification**

```bash
npm --prefix ui run typecheck
npm run tauri -- build
```

Expected: both commands pass. The existing non-fatal Tauri warning about bundle identifier ending with `.app` may still appear.

- [ ] **Step 5: Commit any verification-only doc/test fixups**

If Step 3 or Step 4 required fixes, commit them with a focused message:

```bash
git add <changed-files>
git commit -m "fix: complete show-owned ui recall routing"
```

If no fixes were needed, do not create an empty commit.

---

## Self-Review Notes

- Spec coverage: This plan implements migration phase 11 only. It moves UI-requested recall lockout, scene config lookup, connected check, exact LV1 scene identity validation, and LV1 recall dispatch behind `AppCommandBus` into show-owned command handling. It intentionally does not implement projector cache, logging projection, projector-only emission, React command-result cleanup, `ShellState` removal, or `ActiveCommandBus` removal.
- Placeholder scan: No `TBD`, `TODO`, or unspecified edge-case steps remain. Each task has concrete files, functions, tests, commands, and expected outcomes.
- Type consistency: The plan consistently uses `RecallSceneResult`, `validate_recall_scene_request`, and `AppCommandBus::recall_scene_by_id(scene_id: String)` across tasks.
