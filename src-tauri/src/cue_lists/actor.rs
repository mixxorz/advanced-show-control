use std::sync::{Arc, Mutex};

use tokio::sync::{mpsc, oneshot};

use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus};
use crate::scenes::{RecallSceneResult, ScenesCommand, ScenesHandle};

use super::{
    CueListsCommand, CueListsCommandResult, CueListsEvent, CueListsHandle,
    CueListsProjectionReason, CueListsProjectionState, CueListsState, CueRecallResult,
};

pub struct CueListsTask {
    event_bus: AppEventBus,
    peers: CueListsPeers,
    command_rx: mpsc::Receiver<CueListsCommand>,
}

#[derive(Clone, Default)]
pub struct CueListsPeers {
    scenes: Arc<Mutex<Option<ScenesHandle>>>,
}

impl CueListsPeers {
    pub fn set_scenes(&self, scenes: ScenesHandle) {
        *self.scenes.lock().expect("cue lists peers lock poisoned") = Some(scenes);
    }

    pub fn clear_scenes(&self) {
        *self.scenes.lock().expect("cue lists peers lock poisoned") = None;
    }

    fn scenes(&self) -> Option<ScenesHandle> {
        self.scenes
            .lock()
            .expect("cue lists peers lock poisoned")
            .clone()
    }
}

impl CueListsTask {
    pub fn spawn(self) {
        tauri::async_runtime::spawn(run_cue_lists_actor(self));
    }
}

pub fn build_cue_lists_actor(
    event_bus: AppEventBus,
) -> (CueListsHandle, CueListsTask, CueListsPeers) {
    let (command_tx, command_rx) = mpsc::channel(8);
    let peers = CueListsPeers::default();
    (
        CueListsHandle::new(command_tx),
        CueListsTask {
            event_bus,
            peers: peers.clone(),
            command_rx,
        },
        peers,
    )
}

#[cfg(test)]
pub fn build_cue_lists_actor_with_scenes(
    event_bus: AppEventBus,
    scenes: ScenesHandle,
) -> (CueListsHandle, CueListsTask, CueListsPeers) {
    let (handle, task, peers) = build_cue_lists_actor(event_bus);
    peers.set_scenes(scenes);
    (handle, task, peers)
}

async fn run_cue_lists_actor(task: CueListsTask) {
    let CueListsTask {
        event_bus,
        peers,
        mut command_rx,
    } = task;
    let mut state = CueListsState::default();

    while let Some(command) = command_rx.recv().await {
        match command {
            CueListsCommand::InitialProjectionState { reply } => {
                let _ = reply.send(projection_state(&state));
            }
            CueListsCommand::GetCueListDocument { reply } => {
                let _ = reply.send(state.document());
            }
            CueListsCommand::ReplaceCueListDocument {
                document,
                persisted_cue_list_edit,
                reply,
            } => {
                state.replace_document(document, std::iter::empty());
                publish_state(
                    &event_bus,
                    &state,
                    CueListsProjectionReason::FileReplacement,
                    persisted_cue_list_edit,
                );
                if let Some(reply) = reply {
                    let _ = reply.send(CueListsCommandResult {
                        changed: true,
                        cue_list: None,
                        entry: None,
                    });
                }
            }
            CueListsCommand::CreateCueList { name, reply } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                true,
                false,
                |state| {
                    state
                        .create_cue_list(name)
                        .map(|cue_list| CueListsCommandResult {
                            changed: true,
                            cue_list: Some(cue_list),
                            entry: None,
                        })
                },
            ),
            CueListsCommand::RenameCueList {
                cue_list_id,
                name,
                reply,
            } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                true,
                false,
                |state| {
                    state
                        .rename_cue_list(cue_list_id, name)
                        .map(|_| CueListsCommandResult {
                            changed: true,
                            cue_list: None,
                            entry: None,
                        })
                },
            ),
            CueListsCommand::DeleteCueList { cue_list_id, reply } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                true,
                false,
                |state| {
                    state
                        .delete_cue_list(cue_list_id)
                        .map(|_| CueListsCommandResult {
                            changed: true,
                            cue_list: None,
                            entry: None,
                        })
                },
            ),
            CueListsCommand::ReorderCueLists { ordered_ids, reply } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                true,
                false,
                |state| {
                    state
                        .reorder_cue_lists(ordered_ids)
                        .map(|_| CueListsCommandResult {
                            changed: true,
                            cue_list: None,
                            entry: None,
                        })
                },
            ),
            CueListsCommand::SetActiveCueList { cue_list_id, reply } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                false,
                true,
                |state: &mut CueListsState| {
                    state
                        .set_active_cue_list(cue_list_id)
                        .map(|changed| CueListsCommandResult {
                            changed,
                            cue_list: None,
                            entry: None,
                        })
                },
            ),
            CueListsCommand::AddSceneToActiveCueList {
                scene_internal_id,
                insert_index,
                reply,
            } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                true,
                true,
                |state| {
                    state
                        .add_scene_to_active_cue_list(scene_internal_id, insert_index)
                        .map(|entry| CueListsCommandResult {
                            changed: true,
                            cue_list: None,
                            entry: Some(entry),
                        })
                },
            ),
            CueListsCommand::RemoveCueEntry {
                cue_entry_id,
                reply,
            } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                true,
                false,
                |state| {
                    state
                        .remove_cue_entry(cue_entry_id)
                        .map(|_| CueListsCommandResult {
                            changed: true,
                            cue_list: None,
                            entry: None,
                        })
                },
            ),
            CueListsCommand::ReorderCueEntries {
                ordered_entry_ids,
                reply,
            } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                true,
                false,
                |state| {
                    state
                        .reorder_cue_entries(ordered_entry_ids)
                        .map(|_| CueListsCommandResult {
                            changed: true,
                            cue_list: None,
                            entry: None,
                        })
                },
            ),
            CueListsCommand::CueEntry {
                cue_entry_id,
                reply,
            } => respond_mutation(
                reply,
                &event_bus,
                &mut state,
                CueListsProjectionReason::CueListState,
                true,
                false,
                |state| {
                    state
                        .cue_entry(cue_entry_id)
                        .map(|_| CueListsCommandResult {
                            changed: true,
                            cue_list: None,
                            entry: None,
                        })
                },
            ),
            CueListsCommand::RecallCuedCue { reply } => {
                let result = recall_cued_cue(&peers, &mut state).await;
                if result.is_ok() {
                    publish_state(
                        &event_bus,
                        &state,
                        CueListsProjectionReason::CueListState,
                        true,
                    );
                }
                let _ = reply.send(result);
            }
            CueListsCommand::Shutdown => break,
        }
    }
}

fn respond_mutation<F>(
    reply: Option<oneshot::Sender<Result<CueListsCommandResult, String>>>,
    event_bus: &AppEventBus,
    state: &mut CueListsState,
    reason: CueListsProjectionReason,
    persisted_cue_list_edit: bool,
    publish_only_when_changed: bool,
    mutate: F,
) where
    F: FnOnce(&mut CueListsState) -> Result<CueListsCommandResult, String>,
{
    let result = mutate(state);
    if result.as_ref().is_ok() && (!publish_only_when_changed || result.as_ref().unwrap().changed) {
        publish_state(event_bus, state, reason, persisted_cue_list_edit);
    }
    if let Some(reply) = reply {
        let _ = reply.send(result);
    }
}

fn publish_state(
    event_bus: &AppEventBus,
    state: &CueListsState,
    reason: CueListsProjectionReason,
    persisted_cue_list_edit: bool,
) {
    let _ = event_bus.publish(AppEvent::CueLists(CueListsEvent::StateChanged {
        reason,
        state: projection_state(state),
        persisted_cue_list_edit,
    }));
}

fn projection_state(state: &CueListsState) -> CueListsProjectionState {
    CueListsProjectionState {
        document: state.document(),
        last_recall_status: None,
    }
}

async fn recall_cued_cue(
    peers: &CueListsPeers,
    state: &mut CueListsState,
) -> Result<CueRecallResult, AppCommandError> {
    let scenes = peers.scenes().ok_or(AppCommandError::ScenesUnavailable)?;
    let entry = state.cued_entry().map_err(AppCommandError::CommandFailed)?;
    let (reply, rx) = oneshot::channel();
    scenes
        .send(ScenesCommand::RecallScene {
            internal_scene_id: entry.scene_internal_id,
            reply,
        })
        .await
        .map_err(|_| AppCommandError::ScenesUnavailable)?;
    let recalled: RecallSceneResult = rx
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed)??;
    let recalled_entry = state
        .advance_after_successful_recall()
        .map_err(AppCommandError::CommandFailed)?;
    let _ = recalled;
    Ok(CueRecallResult {
        recalled_entry_id: recalled_entry.id,
        next_cued_entry_id: state.document().cued_cue_entry_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::AppEvent;
    use crate::scenes::{RecallSceneResult, SceneConfig, SceneScopeToggles, ScenesCommand};
    use tokio::sync::{mpsc, oneshot};
    use uuid::Uuid;

    fn scene_config(id: Uuid) -> SceneConfig {
        SceneConfig {
            internal_scene_id: id,
            scene_index: Some(1),
            scene_name: "Intro".to_string(),
            duration_ms: 1_000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn fake_scenes_handle() -> (crate::scenes::ScenesHandle, mpsc::Receiver<ScenesCommand>) {
        let (tx, rx) = mpsc::channel(8);
        (crate::scenes::ScenesHandle::new(tx), rx)
    }

    #[tokio::test]
    async fn command_mutation_publishes_persisted_cue_list_edit() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let (scenes, _rx) = fake_scenes_handle();
        let (handle, task, _peers) = build_cue_lists_actor_with_scenes(event_bus.clone(), scenes);
        task.spawn();

        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::CreateCueList {
                name: "Main".to_string(),
                reply: Some(reply),
            })
            .await
            .unwrap();
        let result = rx.await.unwrap().unwrap();
        assert!(result.changed);

        loop {
            if let AppEvent::CueLists(CueListsEvent::StateChanged {
                persisted_cue_list_edit,
                state,
                ..
            }) = events.recv().await.unwrap()
            {
                assert!(persisted_cue_list_edit);
                assert_eq!(state.document.cue_lists[0].name, "Main");
                break;
            }
        }

        handle.send(CueListsCommand::Shutdown).await.unwrap();
    }

    #[tokio::test]
    async fn recall_cued_cue_routes_through_scenes_and_advances_on_success() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let (scenes, mut scene_rx) = fake_scenes_handle();
        let (handle, task, _peers) = build_cue_lists_actor_with_scenes(event_bus, scenes);
        task.spawn();

        let scene_id = Uuid::from_u128(0x11111111111141118111111111111111);
        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::CreateCueList {
                name: "Main".to_string(),
                reply: Some(reply),
            })
            .await
            .unwrap();
        rx.await.unwrap().unwrap();
        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::AddSceneToActiveCueList {
                scene_internal_id: scene_id,
                insert_index: 0,
                reply: Some(reply),
            })
            .await
            .unwrap();
        let entry = rx.await.unwrap().unwrap().entry.unwrap();
        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::CueEntry {
                cue_entry_id: Some(entry.id),
                reply: Some(reply),
            })
            .await
            .unwrap();
        rx.await.unwrap().unwrap();

        let recall_task = tokio::spawn(async move {
            match scene_rx.recv().await.unwrap() {
                ScenesCommand::RecallScene {
                    internal_scene_id,
                    reply,
                } => {
                    assert_eq!(internal_scene_id, scene_id);
                    let _ = reply.send(Ok(RecallSceneResult {
                        scene: scene_config(scene_id),
                        lv1_scene_index: 1,
                    }));
                }
                other => panic!("unexpected command: {other:?}"),
            }
        });

        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::RecallCuedCue { reply })
            .await
            .unwrap();
        let result = rx.await.unwrap().unwrap();
        assert_eq!(result.recalled_entry_id, entry.id);
        assert_eq!(result.next_cued_entry_id, None);
        recall_task.await.unwrap();

        loop {
            if let AppEvent::CueLists(CueListsEvent::StateChanged {
                reason,
                persisted_cue_list_edit,
                state,
            }) = events.recv().await.unwrap()
            {
                assert_eq!(reason, CueListsProjectionReason::CueListState);
                assert!(persisted_cue_list_edit);
                assert!(state.document.cued_cue_entry_id.is_none());
                break;
            }
        }

        handle.send(CueListsCommand::Shutdown).await.unwrap();
    }
}
