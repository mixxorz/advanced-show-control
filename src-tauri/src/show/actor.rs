use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::lv1::{Lv1ActorError, Lv1ActorHandle, Lv1Command, Lv1StateSnapshot};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus};
use crate::show_file::{backup_folder, read_show_file, write_show_file};

use super::commands::ShowCommand;
use super::events::{ShowEvent, ShowProjectionReason};
use super::handle::ShowStateHandle;
use super::show_file::import_show_file;
use super::state::ShowState;
use super::{LoadShowFileResult, NewShowFileResult, ShowCommandResult};

#[derive(Clone, Default)]
pub struct ShowActorPeers {
    lv1: Arc<Mutex<Option<(u64, Lv1ActorHandle)>>>,
}

impl ShowActorPeers {
    pub fn set_lv1(&self, generation: u64, lv1: Lv1ActorHandle) {
        *self.lv1.lock().expect("show peer lock poisoned") = Some((generation, lv1));
    }

    pub fn clear_lv1(&self, generation: u64) {
        let mut lv1 = self.lv1.lock().expect("show peer lock poisoned");
        if lv1
            .as_ref()
            .is_some_and(|(peer_generation, _)| *peer_generation == generation)
        {
            *lv1 = None;
        }
    }

    fn lv1(&self) -> Option<Lv1ActorHandle> {
        self.lv1
            .lock()
            .expect("show peer lock poisoned")
            .as_ref()
            .map(|(_, lv1)| lv1.clone())
    }
}

pub struct ShowActorTask {
    rx: mpsc::Receiver<ShowCommand>,
    event_bus: AppEventBus,
    peers: ShowActorPeers,
}

impl ShowActorTask {
    pub fn spawn(self) {
        tauri::async_runtime::spawn(run_show_actor(self.rx, self.event_bus, self.peers));
    }
}

pub fn build_show_actor(
    event_bus: AppEventBus,
) -> (ShowStateHandle, ShowActorTask, ShowActorPeers) {
    let (tx, rx) = mpsc::channel(32);
    let peers = ShowActorPeers::default();
    let task = ShowActorTask {
        rx,
        event_bus,
        peers: peers.clone(),
    };
    (ShowStateHandle::new(tx), task, peers)
}

async fn run_show_actor(
    mut rx: mpsc::Receiver<ShowCommand>,
    event_bus: AppEventBus,
    peers: ShowActorPeers,
) {
    let mut state = ShowState::default();
    while let Some(command) = rx.recv().await {
        handle_command(command, &mut state, &event_bus, &peers).await;
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

async fn handle_command(
    command: ShowCommand,
    state: &mut ShowState,
    event_bus: &AppEventBus,
    peers: &ShowActorPeers,
) {
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
        ShowCommand::NewShowFileFromCurrentLv1 { reply } => {
            let lv1 = current_lv1_snapshot(peers).await.ok();
            let selected_scene_id = state.reset_for_new_show(lv1.as_ref());
            publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
            tracing::info!(event = "show_file_created", "New show file created");
            if let Some(reply) = reply {
                let _ = reply.send(Ok(NewShowFileResult { selected_scene_id }));
            }
        }
        ShowCommand::SaveShowFileAs { path, reply } => {
            let result = save_show_file_to_path(state, event_bus, path);
            if let Some(reply) = reply {
                let _ = reply.send(result);
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
        ShowCommand::RefreshLv1Discovery { timeout_ms, reply } => {
            let result = refresh_lv1_discovery(state, event_bus, timeout_ms);
            if let Some(reply) = reply {
                let _ = reply.send(result);
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
        ShowCommand::LoadShowFileFromPath { path, reply } => {
            let result = current_lv1_snapshot(peers).await.and_then(|lv1| {
                let mut file = read_show_file(&path)?;
                load_show_file_from_dto(state, event_bus, path, &mut file, &lv1)
            });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::StoreSceneConfigFromCurrentLv1 { scene_id, reply } => {
            let result = current_lv1_snapshot(peers)
                .await
                .map_err(|_| "Store scene blocked: LV1 state is unavailable".to_string())
                .and_then(|lv1| {
                    store_scene_config_from_channels(state, event_bus, &scene_id, lv1.channels)
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

fn refresh_lv1_discovery(
    state: &mut ShowState,
    event_bus: &AppEventBus,
    timeout_ms: Option<u64>,
) -> Result<ShowCommandResult, String> {
    let systems = crate::lv1::discover(crate::lv1::DiscoverOptions {
        timeout: std::time::Duration::from_millis(timeout_ms.unwrap_or(1000).clamp(100, 6000)),
        ..Default::default()
    })
    .map_err(|err| format!("Failed to discover LV1 systems: {err}"))?
    .iter()
    .filter_map(crate::connection_state::identity_from_discovery)
    .map(|identity| crate::connection_state::DiscoveredLv1System {
        identity,
        latency_ms: None,
        status: crate::connection_state::DiscoveredLv1Status::Available,
    })
    .collect();
    let changed = state.set_discovered_lv1_systems(systems);
    publish_if_changed(
        event_bus,
        ShowProjectionReason::ConnectionMetadata,
        state,
        changed,
    );
    Ok(ShowCommandResult { changed })
}

async fn current_lv1_snapshot(peers: &ShowActorPeers) -> Result<Lv1StateSnapshot, String> {
    let lv1 = peers
        .lv1()
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let (reply, rx) = tokio::sync::oneshot::channel();
    lv1.send(Lv1Command::GetState { reply })
        .await
        .map_err(|error| match error {
            Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
            other => AppCommandError::CommandFailed(other.to_string()),
        })
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)
}

fn map_app_command_error(error: AppCommandError) -> String {
    match error {
        AppCommandError::CommandFailed(message) => message,
        other => other.to_string(),
    }
}

fn save_show_file_to_path(
    state: &mut ShowState,
    event_bus: &AppEventBus,
    path: std::path::PathBuf,
) -> Result<ShowCommandResult, String> {
    let saved_at = crate::time::current_timestamp_millis();
    let file = state.export_show_file(saved_at.clone());
    write_show_file(&path, &file, &backup_folder())?;
    state.mark_saved(path, saved_at);
    publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
    tracing::info!(event = "show_file_saved", "Show file saved");
    Ok(ShowCommandResult { changed: true })
}

fn load_show_file_from_dto(
    state: &mut ShowState,
    event_bus: &AppEventBus,
    path: std::path::PathBuf,
    file: &mut super::show_file::ShowFile,
    lv1: &Lv1StateSnapshot,
) -> Result<LoadShowFileResult, String> {
    let imported = import_show_file(file, lv1)?;
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
    Ok(LoadShowFileResult {
        selected_scene_id,
        saved_at,
        report,
    })
}

fn store_scene_config_from_channels(
    state: &mut ShowState,
    event_bus: &AppEventBus,
    scene_id: &str,
    channels: Vec<crate::lv1::ChannelInfo>,
) -> Result<ShowCommandResult, String> {
    if state.get_scene_config(scene_id).is_none() {
        return Err("Scene config not found".to_string());
    }
    state
        .store_scene_config(scene_id, &channels)
        .map(|changed| {
            publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
            ShowCommandResult { changed }
        })
}
