use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use crate::runtime::errors::AppCommandError;
use crate::runtime::events::{AppEvent, AppEventBus, RuntimeLifecycleEvent};
use crate::scenes::ScenesEvent;
use crate::scenes::{RecallSceneResult, ScenesCommand, ScenesHandle};
use tokio::sync::{mpsc, oneshot};

use super::{
    CueListsCommand, CueListsCommandResult, CueListsEvent, CueListsHandle,
    CueListsProjectionReason, CueListsProjectionState, CueListsState, CueRecallResult,
};

pub struct CueListsTask {
    event_bus: AppEventBus,
    peers: CueListsPeers,
    command_rx: mpsc::Receiver<CueListsCommand>,
    event_rx: tokio::sync::broadcast::Receiver<AppEvent>,
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

    pub fn scenes(&self) -> Option<ScenesHandle> {
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
    let event_rx = event_bus.subscribe();
    let peers = CueListsPeers::default();
    (
        CueListsHandle::new(command_tx),
        CueListsTask {
            event_bus,
            peers: peers.clone(),
            command_rx,
            event_rx,
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
        mut event_rx,
    } = task;
    let mut state = CueListsState::default();
    let mut active_generation = 0_u64;
    let mut valid_scene_ids = HashSet::new();

    loop {
        tokio::select! {
            command = command_rx.recv() => {
                let Some(command) = command else { break; };
                match command {
                    CueListsCommand::InitialProjectionState { reply } => {
                        let _ = reply.send(projection_state(&state));
                    }
                    CueListsCommand::GetCueListDocument { reply } => {
                        let _ = reply.send(state.document());
                    }
                    CueListsCommand::ReplaceCueListDocument {
                        document,
                        valid_scene_ids,
                        persisted_cue_list_edit,
                        reply,
                    } => {
                        let valid_scene_ids = valid_scene_ids.into_iter().collect::<HashSet<_>>();
                        state.replace_document(document, valid_scene_ids);
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
            event = event_rx.recv() => {
                match event {
                    Ok(AppEvent::Runtime(RuntimeLifecycleEvent::ActiveGenerationChanged { generation })) => {
                        active_generation = generation;
                        valid_scene_ids.clear();
                    }
                    Ok(AppEvent::Scenes { generation, event: ScenesEvent::StateChanged { state: scenes_state, persisted_scene_edit, .. } }) if generation == active_generation => {
                        valid_scene_ids = scenes_state.scene_configs.iter().map(|scene| scene.internal_scene_id).collect();
                        let reconciliation = state.reconcile(valid_scene_ids.iter().copied());
                        if let Some(cleared) = reconciliation.cued_entry_cleared {
                            log_cue_cleared_missing_scene(&cleared);
                            publish_state(&event_bus, &state, CueListsProjectionReason::CueListState, persisted_scene_edit);
                        }
                    }
                    Ok(AppEvent::Scenes { generation, .. }) if generation != active_generation => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                        crate::runtime::events::log_lagged_subscriber("cue_lists", count);
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    _ => {}
                }
            }
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
    if result.as_ref().is_ok()
        && (!publish_only_when_changed || result.as_ref().is_ok_and(|result| result.changed))
    {
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

fn log_cue_cleared_missing_scene(cleared: &crate::cue_lists::state::ClearedCueEntry) {
    tracing::warn!(
        event = "cue_cleared_missing_scene",
        cue_list_id = %cleared.cue_list_id,
        cue_entry_id = %cleared.cue_entry_id,
        scene_internal_id = %cleared.scene_internal_id,
        "Cued entry cleared because its scene is unavailable."
    );
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
    use crate::cue_lists::{CueListDocument, state::ClearedCueEntry};
    use crate::runtime::events::AppEvent;
    use crate::scenes::{RecallSceneResult, SceneConfig, SceneScopeToggles, ScenesCommand};
    use std::sync::Mutex;
    use tokio::sync::{mpsc, oneshot};
    use tracing::dispatcher;
    use tracing::field::{Field, Visit};
    use tracing_subscriber::Layer;
    use tracing_subscriber::layer::Context;
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::{LookupSpan, Registry};
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

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    async fn create_and_cue_entry(
        handle: &CueListsHandle,
        scene_id: Uuid,
    ) -> crate::cue_lists::CueEntry {
        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::CreateCueList {
                name: "Main".to_string(),
                reply: Some(reply),
            })
            .await
            .unwrap();
        let _ = rx.await.unwrap().unwrap();

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

        entry
    }

    async fn current_document(handle: &CueListsHandle) -> CueListDocument {
        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::GetCueListDocument { reply })
            .await
            .unwrap();
        rx.await.unwrap()
    }

    async fn wait_for_cued_entry_id(
        handle: &CueListsHandle,
        expected: Option<Uuid>,
    ) -> CueListDocument {
        for _ in 0..50 {
            let document = current_document(handle).await;
            if document.cued_cue_entry_id == expected {
                return document;
            }
            tokio::task::yield_now().await;
        }
        current_document(handle).await
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct CapturedWarnEvent {
        level: Option<String>,
        event: Option<String>,
        message: Option<String>,
        cue_list_id: Option<String>,
        cue_entry_id: Option<String>,
        scene_internal_id: Option<String>,
    }

    #[derive(Clone, Default)]
    struct CapturedWarnEvents(Arc<Mutex<Vec<CapturedWarnEvent>>>);

    impl<S> Layer<S> for CapturedWarnEvents
    where
        S: tracing::Subscriber,
        S: for<'a> LookupSpan<'a>,
    {
        fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
            let mut visitor = CapturedWarnEvent {
                level: Some(event.metadata().level().as_str().to_string()),
                ..Default::default()
            };
            event.record(&mut visitor);
            self.0.lock().unwrap().push(visitor);
        }
    }

    impl Visit for CapturedWarnEvent {
        fn record_str(&mut self, field: &Field, value: &str) {
            match field.name() {
                "event" => self.event = Some(value.to_string()),
                "message" => self.message = Some(value.to_string()),
                "cue_list_id" => self.cue_list_id = Some(value.to_string()),
                "cue_entry_id" => self.cue_entry_id = Some(value.to_string()),
                "scene_internal_id" => self.scene_internal_id = Some(value.to_string()),
                _ => {}
            }
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let value = format!("{value:?}");
            self.record_str(field, value.trim_matches('"'));
        }
    }

    #[tokio::test]
    async fn active_scene_fact_clears_missing_current_cue_and_publishes_persisted_edit() {
        let event_bus = AppEventBus::default();
        let (handle, task, _) = build_cue_lists_actor(event_bus.clone());
        task.spawn();

        let fixture = create_and_cue_entry(&handle, id(10)).await;

        event_bus.publish_runtime_generation_changed(7);
        event_bus.publish(AppEvent::Scenes {
            generation: 7,
            event: crate::scenes::ScenesEvent::StateChanged {
                reason: crate::scenes::ScenesProjectionReason::SceneState,
                state: crate::scenes::ScenesProjectionState {
                    scene_configs: vec![],
                    selected_scene_internal_id: None,
                },
                persisted_scene_edit: true,
            },
        });

        let document = wait_for_cued_entry_id(&handle, None).await;
        assert_eq!(document.cued_cue_entry_id, None);
        assert_eq!(document.cue_lists[0].entries.len(), 1);
        assert_eq!(document.cue_lists[0].entries[0].id, fixture.id);

        handle.send(CueListsCommand::Shutdown).await.unwrap();
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
            event: crate::scenes::ScenesEvent::StateChanged {
                reason: crate::scenes::ScenesProjectionReason::SceneState,
                state: crate::scenes::ScenesProjectionState {
                    scene_configs: vec![],
                    selected_scene_internal_id: None,
                },
                persisted_scene_edit: true,
            },
        });

        tokio::task::yield_now().await;
        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        assert_eq!(
            rx.await.unwrap().document.cued_cue_entry_id,
            Some(fixture.id)
        );

        handle.send(CueListsCommand::Shutdown).await.unwrap();
    }

    #[test]
    fn invalid_current_cue_logs_clear_warning() {
        let captured = CapturedWarnEvents::default();
        let logs = captured.0.clone();
        let subscriber = Registry::default().with(captured);
        let dispatch = tracing::Dispatch::new(subscriber);

        let cue_list_id = Uuid::from_u128(0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa);
        let cue_entry_id = Uuid::from_u128(0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb);
        let scene_internal_id = Uuid::from_u128(0xcccccccccccccccccccccccccccccccc);

        dispatcher::with_default(&dispatch, || {
            log_cue_cleared_missing_scene(&ClearedCueEntry {
                cue_list_id,
                cue_entry_id,
                scene_internal_id,
            });
        });

        let logs = logs.lock().unwrap();
        assert_eq!(logs.len(), 1);
        assert_eq!(
            logs[0],
            CapturedWarnEvent {
                level: Some("WARN".to_string()),
                event: Some("cue_cleared_missing_scene".to_string()),
                message: Some("Cued entry cleared because its scene is unavailable.".to_string()),
                cue_list_id: Some(cue_list_id.to_string()),
                cue_entry_id: Some(cue_entry_id.to_string()),
                scene_internal_id: Some(scene_internal_id.to_string()),
            }
        );
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
    async fn replacement_document_uses_incoming_scene_ids_to_preserve_valid_cues() {
        let event_bus = AppEventBus::default();
        let (scenes, _rx) = fake_scenes_handle();
        let (handle, task, _peers) = build_cue_lists_actor_with_scenes(event_bus, scenes);
        task.spawn();

        let scene_id = Uuid::from_u128(0x22222222222242228222222222222222);
        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::CreateCueList {
                name: "Main".to_string(),
                reply: Some(reply),
            })
            .await
            .unwrap();
        let cue_list = rx.await.unwrap().unwrap().cue_list.unwrap();

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

        let replacement = super::super::CueListDocument {
            cue_lists: vec![super::super::CueList {
                id: cue_list.id,
                name: "Main Updated".to_string(),
                entries: vec![super::super::CueEntry {
                    id: entry.id,
                    scene_internal_id: scene_id,
                }],
            }],
            active_cue_list_id: Some(cue_list.id),
            cued_cue_entry_id: Some(entry.id),
        };

        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::ReplaceCueListDocument {
                document: replacement,
                valid_scene_ids: vec![scene_id],
                persisted_cue_list_edit: true,
                reply: Some(reply),
            })
            .await
            .unwrap();
        let _ = rx.await.unwrap();

        let (reply, rx) = oneshot::channel();
        handle
            .send(CueListsCommand::GetCueListDocument { reply })
            .await
            .unwrap();
        let document = rx.await.unwrap();

        assert_eq!(document.cue_lists[0].name, "Main Updated");
        assert_eq!(document.cued_cue_entry_id, Some(entry.id));

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
