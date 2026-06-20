use tokio::sync::mpsc;

use crate::runtime::events::{AppEvent, AppEventBus};

use super::commands::ShowCommand;
use super::events::{ShowEvent, ShowProjectionReason};
use super::handle::ShowStateHandle;
use super::show_file::import_show_file;
use super::state::ShowState;
use super::{LoadShowFileResult, NewShowFileResult, ShowCommandResult};

pub fn spawn_show_actor(event_bus: AppEventBus) -> ShowStateHandle {
    let (tx, rx) = mpsc::channel(32);
    tauri::async_runtime::spawn(run_show_actor(rx, event_bus));
    ShowStateHandle::new(tx)
}

async fn run_show_actor(mut rx: mpsc::Receiver<ShowCommand>, event_bus: AppEventBus) {
    let mut state = ShowState::default();
    while let Some(command) = rx.recv().await {
        handle_command(command, &mut state, &event_bus);
    }
}

fn publish_state_changed(event_bus: &AppEventBus, reason: ShowProjectionReason, state: &ShowState) {
    event_bus.publish(AppEvent::Show(ShowEvent::StateChanged {
        reason,
        state: state.projection_state(),
    }));
}

fn publish_if_changed(
    event_bus: &AppEventBus,
    reason: ShowProjectionReason,
    state: &ShowState,
    changed: bool,
) {
    if changed {
        publish_state_changed(event_bus, reason, state);
    }
}

fn handle_command(command: ShowCommand, state: &mut ShowState, event_bus: &AppEventBus) {
    match command {
        ShowCommand::GetShowDocument { reply } => {
            let _ = reply.send(state.snapshot());
        }
        ShowCommand::CurrentShowFilePath { reply } => {
            let _ = reply.send(state.current_show_file_path());
        }
        ShowCommand::GetLockout { reply } => {
            let _ = reply.send(state.lockout());
        }
        ShowCommand::GetSceneConfig { scene_id, reply } => {
            let _ = reply.send(state.get_scene_config(&scene_id));
        }
        ShowCommand::InitialProjectionState { reply } => {
            let _ = reply.send(state.projection_state());
        }
        ShowCommand::SetLockout { enabled, reply } => {
            let changed = state.set_lockout(enabled);
            publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::SetSceneDuration {
            scene_id,
            duration_ms,
            reply,
        } => {
            let result = state
                .set_scene_duration_ms(&scene_id, duration_ms)
                .map(|changed| {
                    if changed {
                        state.mark_dirty();
                    }
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SetSceneScopeFadersEnabled {
            scene_id,
            enabled,
            reply,
        } => {
            let result = state
                .set_scene_scope_faders_enabled(&scene_id, enabled)
                .map(|changed| {
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SetSceneScopePanEnabled {
            scene_id,
            enabled,
            reply,
        } => {
            let result = state
                .set_scene_scope_pan_enabled(&scene_id, enabled)
                .map(|changed| {
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SetChannelScoped {
            scene_id,
            group,
            channel,
            scoped,
            reply,
        } => {
            let result = state
                .set_channel_scoped(&scene_id, group, channel, scoped)
                .map(|changed| {
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SetAllChannelsScoped {
            scene_id,
            scoped,
            reply,
        } => {
            let result = state
                .set_all_channels_scoped(&scene_id, scoped)
                .map(|changed| {
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::StoreSceneConfig {
            scene_id,
            channels,
            reply,
        } => {
            let result = if state.get_scene_config(&scene_id).is_none() {
                Err("Scene config not found".to_string())
            } else {
                state
                    .store_scene_config(&scene_id, &channels)
                    .map(|changed| {
                        publish_if_changed(
                            event_bus,
                            ShowProjectionReason::ShowState,
                            state,
                            changed,
                        );
                        ShowCommandResult { changed }
                    })
            };
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::CueScene { scene_id, reply } => {
            tracing::debug!(event = "scene_cue_requested", scene_id = %scene_id, "Scene cue requested");
            let result = state
                .get_scene_config(&scene_id)
                .ok_or_else(|| {
                    tracing::warn!(event = "scene_cue_blocked", scene_id = %scene_id, reason = "scene config not found", "Scene cue blocked: scene config not found");
                    "Scene config not found".to_string()
                })
                .and_then(|scene| {
                    let changed = state.cue_scene(&scene_id)?;
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    tracing::info!(event = "scene_cued", scene_id = %scene.scene_id, scene_index = scene.scene_index, scene_name = %scene.scene_name, "Scene cued: {}", scene.scene_name);
                    Ok(super::commands::CueSceneResult { changed, scene })
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SelectSceneConfig { scene_id, reply } => {
            let result = state
                .get_scene_config(&scene_id)
                .ok_or_else(|| "Scene config not found".to_string())
                .map(|scene| {
                    let changed = state.set_selected_scene_id(Some(scene.scene_id.clone()));
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    super::commands::SelectedSceneResult { scene }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::NewShowFile { lv1, reply } => {
            let selected_scene_id = state.reset_for_new_show(lv1.as_ref());
            publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
            tracing::info!(event = "show_file_created", "New show file created");
            if let Some(reply) = reply {
                let _ = reply.send(Ok(NewShowFileResult { selected_scene_id }));
            }
        }
        ShowCommand::MarkShowFileSaved {
            path,
            saved_at,
            reply,
        } => {
            state.mark_saved(path, saved_at);
            publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
            tracing::info!(event = "show_file_saved", "Show file saved");
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed: true });
            }
        }
        ShowCommand::SetDiscoveredLv1Systems { systems, reply } => {
            let changed = state.set_discovered_lv1_systems(systems);
            publish_if_changed(
                event_bus,
                ShowProjectionReason::ConnectionMetadata,
                state,
                changed,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::SetPendingLv1Identity { identity, reply } => {
            let changed = state.set_pending_lv1_identity(identity);
            publish_if_changed(
                event_bus,
                ShowProjectionReason::ConnectionMetadata,
                state,
                changed,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::EstablishConnectedLv1Identity { identity, reply } => {
            let changed = state.establish_connected_lv1_identity(identity);
            publish_if_changed(
                event_bus,
                ShowProjectionReason::ConnectionMetadata,
                state,
                changed,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::ClearConnectedLv1Identity { reply } => {
            let changed = state.clear_connected_lv1_identity();
            publish_if_changed(
                event_bus,
                ShowProjectionReason::ConnectionMetadata,
                state,
                changed,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::SetReconnectState { reconnect, reply } => {
            let changed = state.set_reconnect_state(reconnect);
            publish_if_changed(
                event_bus,
                ShowProjectionReason::ConnectionMetadata,
                state,
                changed,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::HandleRuntimeDisconnected { reason, reply } => {
            let changed = state.handle_runtime_disconnected(reason);
            publish_if_changed(
                event_bus,
                ShowProjectionReason::ConnectionMetadata,
                state,
                changed,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::ExportShowFileSnapshot { saved_at, reply } => {
            let _ = reply.send(state.export_show_file(saved_at));
        }
        ShowCommand::LoadShowFileFromDto {
            path,
            mut file,
            lv1,
            reply,
        } => {
            let result = lv1
                .ok_or_else(|| "Open a show file after LV1 scenes are loaded".to_string())
                .and_then(|lv1| {
                    let imported = import_show_file(&mut file, &lv1)?;
                    let saved_at = file.saved_at.clone();
                    let selected_scene_id = imported.selected_scene_id.clone();
                    let report = imported.report.clone();
                    let should_mark_dirty = report.removed_anything();
                    state.replace_snapshot(imported.snapshot);
                    state.set_selected_scene_id(selected_scene_id.clone());
                    state.mark_saved(path, saved_at.clone());
                    if should_mark_dirty {
                        state.mark_dirty();
                    }
                    publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
                    for scene in report.removed_scenes.iter() {
                        tracing::warn!(event = "show_file_scene_pruned", scene = %scene, "Skipped loading \"{scene}\" because it was not found in the current scene list.");
                    }
                    tracing::info!(event = "show_file_opened", "Show file loaded");
                    Ok(LoadShowFileResult { selected_scene_id, saved_at, report })
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::ReconcileSceneList { scenes, reply } => {
            let _ = state.scene_reconciliation_diagnostic(&scenes);
            let changed = state.reconcile_scene_fade_configs(&scenes);
            publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
            if let Some(reply) = reply {
                let _ = reply.send(changed);
            }
        }
        #[cfg(test)]
        ShowCommand::ReplaceSnapshotForTest { snapshot, reply } => {
            let changed = state.snapshot() != snapshot;
            if changed {
                state.replace_snapshot(snapshot);
                publish_state_changed(event_bus, ShowProjectionReason::ShowState, state);
            }
            if let Some(reply) = reply {
                let _ = reply.send(());
            }
        }
        #[cfg(test)]
        ShowCommand::ClearForTest { reply } => {
            let changed = state.snapshot() != super::types::ShowDocument::empty();
            if changed {
                state.clear();
                publish_state_changed(event_bus, ShowProjectionReason::ShowState, state);
            }
            if let Some(reply) = reply {
                let _ = reply.send(());
            }
        }
    }
}
