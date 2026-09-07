use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, watch};

use crate::cue_lists::{CueListDocument, CueListsCommand, CueListsEvent, CueListsHandle};
use crate::lv1::{Lv1ActorError, Lv1ActorHandle, Lv1Command, Lv1StateSnapshot};
use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, RuntimeLifecycleEvent, log_lagged_subscriber};
use crate::runtime::generation::RuntimeGeneration;
use crate::scenes::{
    SceneDocument, ScenesCommand, ScenesCommandResult, ScenesHandle, ScenesProjectionReason,
};
use crate::show_file::{backup_folder, read_show_file, write_show_file};

use super::commands::ShowCommand;
use super::events::{ShowEvent, ShowProjectionReason};
use super::handle::ShowStateHandle;
use super::lockout::ShowLockoutReader;
use super::show_file::import_show_file;
use super::state::ShowState;
use super::{LoadShowFileResult, NewShowFileResult, ShowCommandResult};

const SHOW_LOCAL_ACTOR_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Default)]
pub struct ShowActorPeers {
    runtime_generation: RuntimeGeneration,
    lv1: Arc<Mutex<Option<(u64, Lv1ActorHandle)>>>,
    scenes: Arc<Mutex<Option<ScenesHandle>>>,
    cue_lists: Arc<Mutex<Option<CueListsHandle>>>,
}

impl ShowActorPeers {
    pub(crate) fn runtime_generation(&self) -> RuntimeGeneration {
        self.runtime_generation.clone()
    }

    pub fn set_lv1(&self, generation: u64, lv1: Lv1ActorHandle) {
        *self.lv1.lock().expect("show peer lock poisoned") = Some((generation, lv1));
    }

    pub fn set_scenes(&self, scenes: ScenesHandle) {
        *self.scenes.lock().expect("show peer lock poisoned") = Some(scenes);
    }

    pub fn set_cue_lists(&self, cue_lists: CueListsHandle) {
        *self.cue_lists.lock().expect("show peer lock poisoned") = Some(cue_lists);
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

    fn lv1(&self) -> Option<(u64, Lv1ActorHandle)> {
        self.lv1
            .lock()
            .expect("show peer lock poisoned")
            .as_ref()
            .map(|(generation, lv1)| (*generation, lv1.clone()))
    }

    pub fn scenes(&self) -> Option<ScenesHandle> {
        self.scenes
            .lock()
            .expect("show peer lock poisoned")
            .as_ref()
            .cloned()
    }

    pub fn cue_lists(&self) -> Option<CueListsHandle> {
        self.cue_lists
            .lock()
            .expect("show peer lock poisoned")
            .as_ref()
            .cloned()
    }
}

pub struct ShowActorTask {
    rx: mpsc::Receiver<ShowCommand>,
    event_bus: AppEventBus,
    peers: ShowActorPeers,
    state: ShowState,
    lockout_tx: watch::Sender<bool>,
}

impl ShowActorTask {
    pub fn spawn(self) {
        tauri::async_runtime::spawn(run_show_actor(
            self.rx,
            self.event_bus,
            self.peers,
            self.state,
            self.lockout_tx,
        ));
    }
}

pub fn build_show_actor(
    event_bus: AppEventBus,
) -> (
    ShowStateHandle,
    ShowActorTask,
    ShowActorPeers,
    ShowLockoutReader,
) {
    build_show_actor_with_state(event_bus, ShowState::default())
}

fn build_show_actor_with_state(
    event_bus: AppEventBus,
    state: ShowState,
) -> (
    ShowStateHandle,
    ShowActorTask,
    ShowActorPeers,
    ShowLockoutReader,
) {
    let (tx, rx) = mpsc::channel(32);
    let (lockout_tx, lockout_rx) = watch::channel(state.lockout());
    let peers = ShowActorPeers::default();
    let task = ShowActorTask {
        rx,
        event_bus,
        peers: peers.clone(),
        state,
        lockout_tx,
    };
    (
        ShowStateHandle::new(tx),
        task,
        peers,
        ShowLockoutReader::new(lockout_rx),
    )
}

async fn run_show_actor(
    mut rx: mpsc::Receiver<ShowCommand>,
    event_bus: AppEventBus,
    peers: ShowActorPeers,
    mut state: ShowState,
    lockout_tx: watch::Sender<bool>,
) {
    let mut events = event_bus.subscribe();
    let mut active_generation = 0;
    loop {
        tokio::select! {
            command = rx.recv() => {
                let Some(command) = command else { break; };
                handle_command(command, &mut state, &event_bus, &peers).await;
                publish_lockout_if_changed(&lockout_tx, &state);
            }
            event = events.recv() => {
                match event {
                    Ok(event) => handle_app_event(event, &mut active_generation, &mut state, &event_bus),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        log_lagged_subscriber("show-actor", count);
                        state.mark_dirty();
                        publish_state_changed(&event_bus, ShowProjectionReason::FileMetadata, &state);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

fn publish_lockout_if_changed(lockout_tx: &watch::Sender<bool>, state: &ShowState) {
    lockout_tx.send_if_modified(|current| {
        let next = state.lockout();
        let changed = *current != next;
        *current = next;
        changed
    });
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
        AppEvent::Scenes {
            generation: _,
            event:
                crate::scenes::ScenesEvent::StateChanged {
                    persisted_scene_edit: true,
                    ..
                },
        } => {
            state.mark_dirty();
            publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
        }
        AppEvent::CueLists(CueListsEvent::StateChanged {
            persisted_cue_list_edit: true,
            ..
        }) => {
            state.mark_dirty();
            publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
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
        ShowCommand::CurrentShowFilePath { reply } => {
            let _ = reply.send(state.current_show_file_path());
        }
        ShowCommand::GetLockout { reply } => {
            let _ = reply.send(state.lockout());
        }
        ShowCommand::InitialProjectionState { reply } => {
            let _ = reply.send(state.projection_state());
        }
        ShowCommand::SetLockout { enabled, reply } => {
            let changed = state.set_lockout(enabled);
            publish_if_changed(
                event_bus,
                ShowProjectionReason::FileMetadata,
                state,
                changed,
            );
            if let Some(reply) = reply {
                let _ = reply.send(ShowCommandResult { changed });
            }
        }
        ShowCommand::NewShowFileFromCurrentLv1 { reply } => {
            let result = async {
                let (expected_generation, lv1) = current_lv1_snapshot(peers).await?;
                let scene_document = SceneDocument {
                    scene_configs: crate::scenes::align_scene_configs(Vec::new(), &lv1.scene_list),
                    selected_scene_internal_id: None,
                };
                let selected_scene_internal_id = scene_document
                    .scene_configs
                    .first()
                    .map(|scene| scene.internal_scene_id.to_string());
                let original_scene = current_scene_document(peers).await?;
                let original_cue_lists = current_cue_list_document(peers).await?;
                peers
                    .runtime_generation()
                    .if_current_async(expected_generation, || async {
                        validate_lv1_snapshot(peers, expected_generation, &lv1).await?;
                        replace_documents_with_rollback(
                            peers,
                            original_scene,
                            original_cue_lists,
                            scene_document,
                            CueListDocument::default(),
                            ScenesProjectionReason::FileReplacement,
                            false,
                            false,
                        )
                        .await?;
                        state.reset_for_new_show();
                        publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
                        tracing::info!(event = "session_created", "New session created");
                        Ok::<_, String>(())
                    })
                    .await
                    .ok_or_else(|| "LV1 generation is no longer current".to_string())??;
                Ok(NewShowFileResult {
                    selected_scene_internal_id,
                })
            }
            .await;
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        ShowCommand::SaveShowFileAs { path, reply } => {
            let result = async {
                let saved_at = crate::time::current_timestamp_millis();
                let scene_document = current_scene_document(peers).await?;
                let cue_list_document = current_cue_list_document(peers).await?;
                let file = crate::show::show_file::export_show_file(
                    scene_document,
                    cue_list_document,
                    state.lockout(),
                    saved_at.clone(),
                );
                write_show_file(&path, &file, &backup_folder())?;
                state.mark_saved(path, saved_at);
                publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
                tracing::info!(event = "session_saved", "Session saved");
                Ok(ShowCommandResult { changed: true })
            }
            .await;
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
        ShowCommand::CompleteLv1Connection { identity, reply } => {
            let outcome = state.complete_lv1_connection(identity);
            let changed = outcome.changed;
            publish_if_changed(
                event_bus,
                ShowProjectionReason::ConnectionMetadata,
                state,
                changed,
            );
            if let Some(reply) = reply {
                let _ = reply.send(outcome);
            }
        }
        ShowCommand::CompleteLv1ConnectionIfCurrent {
            identity,
            runtime_generation,
            expected_generation,
            reply,
        } => {
            let outcome = runtime_generation
                .if_current(expected_generation, || {
                    let outcome = state.complete_lv1_connection(identity);
                    publish_if_changed(
                        event_bus,
                        ShowProjectionReason::ConnectionMetadata,
                        state,
                        outcome.changed,
                    );
                    outcome
                })
                .await
                .unwrap_or(super::CompleteConnectionOutcome {
                    accepted: false,
                    changed: false,
                });
            let _ = reply.send(outcome);
        }
        ShowCommand::ClearLv1ConnectionIfCurrent {
            runtime_generation,
            expected_generation,
            reply,
        } => {
            let outcome = runtime_generation
                .if_current(expected_generation, || {
                    let changed = state.clear_lv1_connection();
                    publish_if_changed(
                        event_bus,
                        ShowProjectionReason::ConnectionMetadata,
                        state,
                        changed,
                    );
                    super::CompleteConnectionOutcome {
                        accepted: true,
                        changed,
                    }
                })
                .await
                .unwrap_or(super::CompleteConnectionOutcome {
                    accepted: false,
                    changed: false,
                });
            let _ = reply.send(outcome);
        }
        ShowCommand::FailLv1Connection { reply } => {
            let changed = state.fail_lv1_connection();
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
        ShowCommand::FailLv1ConnectionIfCurrent {
            runtime_generation,
            expected_generation,
            reply,
        } => {
            let outcome = runtime_generation
                .if_current(expected_generation, || {
                    let changed = state.fail_lv1_connection();
                    publish_if_changed(
                        event_bus,
                        ShowProjectionReason::ConnectionMetadata,
                        state,
                        changed,
                    );
                    super::CompleteConnectionOutcome {
                        accepted: true,
                        changed,
                    }
                })
                .await
                .unwrap_or(super::CompleteConnectionOutcome {
                    accepted: false,
                    changed: false,
                });
            let _ = reply.send(outcome);
        }
        ShowCommand::LoadShowFileFromPath { path, reply } => {
            let result = async {
                let (expected_generation, lv1) = current_lv1_snapshot(peers).await?;
                let mut file = read_show_file(&path)?;
                load_show_file_from_dto_if_current(
                    state,
                    event_bus,
                    peers,
                    path,
                    &mut file,
                    &lv1,
                    expected_generation,
                    false,
                )
                .await
            }
            .await;
            if let Some(reply) = reply {
                let _ = reply.send(result);
            }
        }
        #[cfg(test)]
        ShowCommand::ClearForTest { reply } => {
            state.clear();
            publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
            if let Some(reply) = reply {
                let _ = reply.send(());
            }
        }
    }
}

async fn current_lv1_snapshot(peers: &ShowActorPeers) -> Result<(u64, Lv1StateSnapshot), String> {
    let (generation, lv1) = peers
        .lv1()
        .ok_or(AppCommandError::Lv1Unavailable)
        .map_err(map_app_command_error)?;
    let snapshot = get_lv1_state(&lv1).await?;
    if snapshot.connection != crate::lv1::ConnectionStatus::Connected {
        return Err(AppCommandError::Lv1Unavailable.to_string());
    }
    Ok((generation, snapshot))
}

async fn get_lv1_state(lv1: &Lv1ActorHandle) -> Result<Lv1StateSnapshot, String> {
    let (reply, rx) = tokio::sync::oneshot::channel();
    tokio::time::timeout(
        SHOW_LOCAL_ACTOR_TIMEOUT,
        lv1.send(Lv1Command::GetState { reply }),
    )
    .await
    .map_err(|_| "LV1 state request timed out".to_string())?
    .map_err(|error| match error {
        Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
        other => AppCommandError::CommandFailed(other.to_string()),
    })
    .map_err(map_app_command_error)?;
    tokio::time::timeout(SHOW_LOCAL_ACTOR_TIMEOUT, rx)
        .await
        .map_err(|_| "LV1 state reply timed out".to_string())?
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)
}

async fn validate_lv1_snapshot(
    peers: &ShowActorPeers,
    expected_generation: u64,
    expected_snapshot: &Lv1StateSnapshot,
) -> Result<(), String> {
    let (peer_generation, lv1) = peers
        .lv1()
        .ok_or_else(|| "LV1 generation is no longer current".to_string())?;
    if peer_generation != expected_generation {
        return Err("LV1 generation is no longer current".to_string());
    }
    let snapshot = get_lv1_state(&lv1).await?;
    if snapshot.connection != crate::lv1::ConnectionStatus::Connected {
        return Err("LV1 is no longer connected".to_string());
    }
    if snapshot.scene_list != expected_snapshot.scene_list {
        return Err("LV1 scene list changed during show replacement".to_string());
    }
    Ok(())
}

fn map_app_command_error(error: AppCommandError) -> String {
    match error {
        AppCommandError::CommandFailed(message) => message,
        other => other.to_string(),
    }
}

#[cfg(test)]
async fn load_show_file_from_dto(
    state: &mut ShowState,
    event_bus: &AppEventBus,
    peers: &ShowActorPeers,
    path: std::path::PathBuf,
    file: &mut super::show_file::ShowFile,
    lv1: &Lv1StateSnapshot,
) -> Result<LoadShowFileResult, String> {
    load_show_file_from_dto_if_current(state, event_bus, peers, path, file, lv1, 0, true).await
}

#[allow(clippy::too_many_arguments)]
async fn load_show_file_from_dto_if_current(
    state: &mut ShowState,
    event_bus: &AppEventBus,
    peers: &ShowActorPeers,
    path: std::path::PathBuf,
    file: &mut super::show_file::ShowFile,
    lv1: &Lv1StateSnapshot,
    expected_generation: u64,
    allow_missing_lv1_peer: bool,
) -> Result<LoadShowFileResult, String> {
    let imported = import_show_file(file, lv1)?;
    let saved_at = file.saved_at.clone();
    let selected_scene_internal_id = imported.selected_scene_internal_id.clone();
    let imported_scene_configs = imported.snapshot.scene_configs;
    let imported_cue_list_snapshot = imported.cue_list_snapshot.clone();
    let aligned_scene_configs =
        crate::scenes::align_scene_configs(imported_scene_configs.clone(), &lv1.scene_list);
    let alignment_changed = aligned_scene_configs != imported_scene_configs;
    let mut should_mark_dirty = imported.generated_internal_scene_ids || alignment_changed;
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
    let scene_document = SceneDocument {
        scene_configs: aligned_scene_configs.clone(),
        selected_scene_internal_id: selected_scene_internal_id.clone(),
    };
    let original_scene = current_scene_document(peers).await?;
    let original_cue_lists = current_cue_list_document(peers).await?;
    let readback_original_scene = original_scene.clone();
    let readback_original_cue_lists = original_cue_lists.clone();
    peers
        .runtime_generation()
        .if_current_async(expected_generation, || async {
            if !allow_missing_lv1_peer {
                validate_lv1_snapshot(peers, expected_generation, lv1).await?;
            }
            replace_documents_with_rollback(
                peers,
                original_scene,
                original_cue_lists,
                scene_document,
                imported.cue_list_snapshot.clone(),
                ScenesProjectionReason::FileReplacement,
                false,
                false,
            )
            .await?;
            let reconciled_cue_list_document = match current_cue_list_document(peers).await {
                Ok(document) => document,
                Err(error) => {
                    let rollback = rollback_documents(
                        peers,
                        readback_original_scene.clone(),
                        readback_original_cue_lists.clone(),
                        readback_original_scene
                            .scene_configs
                            .iter()
                            .map(|scene| scene.internal_scene_id)
                            .collect(),
                        ScenesProjectionReason::FileReplacement,
                        false,
                        false,
                    )
                    .await;
                    return Err(format_replacement_error(
                        "cue-list readback",
                        error,
                        rollback,
                    ));
                }
            };
            if reconciled_cue_list_document != imported_cue_list_snapshot {
                should_mark_dirty = true;
            }
            state.set_lockout(imported.lockout);
            state.mark_saved(path, saved_at.clone());
            if should_mark_dirty {
                state.mark_dirty();
            }
            publish_state_changed(event_bus, ShowProjectionReason::FileMetadata, state);
            Ok::<_, String>(())
        })
        .await
        .ok_or_else(|| "LV1 generation is no longer current".to_string())??;
    if alignment_changed {
        tracing::debug!(
            event = "session_scene_alignment",
            "{}",
            crate::scenes::scene_alignment_diagnostic(
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
    })
}

async fn current_scene_document(peers: &ShowActorPeers) -> Result<SceneDocument, String> {
    let scenes = peers
        .scenes()
        .ok_or_else(|| "Show blocked: scenes state is unavailable".to_string())?;
    let (reply, rx) = tokio::sync::oneshot::channel();
    tokio::time::timeout(
        SHOW_LOCAL_ACTOR_TIMEOUT,
        scenes.send(ScenesCommand::GetSceneDocument { reply }),
    )
    .await
    .map_err(|_| "Show blocked: scenes state request timed out".to_string())?
    .map_err(|_| "Show blocked: scenes state is unavailable".to_string())?;
    tokio::time::timeout(SHOW_LOCAL_ACTOR_TIMEOUT, rx)
        .await
        .map_err(|_| "Show blocked: scenes state reply timed out".to_string())?
        .map_err(|_| "Show blocked: scenes state is unavailable".to_string())
}

async fn current_cue_list_document(peers: &ShowActorPeers) -> Result<CueListDocument, String> {
    let cue_lists = peers
        .cue_lists()
        .ok_or_else(|| "Show blocked: cue lists state is unavailable".to_string())?;
    let (reply, rx) = tokio::sync::oneshot::channel();
    tokio::time::timeout(
        SHOW_LOCAL_ACTOR_TIMEOUT,
        cue_lists.send(CueListsCommand::GetCueListDocument { reply }),
    )
    .await
    .map_err(|_| "Show blocked: cue lists state request timed out".to_string())?
    .map_err(|_| "Show blocked: cue lists state is unavailable".to_string())?;
    tokio::time::timeout(SHOW_LOCAL_ACTOR_TIMEOUT, rx)
        .await
        .map_err(|_| "Show blocked: cue lists state reply timed out".to_string())?
        .map_err(|_| "Show blocked: cue lists state is unavailable".to_string())
}

#[allow(clippy::too_many_arguments)]
async fn replace_documents_with_rollback(
    peers: &ShowActorPeers,
    original_scene: SceneDocument,
    original_cue_lists: CueListDocument,
    replacement_scene: SceneDocument,
    replacement_cue_lists: CueListDocument,
    reason: ScenesProjectionReason,
    persisted_scene_edit: bool,
    persisted_cue_list_edit: bool,
) -> Result<(), String> {
    let original_scene_ids = original_scene
        .scene_configs
        .iter()
        .map(|scene| scene.internal_scene_id)
        .collect::<Vec<_>>();
    let replacement_scene_ids = replacement_scene
        .scene_configs
        .iter()
        .map(|scene| scene.internal_scene_id)
        .collect::<Vec<_>>();

    let scene_result =
        replace_scene_document(peers, replacement_scene, reason, persisted_scene_edit).await;
    if let Err(error) = scene_result {
        let rollback = rollback_documents(
            peers,
            original_scene,
            original_cue_lists,
            original_scene_ids,
            reason,
            persisted_scene_edit,
            persisted_cue_list_edit,
        )
        .await;
        return Err(format_replacement_error(
            "scene replacement",
            error,
            rollback,
        ));
    }

    if let Err(error) = replace_cue_list_document(
        peers,
        replacement_cue_lists,
        replacement_scene_ids,
        persisted_cue_list_edit,
    )
    .await
    {
        let rollback = rollback_documents(
            peers,
            original_scene,
            original_cue_lists,
            original_scene_ids,
            reason,
            persisted_scene_edit,
            persisted_cue_list_edit,
        )
        .await;
        return Err(format_replacement_error(
            "cue-list replacement",
            error,
            rollback,
        ));
    }

    Ok(())
}

async fn rollback_documents(
    peers: &ShowActorPeers,
    original_scene: SceneDocument,
    original_cue_lists: CueListDocument,
    original_scene_ids: Vec<uuid::Uuid>,
    reason: ScenesProjectionReason,
    persisted_scene_edit: bool,
    persisted_cue_list_edit: bool,
) -> Vec<String> {
    let mut failures = Vec::new();
    if let Err(error) =
        replace_scene_document(peers, original_scene, reason, persisted_scene_edit).await
    {
        failures.push(format!("scene rollback failed: {error}"));
    }
    if let Err(error) = replace_cue_list_document(
        peers,
        original_cue_lists,
        original_scene_ids,
        persisted_cue_list_edit,
    )
    .await
    {
        failures.push(format!("cue-list rollback failed: {error}"));
    }
    failures
}

fn format_replacement_error(operation: &str, error: String, rollback: Vec<String>) -> String {
    if rollback.is_empty() {
        format!("{operation} failed: {error}; rollback completed")
    } else {
        format!(
            "{operation} failed: {error}; rollback incomplete: {}",
            rollback.join("; ")
        )
    }
}

async fn replace_scene_document(
    peers: &ShowActorPeers,
    document: SceneDocument,
    reason: ScenesProjectionReason,
    persisted_scene_edit: bool,
) -> Result<ScenesCommandResult, String> {
    let scenes = peers
        .scenes()
        .ok_or_else(|| "Show blocked: scenes state is unavailable".to_string())?;
    let (reply, rx) = tokio::sync::oneshot::channel();
    tokio::time::timeout(
        SHOW_LOCAL_ACTOR_TIMEOUT,
        scenes.send(ScenesCommand::ReplaceSceneDocument {
            document,
            reason,
            persisted_scene_edit,
            reply: Some(reply),
        }),
    )
    .await
    .map_err(|_| "Show blocked: scenes replacement request timed out".to_string())?
    .map_err(|_| "Show blocked: scenes state is unavailable".to_string())?;
    tokio::time::timeout(SHOW_LOCAL_ACTOR_TIMEOUT, rx)
        .await
        .map_err(|_| "Show blocked: scenes replacement reply timed out".to_string())?
        .map_err(|_| "Show blocked: scenes state is unavailable".to_string())
}

async fn replace_cue_list_document(
    peers: &ShowActorPeers,
    document: CueListDocument,
    valid_scene_ids: Vec<uuid::Uuid>,
    persisted_cue_list_edit: bool,
) -> Result<crate::cue_lists::CueListsCommandResult, String> {
    let cue_lists = peers
        .cue_lists()
        .ok_or_else(|| "Show blocked: cue lists state is unavailable".to_string())?;
    let (reply, rx) = tokio::sync::oneshot::channel();
    tokio::time::timeout(
        SHOW_LOCAL_ACTOR_TIMEOUT,
        cue_lists.send(CueListsCommand::ReplaceCueListDocument {
            document,
            valid_scene_ids,
            persisted_cue_list_edit,
            reply: Some(reply),
        }),
    )
    .await
    .map_err(|_| "Show blocked: cue lists replacement request timed out".to_string())?
    .map_err(|_| "Show blocked: cue lists state is unavailable".to_string())?;
    tokio::time::timeout(SHOW_LOCAL_ACTOR_TIMEOUT, rx)
        .await
        .map_err(|_| "Show blocked: cue lists replacement reply timed out".to_string())?
        .map_err(|_| "Show blocked: cue lists state is unavailable".to_string())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::{
        load_show_file_from_dto, load_show_file_from_dto_if_current, replace_cue_list_document,
        replace_documents_with_rollback, replace_scene_document,
    };
    use crate::cue_lists::{
        CueListDocument, CueListsCommand, CueListsEvent, CueListsProjectionReason,
        CueListsProjectionState, build_cue_lists_actor_with_scenes,
    };
    use crate::lv1::{ConnectionStatus, Lv1StateSnapshot, SceneListEntry};
    use crate::runtime::events::{AppEventBus, RuntimeLifecycleEvent};
    use crate::runtime::generation::RuntimeGeneration;
    use crate::scenes::{SceneConfig, SceneScopeToggles, ScenesProjectionReason};
    use crate::scenes::{ScenesCommand, build_scenes_actor};
    use crate::settings::{AppSettings, SettingsCommand, SettingsHandle};
    use crate::show::commands::ShowCommand;
    use crate::show::events::{ShowEvent, ShowProjectionReason};
    use crate::show::handle::ShowStateHandle;
    use crate::show::{ShowFile, ShowFileSafety, ShowFileSceneConfig, ShowState};

    fn lv1_snapshot(scenes: Vec<SceneListEntry>) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: scenes,
            channels: Vec::new(),
            ping_sequence: 0,
        }
    }

    fn show_file(scenes: Vec<ShowFileSceneConfig>) -> ShowFile {
        ShowFile {
            schema_version: crate::show::SHOW_FILE_SCHEMA_VERSION,
            app_version: "test".to_string(),
            saved_at: "123".to_string(),
            safety: ShowFileSafety { lockout: false },
            scene_configs: scenes,
            cue_lists: Vec::new(),
            active_cue_list_id: None,
            cued_cue_entry_id: None,
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

    fn show_actor_peers() -> super::ShowActorPeers {
        let peers = super::ShowActorPeers::default();
        let event_bus = AppEventBus::default();
        let (scenes, task, _peers) = build_scenes_actor(
            0,
            peers.runtime_generation(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();
        peers.set_scenes(scenes);
        let (cue_lists, task, _cue_lists_peers) =
            build_cue_lists_actor_with_scenes(AppEventBus::default(), peers.scenes().unwrap());
        task.spawn();
        peers.set_cue_lists(cue_lists);
        peers
    }

    fn test_lockout_reader() -> super::ShowLockoutReader {
        let (_sender, receiver) = tokio::sync::watch::channel(false);
        super::ShowLockoutReader::new(receiver)
    }

    fn show_actor(event_bus: AppEventBus) -> (ShowStateHandle, super::ShowActorPeers) {
        let (handle, task, peers, _lockout) = super::build_show_actor(event_bus);
        task.spawn();
        (handle, peers)
    }

    fn fake_settings_handle() -> SettingsHandle {
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let SettingsCommand::GetSettings { reply } = command {
                    let _ = reply.send(AppSettings::default());
                }
            }
        });
        tx
    }

    async fn get_scene_document(
        handle: &crate::scenes::ScenesHandle,
    ) -> crate::scenes::SceneDocument {
        let (reply, rx) = tokio::sync::oneshot::channel();
        handle
            .send(ScenesCommand::GetSceneDocument { reply })
            .await
            .unwrap();
        rx.await.unwrap()
    }

    async fn get_cue_list_document(
        handle: &crate::cue_lists::CueListsHandle,
    ) -> crate::cue_lists::CueListDocument {
        let (reply, rx) = tokio::sync::oneshot::channel();
        handle
            .send(CueListsCommand::GetCueListDocument { reply })
            .await
            .unwrap();
        rx.await.unwrap()
    }

    async fn recv_file_metadata_event(
        events: &mut tokio::sync::broadcast::Receiver<crate::runtime::events::AppEvent>,
    ) -> crate::show::events::ShowProjectionState {
        loop {
            match events.recv().await.unwrap() {
                crate::runtime::events::AppEvent::Show(ShowEvent::StateChanged {
                    reason: ShowProjectionReason::FileMetadata,
                    state,
                }) => {
                    return state;
                }
                _ => continue,
            }
        }
    }

    #[tokio::test]
    async fn clear_current_connection_publishes_metadata_state_change() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let (show, _peers) = show_actor(event_bus);
        let identity = crate::connection_state::Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1".to_string()),
            address: "127.0.0.1".to_string(),
            port: 50_000,
        };
        show.send(ShowCommand::CompleteLv1Connection {
            identity,
            reply: None,
        })
        .await
        .unwrap();
        let _ = events.recv().await.unwrap();

        let (reply, response) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::ClearLv1ConnectionIfCurrent {
            runtime_generation: RuntimeGeneration::default(),
            expected_generation: 0,
            reply,
        })
        .await
        .unwrap();
        assert!(response.await.unwrap().changed);
        assert!(matches!(
            events.recv().await.unwrap(),
            crate::runtime::events::AppEvent::Show(ShowEvent::StateChanged {
                reason: ShowProjectionReason::ConnectionMetadata,
                state,
            }) if state.connected_lv1_identity.is_none()
        ));
    }

    #[tokio::test]
    async fn generation_advance_waits_through_scene_and_cue_replacements() {
        let peers = show_actor_peers();
        let generation = peers.runtime_generation();
        let (scenes_done_tx, scenes_done_rx) = tokio::sync::oneshot::channel();
        let (cue_release_tx, cue_release_rx) = tokio::sync::oneshot::channel();
        let (cue_done_tx, cue_done_rx) = tokio::sync::oneshot::channel();
        let (transaction_release_tx, transaction_release_rx) = tokio::sync::oneshot::channel();
        let transaction_peers = peers.clone();
        let transaction = tokio::spawn(async move {
            generation
                .if_current_async(0, || async move {
                    replace_scene_document(
                        &transaction_peers,
                        crate::scenes::SceneDocument::empty(),
                        ScenesProjectionReason::FileReplacement,
                        false,
                    )
                    .await
                    .unwrap();
                    scenes_done_tx.send(()).unwrap();
                    cue_release_rx.await.unwrap();
                    replace_cue_list_document(
                        &transaction_peers,
                        CueListDocument::default(),
                        Vec::new(),
                        false,
                    )
                    .await
                    .unwrap();
                    cue_done_tx.send(()).unwrap();
                    transaction_release_rx.await.unwrap();
                })
                .await;
        });

        tokio::time::timeout(std::time::Duration::from_secs(1), scenes_done_rx)
            .await
            .unwrap()
            .unwrap();
        let mut advance = tokio::spawn({
            let generation = peers.runtime_generation();
            async move { generation.advance().await }
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut advance)
                .await
                .is_err()
        );

        cue_release_tx.send(()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), cue_done_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), &mut advance)
                .await
                .is_err()
        );

        transaction_release_tx.send(()).unwrap();
        assert_eq!(advance.await.unwrap(), 1);
        tokio::time::timeout(std::time::Duration::from_secs(1), transaction)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn stalled_local_actor_releases_generation_gate_after_rollback_timeout() {
        let peers = show_actor_peers();
        let original_scene = get_scene_document(&peers.scenes().unwrap()).await;
        let original_cue_lists = get_cue_list_document(&peers.cue_lists().unwrap()).await;
        let (scenes_tx, scenes_rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let _receiver = scenes_rx;
            std::future::pending::<()>().await;
        });
        peers.set_scenes(scenes_tx);
        let generation = peers.runtime_generation();
        let transaction_peers = peers.clone();
        let transaction = tokio::spawn(async move {
            generation
                .if_current_async(0, || async move {
                    replace_documents_with_rollback(
                        &transaction_peers,
                        original_scene,
                        original_cue_lists,
                        crate::scenes::SceneDocument::empty(),
                        CueListDocument::default(),
                        ScenesProjectionReason::FileReplacement,
                        false,
                        false,
                    )
                    .await
                    .unwrap_err();
                })
                .await;
        });
        let generation = peers.runtime_generation();
        let advance = tokio::spawn(async move { generation.advance().await });
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(2), advance)
                .await
                .unwrap()
                .unwrap(),
            1
        );
        tokio::time::timeout(std::time::Duration::from_secs(2), transaction)
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn cue_replacement_failure_rolls_back_scene_document() {
        let peers = show_actor_peers();
        let original_scene = get_scene_document(&peers.scenes().unwrap()).await;
        let original_cue_lists = get_cue_list_document(&peers.cue_lists().unwrap()).await;
        let (cue_tx, cue_rx) = tokio::sync::mpsc::channel(1);
        drop(cue_rx);
        peers.set_cue_lists(cue_tx);

        let error = replace_documents_with_rollback(
            &peers,
            original_scene.clone(),
            original_cue_lists,
            crate::scenes::SceneDocument {
                scene_configs: vec![scene_config(42, Some(1), "Replacement", 1_000)],
                selected_scene_internal_id: None,
            },
            CueListDocument::default(),
            ScenesProjectionReason::FileReplacement,
            false,
            false,
        )
        .await
        .unwrap_err();

        assert!(error.contains("cue-list replacement failed"));
        assert!(error.contains("rollback incomplete"));
        assert_eq!(
            get_scene_document(&peers.scenes().unwrap()).await,
            original_scene
        );
    }

    #[tokio::test]
    async fn new_show_rejects_lv1_change_before_mutating_documents() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus);
        let fixture = show_actor_peers();
        peers.set_scenes(fixture.scenes().unwrap());
        peers.set_cue_lists(fixture.cue_lists().unwrap());
        let first = lv1_snapshot(vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }]);
        let mut second = first.clone();
        second.connection = ConnectionStatus::Disconnected;
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(4);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        tokio::spawn(async move {
            let mut snapshots = [first, second].into_iter();
            while let Some(command) = lv1_rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(snapshots.next().unwrap());
                }
            }
        });
        peers.set_lv1(0, lv1);

        let (reply, response) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
            .await
            .unwrap();
        assert_eq!(
            response.await.unwrap().unwrap_err(),
            "LV1 is no longer connected"
        );
        assert!(
            get_scene_document(&peers.scenes().unwrap())
                .await
                .scene_configs
                .is_empty()
        );
        assert_eq!(
            get_cue_list_document(&peers.cue_lists().unwrap()).await,
            CueListDocument::default()
        );
    }

    #[tokio::test]
    async fn stale_load_rejects_before_mutating_either_document() {
        let event_bus = AppEventBus::default();
        let peers = show_actor_peers();
        let generation = peers.runtime_generation();
        assert_eq!(generation.advance().await, 1);
        let mut state = ShowState::default();
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 0))]);

        let result = load_show_file_from_dto_if_current(
            &mut state,
            &event_bus,
            &peers,
            std::path::PathBuf::from("stale.show"),
            &mut file,
            &lv1_snapshot(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
            0,
            true,
        )
        .await;

        assert_eq!(result.unwrap_err(), "LV1 generation is no longer current");
        assert!(
            get_scene_document(&peers.scenes().unwrap())
                .await
                .scene_configs
                .is_empty()
        );
        assert_eq!(
            get_cue_list_document(&peers.cue_lists().unwrap()).await,
            crate::cue_lists::CueListDocument::default()
        );
        assert_eq!(state, ShowState::default());
    }

    #[tokio::test]
    async fn connected_load_aligns_imported_configs_and_adds_default_linked_configs_for_extra_lv1_scenes()
     {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))]);

        let peers = show_actor_peers();
        let result = load_show_file_from_dto(
            &mut state,
            &event_bus,
            &peers,
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
        .await
        .expect("load should succeed");

        let scene_document = get_scene_document(&peers.scenes().unwrap()).await;
        assert_eq!(scene_document.scene_configs.len(), 2);
        assert_eq!(scene_document.scene_configs[0].scene_index, Some(1));
        assert_eq!(scene_document.scene_configs[0].duration_ms, 1_000);
        assert_eq!(scene_document.scene_configs[1].scene_index, Some(2));
        assert_eq!(scene_document.scene_configs[1].scene_name, "Verse");
        assert_eq!(scene_document.scene_configs[1].duration_ms, 0);
        assert_eq!(
            result.selected_scene_internal_id,
            Some(Uuid::from_u128(1).to_string())
        );
    }

    #[tokio::test]
    async fn connected_load_preserves_default_scene_ids_referenced_by_cue_entries() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus.clone());
        let (scenes, task, _scenes_peers) = build_scenes_actor(
            1,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();
        peers.set_scenes(scenes.clone());
        let (cue_lists, task, _cue_lists_peers) =
            build_cue_lists_actor_with_scenes(event_bus, scenes.clone());
        task.spawn();
        peers.set_cue_lists(cue_lists.clone());

        let lv1 = lv1_snapshot(vec![
            SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            },
            SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            },
        ]);
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(4);
        let lv1_handle = crate::lv1::test_actor_handle(lv1_tx);
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(lv1.clone());
                }
            }
        });
        peers.set_lv1(0, lv1_handle);

        let path = std::env::temp_dir().join(format!("show-load-cue-ids-{}.ascs", Uuid::new_v4()));
        let intro_id = Uuid::from_u128(1);
        let verse_id = Uuid::from_u128(2);
        let cue_list_id = Uuid::from_u128(3);
        let intro_entry_id = Uuid::from_u128(4);
        let verse_entry_id = Uuid::from_u128(5);
        let mut file = show_file(vec![
            file_scene(scene_config(1, Some(1), "Intro", 0)),
            file_scene(scene_config(2, Some(2), "Verse", 0)),
        ]);
        file.cue_lists = vec![crate::cue_lists::CueList {
            id: cue_list_id,
            name: "Main".to_string(),
            entries: vec![
                crate::cue_lists::CueEntry {
                    id: intro_entry_id,
                    scene_internal_id: intro_id,
                },
                crate::cue_lists::CueEntry {
                    id: verse_entry_id,
                    scene_internal_id: verse_id,
                },
            ],
        }];
        file.active_cue_list_id = Some(cue_list_id);
        file.cued_cue_entry_id = Some(intro_entry_id);

        crate::show_file::write_show_file(&path, &file, &crate::show_file::backup_folder())
            .unwrap();

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::LoadShowFileFromPath {
            path: path.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();
        assert!(rx.await.unwrap().is_ok());

        let scene_document = get_scene_document(&scenes).await;
        assert_eq!(scene_document.scene_configs.len(), 2);
        assert_eq!(scene_document.scene_configs[0].internal_scene_id, intro_id);
        assert_eq!(scene_document.scene_configs[1].internal_scene_id, verse_id);

        let cue_document = get_cue_list_document(&cue_lists).await;
        assert_eq!(cue_document.cue_lists[0].entries.len(), 2);
        assert_eq!(
            cue_document.cue_lists[0].entries[0].scene_internal_id,
            intro_id
        );
        assert_eq!(
            cue_document.cue_lists[0].entries[1].scene_internal_id,
            verse_id
        );
        assert_eq!(cue_document.cued_cue_entry_id, Some(intro_entry_id));

        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn connected_load_marks_dirty_when_alignment_changes_imported_configs() {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))]);

        let peers = show_actor_peers();
        load_show_file_from_dto(
            &mut state,
            &event_bus,
            &peers,
            path,
            &mut file,
            &lv1_snapshot(vec![SceneListEntry {
                index: 2,
                name: "Intro".to_string(),
            }]),
        )
        .await
        .expect("load should succeed");

        assert!(state.projection_state().show_file_dirty);
        let scene_document = get_scene_document(&peers.scenes().unwrap()).await;
        assert_eq!(scene_document.scene_configs[0].scene_index, Some(2));
    }

    #[tokio::test]
    async fn connected_load_preserves_existing_imported_fade_data_for_matched_scenes() {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_500))]);

        let peers = show_actor_peers();
        load_show_file_from_dto(
            &mut state,
            &event_bus,
            &peers,
            path,
            &mut file,
            &lv1_snapshot(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
        )
        .await
        .expect("load should succeed");

        let scene_document = get_scene_document(&peers.scenes().unwrap()).await;
        assert_eq!(scene_document.scene_configs[0].duration_ms, 1_500);
        assert_eq!(
            scene_document.scene_configs[0].internal_scene_id,
            Uuid::from_u128(1)
        );
    }

    #[tokio::test]
    async fn connected_load_preserves_missing_imported_config_as_unlinked() {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let mut file = show_file(vec![
            file_scene(scene_config(1, Some(1), "Intro", 1_000)),
            file_scene(scene_config(2, Some(2), "Verse", 2_000)),
        ]);

        let peers = show_actor_peers();
        load_show_file_from_dto(
            &mut state,
            &event_bus,
            &peers,
            path,
            &mut file,
            &lv1_snapshot(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
        )
        .await
        .expect("load should succeed");

        let scene_document = get_scene_document(&peers.scenes().unwrap()).await;
        assert_eq!(scene_document.scene_configs.len(), 2);
        assert_eq!(scene_document.scene_configs[0].scene_index, Some(1));
        assert_eq!(scene_document.scene_configs[1].scene_index, None);
        assert_eq!(scene_document.scene_configs[1].scene_name, "Verse");
    }

    #[tokio::test]
    async fn connected_load_clears_missing_cued_entry_but_keeps_the_cue_list_entry() {
        let event_bus = AppEventBus::default();
        let mut state = ShowState::default();
        let path = std::path::PathBuf::from("session.show");
        let cue_list_id = Uuid::from_u128(0x55555555555545558555555555555555);
        let entry_id = Uuid::from_u128(0x66666666666646668666666666666666);
        let missing_scene_id = Uuid::from_u128(0x77777777777747778777777777777777);
        let mut file = show_file(vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))]);
        file.cue_lists = vec![crate::cue_lists::CueList {
            id: cue_list_id,
            name: "Main".to_string(),
            entries: vec![crate::cue_lists::CueEntry {
                id: entry_id,
                scene_internal_id: missing_scene_id,
            }],
        }];
        file.active_cue_list_id = Some(cue_list_id);
        file.cued_cue_entry_id = Some(entry_id);

        let peers = show_actor_peers();
        load_show_file_from_dto(
            &mut state,
            &event_bus,
            &peers,
            path,
            &mut file,
            &lv1_snapshot(vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }]),
        )
        .await
        .expect("load should succeed");

        let cue_document = super::current_cue_list_document(&peers).await.unwrap();
        assert_eq!(cue_document.cue_lists[0].entries.len(), 1);
        assert_eq!(cue_document.cue_lists[0].entries[0].id, entry_id);
        assert_eq!(cue_document.cued_cue_entry_id, None);
    }

    #[tokio::test]
    async fn save_queries_scenes_for_the_scene_document() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus.clone());
        let (scenes, task, _peers) = build_scenes_actor(
            1,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();
        peers.set_scenes(scenes.clone());
        let (cue_lists, task, _cue_lists_peers) =
            build_cue_lists_actor_with_scenes(event_bus.clone(), scenes.clone());
        task.spawn();
        peers.set_cue_lists(cue_lists.clone());

        let scenes_document = crate::scenes::SceneDocument {
            scene_configs: vec![scene_config(11, Some(3), "Scene From Scenes", 2_500)],
            selected_scene_internal_id: Some("selected-from-scenes".to_string()),
        };
        let (reply, rx) = tokio::sync::oneshot::channel();
        scenes
            .send(ScenesCommand::ReplaceSceneDocument {
                document: scenes_document,
                reason: crate::scenes::ScenesProjectionReason::FileReplacement,
                persisted_scene_edit: false,
                reply: Some(reply),
            })
            .await
            .unwrap();
        let _ = rx.await.unwrap();

        let path = std::env::temp_dir().join(format!("show-save-{}.ascs", Uuid::new_v4()));
        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::SaveShowFileAs {
            path: path.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();

        assert!(rx.await.unwrap().is_ok());

        let saved = crate::show_file::read_show_file(&path).unwrap();
        assert_eq!(saved.scene_configs[0].scene_name, "Scene From Scenes");
        assert_eq!(saved.scene_configs[0].scene_index, Some(3));
    }

    #[tokio::test]
    async fn load_replaces_scenes_state_and_keeps_dirty_clear_for_replacement_event() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus.clone());
        let mut events = event_bus.subscribe();
        let (scenes, task, _peers) = build_scenes_actor(
            1,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();
        peers.set_scenes(scenes.clone());
        let (cue_lists, task, _cue_lists_peers) =
            build_cue_lists_actor_with_scenes(event_bus.clone(), scenes.clone());
        task.spawn();
        peers.set_cue_lists(cue_lists.clone());
        let new_lv1 = lv1_snapshot(vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }]);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let handle = crate::lv1::test_actor_handle(tx);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(new_lv1.clone());
                }
            }
        });
        peers.set_lv1(0, handle);

        let path = std::env::temp_dir().join(format!("show-load-{}.ascs", Uuid::new_v4()));
        let file = crate::show::show_file::ShowFile {
            schema_version: crate::show::SHOW_FILE_SCHEMA_VERSION,
            app_version: "test".to_string(),
            saved_at: "123".to_string(),
            safety: crate::show::show_file::ShowFileSafety { lockout: false },
            scene_configs: vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))],
            cue_lists: Vec::new(),
            active_cue_list_id: None,
            cued_cue_entry_id: None,
        };
        crate::show_file::write_show_file(&path, &file, &crate::show_file::backup_folder())
            .unwrap();

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::LoadShowFileFromPath {
            path: path.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();

        assert!(rx.await.unwrap().is_ok());

        let state = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            recv_file_metadata_event(&mut events),
        )
        .await
        .unwrap();
        assert!(!state.show_file_dirty);
        let scene_document = get_scene_document(&scenes).await;
        assert_eq!(scene_document.scene_configs[0].scene_name, "Intro");
        assert_eq!(
            scene_document.selected_scene_internal_id,
            Some(Uuid::from_u128(1).to_string())
        );
    }

    #[tokio::test]
    async fn load_marks_dirty_when_cue_reconciliation_clears_invalid_active_cue() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus.clone());
        let mut events = event_bus.subscribe();
        let (scenes, task, _peers) = build_scenes_actor(
            1,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();
        peers.set_scenes(scenes.clone());
        let (cue_lists, task, _cue_lists_peers) =
            build_cue_lists_actor_with_scenes(event_bus.clone(), scenes.clone());
        task.spawn();
        peers.set_cue_lists(cue_lists.clone());
        let new_lv1 = lv1_snapshot(vec![SceneListEntry {
            index: 1,
            name: "Intro".to_string(),
        }]);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let handle = crate::lv1::test_actor_handle(tx);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(new_lv1.clone());
                }
            }
        });
        peers.set_lv1(0, handle);

        let path = std::env::temp_dir().join(format!("show-load-cue-{}.ascs", Uuid::new_v4()));
        let cue_list_id = Uuid::new_v4();
        let cue_entry_id = Uuid::new_v4();
        let missing_scene_id = Uuid::new_v4();
        let file = crate::show::show_file::ShowFile {
            schema_version: crate::show::SHOW_FILE_SCHEMA_VERSION,
            app_version: "test".to_string(),
            saved_at: "123".to_string(),
            safety: crate::show::show_file::ShowFileSafety { lockout: false },
            scene_configs: vec![file_scene(scene_config(1, Some(1), "Intro", 1_000))],
            cue_lists: vec![crate::cue_lists::CueList {
                id: cue_list_id,
                name: "Main".to_string(),
                entries: vec![crate::cue_lists::CueEntry {
                    id: cue_entry_id,
                    scene_internal_id: missing_scene_id,
                }],
            }],
            active_cue_list_id: Some(cue_list_id),
            cued_cue_entry_id: Some(cue_entry_id),
        };
        crate::show_file::write_show_file(&path, &file, &crate::show_file::backup_folder())
            .unwrap();

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::LoadShowFileFromPath {
            path: path.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();

        assert!(rx.await.unwrap().is_ok());

        let state = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            recv_file_metadata_event(&mut events),
        )
        .await
        .unwrap();
        assert!(state.show_file_dirty);
        let cue_document = get_cue_list_document(&cue_lists).await;
        assert_eq!(cue_document.cued_cue_entry_id, None);
    }

    #[tokio::test]
    async fn persisted_scene_edit_marks_show_file_dirty_but_file_replacement_does_not() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = ShowState::default();

        super::handle_app_event(
            crate::runtime::events::AppEvent::Scenes {
                generation: 1,
                event: crate::scenes::ScenesEvent::StateChanged {
                    reason: crate::scenes::ScenesProjectionReason::SceneState,
                    state: crate::scenes::ScenesProjectionState {
                        scene_configs: vec![scene_config(1, Some(1), "Intro", 1_000)],
                        selected_scene_internal_id: None,
                        scene_settings_clipboard_available: false,
                        ready_generation: Some(0),
                    },
                    persisted_scene_edit: true,
                },
            },
            &mut 1,
            &mut state,
            &event_bus,
        );

        let dirty_state = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let crate::runtime::events::AppEvent::Show(ShowEvent::StateChanged {
                    reason: ShowProjectionReason::FileMetadata,
                    state,
                }) = events.recv().await.unwrap()
                {
                    break state;
                }
            }
        })
        .await
        .unwrap();
        assert!(dirty_state.show_file_dirty);

        super::handle_app_event(
            crate::runtime::events::AppEvent::Scenes {
                generation: 1,
                event: crate::scenes::ScenesEvent::StateChanged {
                    reason: crate::scenes::ScenesProjectionReason::FileReplacement,
                    state: crate::scenes::ScenesProjectionState {
                        scene_configs: vec![scene_config(1, Some(1), "Intro", 1_000)],
                        selected_scene_internal_id: None,
                        scene_settings_clipboard_available: false,
                        ready_generation: Some(0),
                    },
                    persisted_scene_edit: false,
                },
            },
            &mut 1,
            &mut state,
            &event_bus,
        );

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn new_show_fails_when_scenes_state_is_unavailable() {
        let event_bus = AppEventBus::default();
        let (show, _peers) = show_actor(event_bus.clone());
        let mut events = event_bus.subscribe();

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
            .await
            .unwrap();

        assert_eq!(rx.await.unwrap().unwrap_err(), "LV1 actor is unavailable");
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn new_show_replaces_scenes_state_from_current_lv1_and_marks_metadata_clean() {
        let event_bus = AppEventBus::default();
        let (show, peers) = show_actor(event_bus.clone());
        let mut events = event_bus.subscribe();
        let (scenes, task, _peers) = build_scenes_actor(
            1,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            test_lockout_reader(),
        );
        task.spawn();
        peers.set_scenes(scenes.clone());
        let (cue_lists, task, _cue_lists_peers) =
            build_cue_lists_actor_with_scenes(event_bus.clone(), scenes.clone());
        task.spawn();
        peers.set_cue_lists(cue_lists.clone());
        event_bus.publish(crate::runtime::events::AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 1 },
        ));
        tokio::task::yield_now().await;

        let new_lv1 = lv1_snapshot(vec![
            SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            },
            SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            },
        ]);
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let handle = crate::lv1::test_actor_handle(tx);
        tokio::spawn(async move {
            while let Some(command) = rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(new_lv1.clone());
                }
            }
        });
        peers.set_lv1(0, handle);

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
            .await
            .unwrap();

        assert!(rx.await.unwrap().is_ok());

        let state = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            recv_file_metadata_event(&mut events),
        )
        .await
        .unwrap();
        assert!(!state.show_file_dirty);
        let scene_document = get_scene_document(&scenes).await;
        assert_eq!(scene_document.scene_configs.len(), 2);
        assert_eq!(scene_document.scene_configs[0].scene_name, "Intro");
    }

    #[tokio::test]
    async fn persisted_cue_list_edit_marks_show_file_dirty() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let mut state = ShowState::default();

        super::handle_app_event(
            crate::runtime::events::AppEvent::CueLists(CueListsEvent::StateChanged {
                reason: CueListsProjectionReason::CueListState,
                state: CueListsProjectionState {
                    document: crate::cue_lists::CueListDocument::default(),
                    last_recall_status: None,
                },
                persisted_cue_list_edit: true,
            }),
            &mut 1,
            &mut state,
            &event_bus,
        );

        let dirty_state = tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                if let crate::runtime::events::AppEvent::Show(ShowEvent::StateChanged {
                    reason: ShowProjectionReason::FileMetadata,
                    state,
                }) = events.recv().await.unwrap()
                {
                    break state;
                }
            }
        })
        .await
        .unwrap();

        assert!(dirty_state.show_file_dirty);
    }
}
