# Scenes Actor Command Dispatch Consolidation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete issue #59 by giving every `ScenesCommand` one production implementation while preserving both actor-loop states.

**Architecture:** Extract one private asynchronous dispatcher that owns the exhaustive command match and returns an explicit continue/shutdown disposition. Keep mailbox receive, closed-mailbox handling, events, pending deadlines, settings refresh, and `tokio::select!` ordering in `run_scenes_actor`.

**Tech Stack:** Rust 2024, Tokio actors, `AppEventBus`, cargo-nextest

## Global Constraints

- Execute after issue #53 and before issue #58.
- Preserve fresh LV1 state validation, lockout, exact identity checks, generation guards, pending-scene settle timing, blocked/skipped behavior, replies, logs, and projection flags.
- Do not add actor-handle helpers, generic mailbox wrappers, commands, public interfaces, or source-text tests.
- Acquire LV1/fade peers only inside commands that currently need them.
- Before advancing to issue #58, run one successful `make smoke` and inspect `logs/debug-smoke-report.txt`.

---

### Task 1: Extract The Single Command Dispatcher

**Files:**
- Modify: `src-tauri/src/scenes/actor.rs:135-358`
- Test: `src-tauri/src/scenes/actor.rs:1108-1536`

**Interfaces:**
- Consumes: one `ScenesCommand`, mutable `ScenesState`, `ScenesPeers`, `AppEventBus`, and runtime generation number.
- Produces: private `ScenesCommandDispatch::{Continue, Shutdown}` from `dispatch_scenes_command(...)`.

- [ ] **Step 1: Run behavior characterization before extraction**

Run the tests that exercise normal commands, pending-settle commands, and shutdown:

```bash
cargo nextest run -p advanced-show-control scenes::actor::tests::store_scene_config_from_current_lv1_publishes_state_change
cargo nextest run -p advanced-show-control scenes::actor::tests::select_scene_config_publish_persisted_scene_edits
cargo nextest run -p advanced-show-control scenes::actor::tests::copy_and_paste_scene_settings_publish_only_changed_projection_state
cargo nextest run -p advanced-show-control scenes::actor::tests::scene_recall_handle_sends_shutdown_command
```

Expected: all pass before the structural refactor. The copy/paste test enters the 25 ms pending-settle state, sends commands there, and sends shutdown while that state remains active.

- [ ] **Step 2: Add the private disposition and complete dispatcher**

Add immediately after `run_scenes_actor`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenesCommandDispatch {
    Continue,
    Shutdown,
}

async fn dispatch_scenes_command(
    command: ScenesCommand,
    recall_state: &mut ScenesState,
    peers: &ScenesPeers,
    event_bus: &AppEventBus,
    generation: u64,
) -> ScenesCommandDispatch {
    match command {
        ScenesCommand::GetSceneDocument { reply } => {
            let _ = reply.send(recall_state.snapshot());
        }
        ScenesCommand::GetSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let _ = reply.send(recall_state.get_scene_config(internal_scene_id));
        }
        ScenesCommand::InitialProjectionState { reply } => {
            let _ = reply.send(recall_state.projection_state());
        }
        ScenesCommand::SetSceneDuration {
            internal_scene_id,
            duration_ms,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
                true,
                |state| state.set_scene_duration_ms(internal_scene_id, duration_ms),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SetSceneScopeFadersEnabled {
            internal_scene_id,
            enabled,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
                true,
                |state| state.set_scene_scope_faders_enabled(internal_scene_id, enabled),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SetSceneScopePanEnabled {
            internal_scene_id,
            enabled,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
                true,
                |state| state.set_scene_scope_pan_enabled(internal_scene_id, enabled),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::LinkSceneConfig {
            source_internal_scene_id,
            target_scene_index,
            overwrite_existing,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
                true,
                |state| {
                    state.link_scene_config_by_index(
                        source_internal_scene_id,
                        target_scene_index,
                        overwrite_existing,
                    )
                },
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::DeleteSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
                true,
                |state| state.delete_scene_config(internal_scene_id),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SetChannelScoped {
            internal_scene_id,
            group,
            channel,
            scoped,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
                true,
                |state| state.set_channel_scoped(internal_scene_id, group, channel, scoped),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SetAllChannelsScoped {
            internal_scene_id,
            scoped,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
                true,
                |state| state.set_all_channels_scoped(internal_scene_id, scoped),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::SelectSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let result = recall_state.select_scene_config(internal_scene_id).map(|changed| {
                if changed {
                    publish_scene_state_changed(
                        event_bus,
                        generation,
                        ScenesProjectionReason::SceneState,
                        recall_state,
                        true,
                    );
                }
                SelectedSceneResult {
                    scene: recall_state.get_scene_config(internal_scene_id).unwrap(),
                }
            });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::CopySceneSettings {
            source_internal_scene_id,
            reply,
        } => {
            let result = copy_scene_settings(
                recall_state,
                source_internal_scene_id,
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::PasteSceneSettings {
            destination_internal_scene_id,
            reply,
        } => {
            let result = mutate_scene_state(
                recall_state,
                ScenesProjectionReason::SceneState,
                true,
                |state| state.paste_scene_settings(destination_internal_scene_id),
                event_bus,
                generation,
            );
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::StoreSceneConfigFromCurrentLv1 {
            internal_scene_id,
            reply,
        } => {
            let peer_handles = peers.handles();
            let result = store_scene_config_from_current_lv1(
                &peer_handles.lv1,
                event_bus,
                generation,
                recall_state,
                internal_scene_id,
            )
            .await;
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ScenesCommand::ReplaceSceneDocument {
            document,
            reason,
            persisted_scene_edit,
            reply,
        } => {
            recall_state.replace_snapshot_for_session(document);
            publish_scene_state_changed(
                event_bus,
                generation,
                reason,
                recall_state,
                persisted_scene_edit,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ScenesCommandResult { changed: true });
            }
        }
        ScenesCommand::RecallScene {
            internal_scene_id,
            reply,
        } => {
            let peer_handles = peers.handles();
            let scene_document = recall_state.snapshot();
            let lockout = recall_state.lockout();
            let _ = reply.send(
                handle_explicit_recall_scene(
                    lockout,
                    &peer_handles.lv1,
                    &scene_document,
                    internal_scene_id,
                )
                .await,
            );
        }
        ScenesCommand::Shutdown => return ScenesCommandDispatch::Shutdown,
    }
    ScenesCommandDispatch::Continue
}
```

- [ ] **Step 3: Delegate both actor-loop mailbox branches**

Replace each duplicated command match, without moving its `tokio::select!` arm, with:

```rust
command = command_rx.recv() => {
    let Some(command) = command else {
        break;
    };
    if dispatch_scenes_command(
        command,
        &mut recall_state,
        &peers,
        &event_bus,
        generation,
    )
    .await
        == ScenesCommandDispatch::Shutdown
    {
        break;
    }
}
```

Keep the pending branch's unconditional `continue` after its `tokio::select!`. Do not move event arms, the pending deadline, settings refresh, or scene-observation processing.

- [ ] **Step 4: Run focused and full scenes verification**

Run:

```bash
cargo nextest run -p advanced-show-control scenes::actor::tests::store_scene_config_from_current_lv1_publishes_state_change
cargo nextest run -p advanced-show-control scenes::actor::tests::select_scene_config_publish_persisted_scene_edits
cargo nextest run -p advanced-show-control scenes::actor::tests::copy_and_paste_scene_settings_publish_only_changed_projection_state
cargo nextest run -p advanced-show-control scenes::actor::tests::scene_recall_handle_sends_shutdown_command
cargo nextest run -p advanced-show-control scenes
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

Expected: all behavior remains green and the compiler enforces one exhaustive command match.

- [ ] **Step 5: Run the issue smoke checkpoint**

Run `make smoke`, then read `logs/debug-smoke-report.txt`. Expected: authoritative suite success; diagnose, fix, and rerun only after a failure.

- [ ] **Step 6: Commit the verified issue**

```bash
git status --short
git diff -- src-tauri/src/scenes/actor.rs
git add src-tauri/src/scenes/actor.rs
git commit -m "refactor: consolidate scenes command dispatch"
```
