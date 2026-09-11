use super::*;
use crate::runtime::events::{AppEvent, AppEventBus};
use crate::scenes::{
    SceneConfig, SceneDocument, SceneScopeToggles, ScenesCommand, ScenesProjectionReason,
};
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
}

impl Session {
    async fn new() -> Self {
        Self::with_scenes(vec![]).await
    }

    async fn with_scenes(configs: Vec<SceneConfig>) -> Self {
        let events = AppEventBus::default();
        let (_show, task, _peers, lockout) = crate::show::build_show_actor(events.clone());
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
        let (scenes, task, peers) = crate::scenes::build_scenes_actor(
            0,
            crate::runtime::generation::RuntimeGeneration::default(),
            events.clone(),
            events.subscribe(),
            settings_handle,
            settings,
            lockout,
        );
        let cues = task.cue_lists_handle();
        let (fade, _commands) = tokio::sync::mpsc::channel(8);
        peers.set_peers_for_generation(0, crate::lv1::test_actor_handle(lv1_tx), fade);
        task.spawn();
        let session = Self {
            cues,
            scenes,
            events,
            recalls,
            _show,
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
        let (reply, response) = oneshot::channel();
        self.scenes
            .send(ScenesCommand::ReplaceSceneDocument {
                document: SceneDocument {
                    scene_configs: configs,
                    selected_scene_internal_id: None,
                },
                reason: ScenesProjectionReason::SceneState,
                persisted_scene_edit: true,
                reply: Some(reply),
            })
            .await
            .unwrap();
        response.await.unwrap();
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

    async fn document(&self) -> CueListDocument {
        let (reply, response) = oneshot::channel();
        self.cues
            .send(CueListsCommand::GetCueListDocument { reply })
            .await
            .unwrap();
        response.await.unwrap()
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
    session.install(vec![]).await;
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
async fn projected_scene_facts_cannot_mutate_the_owned_cue_document() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    let entry = session.cue(id).await;
    session.events.publish(AppEvent::Scenes {
        generation: 0,
        event: crate::scenes::ScenesEvent::StateChanged {
            reason: ScenesProjectionReason::SceneState,
            state: crate::scenes::ScenesProjectionState {
                scene_configs: vec![],
                selected_scene_internal_id: None,
                scene_settings_clipboard_available: false,
                ready_generation: Some(0),
            },
            persisted_scene_edit: true,
        },
    });
    for _ in 0..32 {
        tokio::task::yield_now().await;
        assert_eq!(session.document().await.cued_cue_entry_id, Some(entry.id));
    }
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

    assert!(matches!(
        events.recv().await.unwrap(),
        AppEvent::CueLists(CueListsEvent::StateChanged {
            persisted_cue_list_edit: true,
            ..
        })
    ));

    handle.send(CueListsCommand::Shutdown).await.unwrap();
}

#[tokio::test]
async fn replacement_document_uses_incoming_scene_ids_to_preserve_valid_cues() {
    let session = Session::new().await;
    let handle = session.cues;

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

    let replacement = super::CueListDocument {
        cue_lists: vec![super::CueList {
            id: cue_list.id,
            name: "Main Updated".to_string(),
            entries: vec![super::CueEntry {
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
async fn replacement_document_marks_persisted_edit_when_invalid_cue_is_cleared() {
    let session = Session::new().await;
    let event_bus = session.events.clone();
    let mut events = event_bus.subscribe();
    let handle = session.cues;

    let scene_id = Uuid::from_u128(0x33333333333343338333333333333333);
    let cue_list_id = Uuid::from_u128(0x44444444444444448444444444444444);
    let cue_entry_id = Uuid::from_u128(0x55555555555545558555555555555555);

    let replacement = super::CueListDocument {
        cue_lists: vec![super::CueList {
            id: cue_list_id,
            name: "Main".to_string(),
            entries: vec![super::CueEntry {
                id: cue_entry_id,
                scene_internal_id: scene_id,
            }],
        }],
        active_cue_list_id: Some(cue_list_id),
        cued_cue_entry_id: Some(cue_entry_id),
    };

    let (reply, rx) = oneshot::channel();
    handle
        .send(CueListsCommand::ReplaceCueListDocument {
            document: replacement,
            valid_scene_ids: vec![],
            persisted_cue_list_edit: false,
            reply: Some(reply),
        })
        .await
        .unwrap();
    let _ = rx.await.unwrap();

    loop {
        if let AppEvent::CueLists(CueListsEvent::StateChanged {
            reason: CueListsProjectionReason::FileReplacement,
            persisted_cue_list_edit,
            state,
        }) = events.recv().await.unwrap()
        {
            assert!(persisted_cue_list_edit);
            assert_eq!(state.document.cued_cue_entry_id, None);
            break;
        }
    }

    handle.send(CueListsCommand::Shutdown).await.unwrap();
}

#[tokio::test]
async fn replacement_document_marks_persisted_edit_when_invalid_active_list_is_cleared() {
    let session = Session::new().await;
    let event_bus = session.events.clone();
    let mut events = event_bus.subscribe();
    let handle = session.cues;

    let cue_list_id = Uuid::from_u128(0x66666666666646668666666666666666);
    let replacement = super::CueListDocument {
        cue_lists: vec![super::CueList {
            id: cue_list_id,
            name: "Main".to_string(),
            entries: Vec::new(),
        }],
        active_cue_list_id: Some(Uuid::from_u128(0x77777777777747778777777777777777)),
        cued_cue_entry_id: None,
    };

    let (reply, rx) = oneshot::channel();
    handle
        .send(CueListsCommand::ReplaceCueListDocument {
            document: replacement,
            valid_scene_ids: vec![],
            persisted_cue_list_edit: false,
            reply: Some(reply),
        })
        .await
        .unwrap();
    let _ = rx.await.unwrap();

    loop {
        if let AppEvent::CueLists(CueListsEvent::StateChanged {
            reason: CueListsProjectionReason::FileReplacement,
            persisted_cue_list_edit,
            state,
        }) = events.recv().await.unwrap()
        {
            assert!(persisted_cue_list_edit);
            assert_eq!(state.document.active_cue_list_id, None);
            break;
        }
    }

    handle.send(CueListsCommand::Shutdown).await.unwrap();
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
async fn generation_changes_preserve_cue_references_without_persisted_edits() {
    let session = Session::new().await;
    let id = Uuid::new_v4();
    session.install(vec![scene(id)]).await;
    let entry = session.cue(id).await;
    let mut events = session.events.subscribe();
    session.events.publish_runtime_generation_changed(8);
    loop {
        if let AppEvent::Scenes { generation: 8, .. } = events.recv().await.unwrap() {
            break;
        }
    }
    assert_eq!(session.document().await.cued_cue_entry_id, Some(entry.id));
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, AppEvent::CueLists(_)));
    }
}
