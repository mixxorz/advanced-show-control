use crate::cue_lists::*;
use crate::runtime::events::{AppEvent, AppEventBus};
use crate::scenes::{SceneConfig, SceneDocument, SceneScopeToggles, ScenesCommand};

pub(crate) async fn replace_scenes(
    handle: &crate::scenes::ScenesHandle,
    scenes: SceneDocument,
    expected_generation: u64,
) {
    let (reply, response) = oneshot::channel();
    handle
        .send(ScenesCommand::GetSessionDocument { reply })
        .await
        .unwrap();
    let mut document = response.await.unwrap();
    document.scenes = scenes;
    let (reply, response) = oneshot::channel();
    handle
        .send(ScenesCommand::ReplaceSessionDocument {
            replacement: crate::session::SessionReplacement::new(document),
            expected_generation,
            reply,
        })
        .await
        .unwrap();
    response.await.unwrap().unwrap();
}
use tokio::sync::oneshot;
use uuid::Uuid;

fn scene(id: Uuid) -> SceneConfig {
    SceneConfig {
        internal_scene_id: id,
        scene_index: None,
        scene_name: "Intro".into(),
        duration_ms: 1_000,
        channel_configs: vec![],
        scoped_channels: vec![],
        scope_toggles: SceneScopeToggles::default(),
    }
}

struct Session {
    cues: CueListsHandle,
    scenes: crate::scenes::ScenesHandle,
    events: AppEventBus,
    recalls: tokio::sync::mpsc::Receiver<crate::lv1::Lv1Command>,
    _show: crate::show::ShowStateHandle,
    generation: crate::runtime::generation::RuntimeGeneration,
}

impl Session {
    async fn new() -> Self {
        Self::with_scenes(vec![]).await
    }

    async fn with_scenes(configs: Vec<SceneConfig>) -> Self {
        let events = AppEventBus::default();
        let (_show, task, show_peers, lockout) = crate::show::build_show_actor(events.clone());
        task.spawn();
        let settings_dir =
            std::env::temp_dir().join(format!("cue-session-test-{}", Uuid::new_v4()));
        let (settings_handle, task, settings) =
            crate::settings::build_settings_actor(settings_dir, events.clone());
        task.spawn();
        let snapshot = crate::lv1::Lv1StateSnapshot {
            connection: crate::lv1::ConnectionStatus::Connected,
            scene: None,
            scene_list: configs
                .iter()
                .filter_map(|scene| {
                    scene.scene_index.map(|index| crate::lv1::SceneListEntry {
                        index,
                        name: scene.scene_name.clone(),
                    })
                })
                .collect(),
            channels: vec![],
            ping_sequence: 0,
        };
        let initial_scene_list = snapshot.scene_list.clone();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        let (recall_tx, recalls) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::Lv1Command::GetState { reply } => {
                        let _ = reply.send(snapshot.clone());
                    }
                    command => {
                        let _ = recall_tx.send(command).await;
                    }
                }
            }
        });
        let generation = crate::runtime::generation::RuntimeGeneration::default();
        let (scenes, task, peers) = crate::scenes::build_scenes_actor(
            0,
            generation.clone(),
            events.clone(),
            events.subscribe(),
            settings_handle,
            settings,
            lockout,
        );
        let cues = task.cue_lists_handle();
        let (fade, _commands) = tokio::sync::mpsc::channel(8);
        let lv1 = crate::lv1::test_actor_handle(lv1_tx);
        peers.set_peers_for_generation(0, lv1.clone(), fade);
        show_peers.set_lv1(0, lv1);
        show_peers.set_scenes(scenes.clone());
        task.spawn();
        let session = Self {
            cues,
            scenes,
            events,
            recalls,
            _show,
            generation,
        };
        session.install(configs).await;
        let (reply, response) = oneshot::channel();
        session
            .scenes
            .send(ScenesCommand::RuntimePeersReady {
                generation: 0,
                initial_scene_list,
                reply,
            })
            .await
            .unwrap();
        response.await.unwrap().unwrap();
        session
    }

    async fn install(&self, configs: Vec<SceneConfig>) {
        replace_scenes(
            &self.scenes,
            SceneDocument {
                scene_configs: configs,
                selected_scene_internal_id: None,
            },
            0,
        )
        .await;
    }

    async fn cue(&self, scene_internal_id: Uuid) -> CueEntry {
        let (reply, response) = oneshot::channel();
        self.cues
            .send(CueListsCommand::CreateCueList {
                name: "Main".into(),
                reply: Some(reply),
            })
            .await
            .unwrap();
        response.await.unwrap().unwrap();
        let (reply, response) = oneshot::channel();
        self.cues
            .send(CueListsCommand::AddSceneToActiveCueList {
                scene_internal_id,
                insert_index: 0,
                reply: Some(reply),
            })
            .await
            .unwrap();
        let entry = response.await.unwrap().unwrap().entry.unwrap();
        let (reply, response) = oneshot::channel();
        self.cues
            .send(CueListsCommand::CueEntry {
                cue_entry_id: Some(entry.id),
                reply: Some(reply),
            })
            .await
            .unwrap();
        response.await.unwrap().unwrap();
        entry
    }

    async fn snapshot(&self) -> crate::session::SessionDocument {
        let (reply, response) = oneshot::channel();
        self.scenes
            .send(ScenesCommand::GetSessionDocument { reply })
            .await
            .unwrap();
        response.await.unwrap()
    }

    async fn replace(
        &self,
        document: crate::session::SessionDocument,
        expected_generation: u64,
    ) -> (
        crate::session::SessionReplacement,
        oneshot::Receiver<Result<crate::session::SessionDocument, String>>,
    ) {
        let replacement = crate::session::SessionReplacement::new(document);
        let (reply, response) = oneshot::channel();
        self.scenes
            .send(ScenesCommand::ReplaceSessionDocument {
                replacement: replacement.clone(),
                expected_generation,
                reply,
            })
            .await
            .unwrap();
        (replacement, response)
    }

    async fn document(&self) -> CueListDocument {
        let (reply, response) = oneshot::channel();
        self.cues
            .send(CueListsCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        response.await.unwrap().document
    }
}

#[tokio::test]
async fn deleting_a_scene_reconciles_its_cue_before_the_next_document_read() {
    let capture = crate::test_support::TracingCapture::new();
    let _guard = capture.install();
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    let entry = session.cue(id).await;
    let (reply, response) = oneshot::channel();
    session
        .scenes
        .send(ScenesCommand::DeleteSceneConfig {
            internal_scene_id: id,
            reply: Some(reply),
        })
        .await
        .unwrap();
    response.await.unwrap().unwrap();
    let document = session.document().await;
    assert_eq!(document.cued_cue_entry_id, None);
    assert_eq!(document.cue_lists[0].entries, vec![entry.clone()]);
    let logs = capture.matching("cue_cleared_missing_scene", tracing::Level::WARN);
    assert_eq!(logs.len(), 1);
    assert_eq!(
        logs[0].message.as_deref(),
        Some("Cued entry cleared because its scene is unavailable.")
    );
    assert_eq!(
        logs[0].fields.get("scene_internal_id"),
        Some(&id.to_string())
    );
    assert_eq!(
        logs[0].fields.get("cue_entry_id"),
        Some(&entry.id.to_string())
    );
    assert_eq!(
        logs[0].fields.get("cue_list_id"),
        Some(&document.cue_lists[0].id.to_string())
    );
}

#[tokio::test]
async fn projected_facts_and_generation_changes_cannot_mutate_the_owned_cue_document() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    let entry = session.cue(id).await;
    let mut events = session.events.subscribe();
    session.events.publish(AppEvent::Scenes {
        generation: 0,
        event: crate::scenes::ScenesEvent::StateChanged {
            state: crate::scenes::ScenesProjectionState {
                scene_configs: vec![],
                selected_scene_internal_id: None,
                scene_settings_clipboard_available: false,
                ready_generation: Some(0),
            },
            persisted_scene_edit: true,
        },
    });
    session.events.publish_runtime_generation_changed(8);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            let event = events.recv().await.unwrap();
            assert!(!matches!(event, AppEvent::CueLists(_)));
            if matches!(event, AppEvent::Scenes { generation: 8, .. }) {
                break;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(session.document().await.cued_cue_entry_id, Some(entry.id));
}

#[tokio::test]
async fn command_mutation_publishes_persisted_cue_list_edit() {
    let session = Session::new().await;
    let event_bus = session.events.clone();
    let mut events = event_bus.subscribe();
    let handle = session.cues;

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
        if let AppEvent::CueLists(state) = events.recv().await.unwrap() {
            assert_eq!(state.document.cue_lists[0].name, "Main");
            break;
        }
    }

    handle.send(CueListsCommand::Shutdown).await.unwrap();
}

#[tokio::test]
async fn set_active_cue_list_publishes_persisted_cue_list_edit() {
    let session = Session::new().await;
    let event_bus = session.events.clone();
    let mut events = event_bus.subscribe();
    let handle = session.cues;

    let (reply, rx) = oneshot::channel();
    handle
        .send(CueListsCommand::CreateCueList {
            name: "Main".to_string(),
            reply: Some(reply),
        })
        .await
        .unwrap();
    rx.await.unwrap().unwrap();
    while events.try_recv().is_ok() {}

    let (reply, rx) = oneshot::channel();
    handle
        .send(CueListsCommand::SetActiveCueList {
            cue_list_id: None,
            reply: Some(reply),
        })
        .await
        .unwrap();
    assert!(rx.await.unwrap().unwrap().changed);

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if let AppEvent::CueLists(state) = events.recv().await.unwrap() {
                assert!(state.document.active_cue_list_id.is_none());
                break;
            }
        }
    })
    .await
    .unwrap();

    handle.send(CueListsCommand::Shutdown).await.unwrap();
}

#[tokio::test]
async fn session_replacement_preserves_valid_cue_references() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    let entry = session.cue(id).await;
    let mut document = session.snapshot().await;
    document.cue_lists.cue_lists[0].name = "Updated".into();
    let (_, response) = session.replace(document.clone(), 0).await;
    assert_eq!(response.await.unwrap().unwrap(), document);
    assert_eq!(session.document().await.cued_cue_entry_id, Some(entry.id));
}

#[tokio::test]
async fn session_replacement_clears_invalid_active_list() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    session.cue(id).await;
    let mut document = session.snapshot().await;
    document.cue_lists.active_cue_list_id = Some(Uuid::new_v4());
    let (_, response) = session.replace(document, 0).await;
    let document = response.await.unwrap().unwrap();
    assert_eq!(document.cue_lists.active_cue_list_id, None);
    assert_eq!(document.cue_lists.cued_cue_entry_id, None);
    assert_eq!(document.cue_lists.cue_lists.len(), 1);
    assert_eq!(session.snapshot().await, document);
}

#[tokio::test]
async fn cue_advances_only_after_successful_lv1_dispatch() {
    for succeeds in [true, false] {
        let id = Uuid::new_v4();
        let mut config = scene(id);
        config.scene_index = Some(1);
        let mut session = Session::with_scenes(vec![config]).await;
        let entry = session.cue(id).await;
        let (reply, response) = oneshot::channel();
        session
            .cues
            .send(CueListsCommand::AddSceneToActiveCueList {
                scene_internal_id: id,
                insert_index: 1,
                reply: Some(reply),
            })
            .await
            .unwrap();
        let next = response.await.unwrap().unwrap().entry.unwrap();
        let (reply, mut response) = oneshot::channel();
        session
            .cues
            .send(CueListsCommand::RecallCuedCue { reply })
            .await
            .unwrap();
        let crate::lv1::Lv1Command::RecallScene {
            scene_index,
            reply: Some(reply),
        } = session.recalls.recv().await.unwrap()
        else {
            panic!("expected LV1 recall");
        };
        assert_eq!(scene_index, 1);
        assert_eq!(
            response.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        );
        reply
            .send(if succeeds {
                Ok(crate::lv1::RecallSceneDispatch {
                    scene_observation_sequence: 0,
                })
            } else {
                Err(crate::lv1::Lv1ActorError::NotConnected)
            })
            .unwrap();
        let result = response.await.unwrap();
        if succeeds {
            assert_eq!(
                result.unwrap(),
                CueRecallResult {
                    recalled_entry_id: entry.id,
                    next_cued_entry_id: Some(next.id)
                }
            );
        } else {
            assert!(result.is_err());
        }
        assert_eq!(
            session.document().await.cued_cue_entry_id,
            Some(if succeeds { next.id } else { entry.id })
        );
    }
}

#[tokio::test]
async fn session_replacement_returns_one_reconciled_document() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    let entry = session.cue(id).await;
    let original = session.snapshot().await;
    assert_eq!(original.scenes.scene_configs, vec![scene(id)]);
    assert_eq!(original.cue_lists.cued_cue_entry_id, Some(entry.id));

    let mut next = original.clone();
    next.scenes.scene_configs.clear();
    let (replacement, response) = session.replace(next, 0).await;
    let committed = response.await.unwrap().unwrap();
    assert!(committed.scenes.scene_configs.is_empty());
    assert_eq!(committed.cue_lists.cued_cue_entry_id, None);
    assert_eq!(
        committed.cue_lists.cue_lists[0].entries,
        original.cue_lists.cue_lists[0].entries
    );
    assert_eq!(session.snapshot().await, committed);
    assert_eq!(replacement.cancel_or_committed().unwrap(), committed);
}

#[tokio::test]
async fn timed_out_new_session_leaves_documents_intact_and_releases_show_commands() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    session.cue(id).await;
    let original = session.snapshot().await;
    let gate = session.generation.hold_for_test().await;
    let (reply, response) = oneshot::channel();
    session
        ._show
        .send(crate::show::ShowCommand::NewShowFileFromCurrentLv1 { reply: Some(reply) })
        .await
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(1), response)
        .await
        .unwrap()
        .unwrap();
    assert!(result.unwrap_err().contains("timed out"));
    let (reply, response) = oneshot::channel();
    session
        ._show
        .send(crate::show::ShowCommand::SetLockout {
            enabled: true,
            reply: Some(reply),
        })
        .await
        .unwrap();
    assert!(response.await.unwrap().changed);
    drop(gate);
    assert_eq!(session.snapshot().await, original);
}

#[tokio::test]
async fn replacement_queued_during_recall_dispatch_preserves_the_replacement_document() {
    let id = Uuid::new_v4();
    let mut config = scene(id);
    config.scene_index = Some(1);
    let mut session = Session::with_scenes(vec![config]).await;
    session.cue(id).await;
    let original = session.snapshot().await;
    let (reply, recalled) = oneshot::channel();
    session
        .cues
        .send(CueListsCommand::RecallCuedCue { reply })
        .await
        .unwrap();
    let crate::lv1::Lv1Command::RecallScene {
        reply: Some(dispatch),
        ..
    } = session.recalls.recv().await.unwrap()
    else {
        panic!("expected recall");
    };
    let (_, replaced) = session.replace(original.clone(), 0).await;
    dispatch
        .send(Ok(crate::lv1::RecallSceneDispatch {
            scene_observation_sequence: 0,
        }))
        .unwrap();
    assert_eq!(replaced.await.unwrap().unwrap(), original);
    let _ = recalled.await.unwrap();
    assert_eq!(session.snapshot().await, original);
}

#[tokio::test]
async fn stale_session_replacement_changes_neither_document() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    session.cue(id).await;
    let original = session.snapshot().await;
    let mut next = original.clone();
    next.scenes.scene_configs.clear();
    next.cue_lists.cue_lists.clear();
    let (_, response) = session.replace(next, 1).await;
    assert!(response.await.unwrap().is_err());
    assert_eq!(session.snapshot().await, original);
}

#[tokio::test]
async fn canceled_session_replacement_cannot_commit_after_generation_gate_releases() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    session.cue(id).await;
    let original = session.snapshot().await;
    let gate = session.generation.hold_for_test().await;
    let next = crate::session::SessionDocument {
        scenes: SceneDocument::empty(),
        cue_lists: CueListDocument::default(),
    };
    let (replacement, response) = session.replace(next, 0).await;
    assert!(replacement.cancel_or_committed().is_err());
    drop(gate);
    assert!(response.await.unwrap().is_err());
    assert_eq!(session.snapshot().await, original);
}
