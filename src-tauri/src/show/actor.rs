use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;

use crate::lv1::{Lv1ActorError, Lv1ActorHandle, Lv1Command, Lv1Event, Lv1StateSnapshot};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, RuntimeLifecycleEvent, log_lagged_subscriber};
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
    let mut events = event_bus.subscribe();
    let mut active_generation = 0;
    loop {
        tokio::select! {
            command = rx.recv() => {
                let Some(command) = command else { break; };
                handle_command(command, &mut state, &event_bus, &peers).await;
            }
            event = events.recv() => {
                match event {
                    Ok(event) => handle_app_event(event, &mut active_generation, &mut state, &event_bus),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("show-actor", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

fn handle_app_event(
    event: AppEvent,
    active_generation: &mut u64,
    state: &mut ShowState,
    event_bus: &AppEventBus,
) {
    match event {
        AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation }) => {
            *active_generation = generation;
        }
        AppEvent::Lv1 {
            generation,
            event: Lv1Event::Disconnected { reason },
        } if generation == *active_generation => {
            let changed = state.handle_runtime_disconnected(reason);
            publish_if_changed(
                event_bus,
                ShowProjectionReason::ConnectionMetadata,
                state,
                changed,
            );
        }
        AppEvent::Lv1 {
            generation,
            event: Lv1Event::SceneListChanged(scenes),
        } if generation == *active_generation => {
            let before = state.scene_configs().to_vec();
            let after = crate::show::scene_alignment::align_scene_configs(before.clone(), &scenes);
            let changed = state.replace_scene_configs_if_changed(after);
            if changed {
                tracing::debug!(
                    event = "session_scene_alignment",
                    "{}",
                    crate::show::scene_alignment::scene_alignment_diagnostic(
                        &before,
                        state.scene_configs(),
                        &scenes
                    )
                );
            }
            publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
        }
        _ => {}
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
        ShowCommand::GetSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let _ = reply.send(state.get_scene_config(internal_scene_id));
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
            internal_scene_id,
            duration_ms,
            reply,
        } => {
            let result = state
                .set_scene_duration_ms(internal_scene_id, duration_ms)
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
            internal_scene_id,
            enabled,
            reply,
        } => {
            let result = state
                .set_scene_scope_faders_enabled(internal_scene_id, enabled)
                .map(|changed| {
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SetSceneScopePanEnabled {
            internal_scene_id,
            enabled,
            reply,
        } => {
            let result = state
                .set_scene_scope_pan_enabled(internal_scene_id, enabled)
                .map(|changed| {
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SetChannelScoped {
            internal_scene_id,
            group,
            channel,
            scoped,
            reply,
        } => {
            let result = state
                .set_channel_scoped(internal_scene_id, group, channel, scoped)
                .map(|changed| {
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::LinkSceneConfig {
            source_internal_scene_id,
            target_scene_index,
            overwrite_existing,
            reply,
        } => {
            let result = current_lv1_snapshot(peers)
                .await
                .map_err(|_| "Link blocked: LV1 state is unavailable".to_string())
                .and_then(|lv1| {
                    let target = lv1
                        .scene_list
                        .iter()
                        .find(|scene| scene.index == target_scene_index)
                        .ok_or_else(|| "Link blocked: target scene not found".to_string())?;
                    state
                        .link_scene_config(source_internal_scene_id, target, overwrite_existing)
                        .map(|changed| {
                            if changed {
                                state.mark_dirty();
                            }
                            publish_if_changed(
                                event_bus,
                                ShowProjectionReason::ShowState,
                                state,
                                changed,
                            );
                            ShowCommandResult { changed }
                        })
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::DeleteSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let result = state.delete_scene_config(internal_scene_id).map(|changed| {
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
        ShowCommand::SetAllChannelsScoped {
            internal_scene_id,
            scoped,
            reply,
        } => {
            let result = state
                .set_all_channels_scoped(internal_scene_id, scoped)
                .map(|changed| {
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    ShowCommandResult { changed }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::CueScene {
            internal_scene_id,
            reply,
        } => {
            tracing::debug!(event = "scene_cue_requested", internal_scene_id = %internal_scene_id, "Scene cue requested");
            let result = state
                .get_scene_config(internal_scene_id)
                .ok_or_else(|| {
                    tracing::warn!(event = "scene_cue_blocked", internal_scene_id = %internal_scene_id, reason = "scene config not found", "Scene cue blocked: scene config not found");
                    "Scene config not found".to_string()
                })
                .and_then(|scene| {
                    if scene.scene_index.is_none() {
                        return Err("Cue blocked: scene is unlinked".to_string());
                    }
                    let changed = state.cue_scene(internal_scene_id)?;
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    tracing::info!(event = "scene_cued", internal_scene_id = %scene.internal_scene_id, scene_index = scene.scene_index, scene_name = %scene.scene_name, "Scene cued: {}", scene.scene_name);
                    Ok(super::commands::CueSceneResult { changed, scene })
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SelectSceneConfig {
            internal_scene_id,
            reply,
        } => {
            let result = state
                .get_scene_config(internal_scene_id)
                .ok_or_else(|| "Scene config not found".to_string())
                .map(|scene| {
                    let changed = state
                        .set_selected_scene_internal_id(Some(scene.internal_scene_id.to_string()));
                    publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
                    super::commands::SelectedSceneResult { scene }
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::NewShowFileFromCurrentLv1 { reply } => {
            let lv1 = current_lv1_snapshot(peers).await.ok();
            let selected_scene_internal_id = state.reset_for_new_show(lv1.as_ref());
            publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
            tracing::info!(event = "session_created", "New session created");
            if let Some(reply) = reply {
                let _ = reply.send(Ok(NewShowFileResult {
                    selected_scene_internal_id,
                }));
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
        ShowCommand::LoadShowFileFromPath { path, reply } => {
            let result = current_lv1_snapshot(peers).await.and_then(|lv1| {
                let mut file = read_show_file(&path)?;
                load_show_file_from_dto(state, event_bus, path, &mut file, &lv1)
            });
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::StoreSceneConfigFromCurrentLv1 {
            internal_scene_id,
            reply,
        } => {
            let result = current_lv1_snapshot(peers)
                .await
                .map_err(|_| "Store scene blocked: LV1 state is unavailable".to_string())
                .and_then(|lv1| {
                    store_scene_config_from_channels(
                        state,
                        event_bus,
                        internal_scene_id,
                        lv1.channels,
                    )
                });
            if let Some(reply) = reply {
                let _ = reply.send(result);
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
    tracing::info!(event = "session_saved", "Session saved");
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
    let selected_scene_internal_id = imported.selected_scene_internal_id.clone();
    let report = imported.report.clone();
    let imported_scene_configs = imported.snapshot.scene_configs;
    let aligned_scene_configs = crate::show::scene_alignment::align_scene_configs(
        imported_scene_configs.clone(),
        &lv1.scene_list,
    );
    let alignment_changed = aligned_scene_configs != imported_scene_configs;
    let should_mark_dirty =
        report.removed_anything() || imported.generated_internal_scene_ids || alignment_changed;
    let selected_scene_internal_id = selected_scene_internal_id
        .filter(|selected| {
            aligned_scene_configs
                .iter()
                .any(|scene| scene.internal_scene_id.to_string() == *selected)
        })
        .or_else(|| {
            aligned_scene_configs
                .first()
                .map(|scene| scene.internal_scene_id.to_string())
        });
    state.replace_snapshot(crate::show::types::ShowDocument {
        lockout: imported.snapshot.lockout,
        scene_configs: aligned_scene_configs.clone(),
        cued_scene_internal_id: imported.snapshot.cued_scene_internal_id,
    });
    state.set_selected_scene_internal_id(selected_scene_internal_id.clone());
    state.mark_saved(path, saved_at.clone());
    if should_mark_dirty {
        state.mark_dirty();
    }
    publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
    for scene in report.removed_scenes.iter() {
        tracing::warn!(event = "session_scene_pruned", scene = %scene, "Skipped loading \"{scene}\" because it was not found in the current scene list.");
    }
    if alignment_changed {
        tracing::debug!(
            event = "session_scene_alignment",
            "{}",
            crate::show::scene_alignment::scene_alignment_diagnostic(
                &imported_scene_configs,
                &aligned_scene_configs,
                &lv1.scene_list
            )
        );
    }
    tracing::info!(event = "session_opened", "Session loaded");
    Ok(LoadShowFileResult {
        selected_scene_internal_id,
        saved_at,
        report,
    })
}

fn store_scene_config_from_channels(
    state: &mut ShowState,
    event_bus: &AppEventBus,
    internal_scene_id: uuid::Uuid,
    channels: Vec<crate::lv1::ChannelInfo>,
) -> Result<ShowCommandResult, String> {
    if state.get_scene_config(internal_scene_id).is_none() {
        return Err("Scene config not found".to_string());
    }
    state
        .store_scene_config(internal_scene_id, &channels)
        .map(|changed| {
            publish_if_changed(event_bus, ShowProjectionReason::ShowState, state, changed);
            ShowCommandResult { changed }
        })
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use crate::lv1::{ConnectionStatus, Lv1StateSnapshot, SceneListEntry};
    use crate::runtime::events::AppEventBus;

    use super::load_show_file_from_dto;
    use crate::show::{
        SceneConfig, SceneScopeToggles, ShowFile, ShowFileSafety, ShowFileSceneConfig, ShowState,
    };

    fn lv1_snapshot(scenes: Vec<SceneListEntry>) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: scenes,
            channels: Vec::new(),
        }
    }

    fn show_file(scenes: Vec<ShowFileSceneConfig>) -> ShowFile {
        ShowFile {
            schema_version: crate::show::SHOW_FILE_SCHEMA_VERSION,
            app_version: "test".to_string(),
            saved_at: "123".to_string(),
            safety: ShowFileSafety { lockout: false },
            scene_configs: scenes,
            cued_scene_internal_id: None,
        }
    }

    fn scene_config(id: u128, index: Option<i32>, name: &str, duration_ms: u64) -> SceneConfig {
        SceneConfig {
            internal_scene_id: Uuid::from_u128(id),
            scene_index: index,
            scene_name: name.to_string(),
            duration_ms,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn file_scene(config: SceneConfig) -> ShowFileSceneConfig {
        ShowFileSceneConfig {
            internal_scene_id: Some(config.internal_scene_id),
            scene_index: config.scene_index,
            scene_name: config.scene_name,
            duration_ms: config.duration_ms,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: Default::default(),
        }
    }

    #[test]
    fn connected_load_aligns_imported_configs_and_adds_default_linked_configs_for_extra_lv1_scenes()
    {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))]);

        let result = load_show_file_from_dto(
            &mut state,
            &event_bus,
            path,
            &mut file,
            &lv1_snapshot(vec![
                SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                },
                SceneListEntry {
                    index: 2,
                    name: "Verse".to_string(),
                },
            ]),
        )
        .expect("load should succeed");

        assert_eq!(state.scene_configs().len(), 2);
        assert_eq!(state.scene_configs()[0].scene_index, Some(1));
        assert_eq!(state.scene_configs()[0].duration_ms, 1_000);
        assert_eq!(state.scene_configs()[1].scene_index, Some(2));
        assert_eq!(state.scene_configs()[1].scene_name, "Verse");
        assert_eq!(state.scene_configs()[1].duration_ms, 0);
        assert_eq!(
            result.selected_scene_internal_id,
            Some(Uuid::from_u128(1).to_string())
        );
    }

    #[test]
    fn connected_load_drops_blank_file_configs_before_alignment() {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![
            file_scene(scene_config(1, Some(1), "Intro", 1_000)),
            file_scene(scene_config(2, Some(2), "Verse", 0)),
        ]);

        load_show_file_from_dto(
            &mut state,
            &event_bus,
            path,
            &mut file,
            &lv1_snapshot(vec![
                SceneListEntry {
                    index: 1,
                    name: "Intro".to_string(),
                },
                SceneListEntry {
                    index: 2,
                    name: "Verse".to_string(),
                },
            ]),
        )
        .expect("load should succeed");

        assert_eq!(state.scene_configs().len(), 2);
        assert_eq!(
            state.scene_configs()[0].internal_scene_id,
            Uuid::from_u128(1)
        );
        assert_eq!(state.scene_configs()[0].duration_ms, 1_000);
        assert_ne!(
            state.scene_configs()[1].internal_scene_id,
            Uuid::from_u128(2)
        );
        assert_eq!(state.scene_configs()[1].scene_index, Some(2));
        assert_eq!(state.scene_configs()[1].duration_ms, 0);
    }

    #[test]
    fn connected_load_marks_dirty_when_alignment_changes_imported_configs() {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))]);

        load_show_file_from_dto(
            &mut state,
            &event_bus,
            path,
            &mut file,
            &lv1_snapshot(vec![SceneListEntry {
                index: 2,
                name: "Intro".to_string(),
            }]),
        )
        .expect("load should succeed");

        assert!(state.projection_state().show_file_dirty);
        assert_eq!(state.scene_configs()[0].scene_index, Some(2));
    }

    #[test]
    fn connected_load_preserves_existing_imported_fade_data_for_matched_scenes() {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_500))]);

        load_show_file_from_dto(
            &mut state,
            &event_bus,
            path,
            &mut file,
            &lv1_snapshot(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
        )
        .expect("load should succeed");

        assert_eq!(state.scene_configs()[0].duration_ms, 1_500);
        assert_eq!(
            state.scene_configs()[0].internal_scene_id,
            Uuid::from_u128(1)
        );
    }

    #[test]
    fn connected_load_preserves_missing_imported_config_as_unlinked() {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![
            file_scene(scene_config(1, Some(1), "Intro", 1_000)),
            file_scene(scene_config(2, Some(2), "Verse", 2_000)),
        ]);

        load_show_file_from_dto(
            &mut state,
            &event_bus,
            path,
            &mut file,
            &lv1_snapshot(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
        )
        .expect("load should succeed");

        assert_eq!(state.scene_configs().len(), 2);
        assert_eq!(state.scene_configs()[0].scene_index, Some(1));
        assert_eq!(state.scene_configs()[1].scene_index, None);
        assert_eq!(state.scene_configs()[1].scene_name, "Verse");
    }
}
