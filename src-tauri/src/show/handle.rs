use tokio::sync::mpsc;

use crate::runtime::events::AppEventBus;

use super::commands::ShowCommand;

#[derive(Clone)]
pub struct ShowStateHandle {
    tx: mpsc::Sender<ShowCommand>,
}

impl ShowStateHandle {
    pub fn new_empty(event_bus: AppEventBus) -> Self {
        let (handle, task, _peers, _lockout) = super::actor::build_show_actor(event_bus);
        task.spawn();
        handle
    }

    #[cfg(test)]
    pub(crate) fn new_stalled() -> Self {
        let (tx, rx) = mpsc::channel(1);
        tokio::spawn(async move {
            let _receiver = rx;
            std::future::pending::<()>().await;
        });
        Self { tx }
    }

    pub(super) fn new(tx: mpsc::Sender<ShowCommand>) -> Self {
        Self { tx }
    }

    pub async fn send(
        &self,
        command: ShowCommand,
    ) -> Result<(), mpsc::error::SendError<ShowCommand>> {
        self.tx.send(command).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_state::Lv1SystemIdentity;
    use crate::lv1::{ConnectionStatus, Lv1Event, Lv1StateSnapshot, SceneListEntry};
    use crate::runtime::events::{AppEvent, AppEventBus, RuntimeLifecycleEvent};
    use crate::runtime::generation::RuntimeGeneration;
    use crate::scenes::build_scenes_actor;
    use crate::settings::{AppSettings, SettingsCommand, SettingsHandle};
    use crate::show::events::{ShowEvent, ShowProjectionReason};
    use crate::show::{ShowCommand, ShowCommandResult, ShowFile, ShowFileSafety};

    async fn recv_show_event(
        events: &mut tokio::sync::broadcast::Receiver<AppEvent>,
        expected_reason: ShowProjectionReason,
    ) {
        loop {
            let event = events.recv().await.unwrap();
            if matches!(
                event,
                AppEvent::Show(ShowEvent::StateChanged { reason, .. }) if reason == expected_reason
            ) {
                break;
            }
        }
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

    #[tokio::test]
    async fn lockout_reader_tracks_the_latest_show_owned_value() {
        let event_bus = AppEventBus::default();
        let (show, task, _peers, mut lockout) = super::super::actor::build_show_actor(event_bus);
        task.spawn();

        assert!(!lockout.current());
        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::SetLockout {
            enabled: true,
            reply: Some(reply),
        })
        .await
        .unwrap();
        rx.await.unwrap();

        assert!(lockout.changed().await.unwrap());
        assert!(lockout.current());
    }

    #[tokio::test]
    async fn lockout_reader_tracks_imported_show_file_lockout() {
        let event_bus = AppEventBus::default();
        let (show, task, peers, mut lockout) =
            super::super::actor::build_show_actor(event_bus.clone());
        let (scenes, scenes_task, _scenes_peers) = build_scenes_actor(
            0,
            RuntimeGeneration::default(),
            event_bus.clone(),
            event_bus.subscribe(),
            fake_settings_handle(),
            AppSettings::default(),
            lockout.clone(),
        );
        peers.set_scenes(scenes);
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                if let crate::lv1::Lv1Command::GetState { reply } = command {
                    let _ = reply.send(Lv1StateSnapshot {
                        connection: ConnectionStatus::Connected,
                        scene: None,
                        scene_list: vec![SceneListEntry {
                            index: 1,
                            name: "Intro".to_string(),
                        }],
                        channels: Vec::new(),
                        ping_sequence: 0,
                    });
                }
            }
        });
        peers.set_lv1(0, crate::lv1::test_actor_handle(lv1_tx));
        task.spawn();
        scenes_task.spawn();

        let path = std::env::temp_dir().join(format!("show-lockout-{}.ascs", uuid::Uuid::new_v4()));
        crate::show_file::write_show_file(
            &path,
            &ShowFile {
                schema_version: crate::show::SHOW_FILE_SCHEMA_VERSION,
                app_version: "test".to_string(),
                saved_at: "123".to_string(),
                safety: ShowFileSafety { lockout: true },
                scene_configs: Vec::new(),
                cue_lists: Vec::new(),
                active_cue_list_id: None,
                cued_cue_entry_id: None,
            },
            &crate::show_file::backup_folder(),
        )
        .unwrap();

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::LoadShowFileFromPath {
            path: path.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();
        let result = rx.await.unwrap();
        assert!(result.is_ok(), "show import failed: {result:?}");

        assert!(lockout.changed().await.unwrap());
        assert!(lockout.current());
        std::fs::remove_file(path).unwrap();
    }

    #[tokio::test]
    async fn show_event_carries_full_projection_state() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        show.send(ShowCommand::SetLockout {
            enabled: true,
            reply: None,
        })
        .await
        .unwrap();

        let event = events.recv().await.unwrap();
        match event {
            AppEvent::Show(ShowEvent::StateChanged { reason, state }) => {
                assert_eq!(reason, ShowProjectionReason::FileMetadata);
                assert!(state.lockout);
                assert_eq!(state.show_file_name, "Untitled Session");
                assert!(!state.show_file_dirty);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn set_lockout_publishes_show_event_when_changed() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::SetLockout {
            enabled: true,
            reply: Some(reply),
        })
        .await
        .unwrap();
        assert!(rx.await.unwrap().changed);

        recv_show_event(&mut events, ShowProjectionReason::FileMetadata).await;
    }

    #[tokio::test]
    async fn no_op_lockout_change_does_not_publish_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::SetLockout {
            enabled: false,
            reply: Some(reply),
        })
        .await
        .unwrap();
        assert!(!rx.await.unwrap().changed);

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn complete_connection_publishes_one_full_projection_and_noop_publishes_none() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50_000,
        };

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::CompleteLv1Connection {
            identity: identity.clone(),
            reply: Some(reply),
        })
        .await
        .unwrap();
        assert_eq!(
            rx.await.unwrap(),
            crate::show::CompleteConnectionOutcome {
                accepted: true,
                changed: true,
            }
        );

        let AppEvent::Show(ShowEvent::StateChanged { reason, state }) =
            events.recv().await.unwrap()
        else {
            panic!("expected Show projection");
        };
        assert_eq!(reason, ShowProjectionReason::ConnectionMetadata);
        assert_eq!(state.connected_lv1_identity, Some(identity.clone()));
        assert!(events.try_recv().is_err());

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::CompleteLv1Connection {
            identity,
            reply: Some(reply),
        })
        .await
        .unwrap();
        assert_eq!(
            rx.await.unwrap(),
            crate::show::CompleteConnectionOutcome {
                accepted: true,
                changed: false,
            }
        );
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn failed_connection_clears_connected_identity_with_one_projection() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        let identity = Lv1SystemIdentity {
            uuid: Some("uuid-1".to_string()),
            host: Some("LV1-FOH".to_string()),
            address: "192.168.1.35".to_string(),
            port: 50_000,
        };
        show.send(ShowCommand::CompleteLv1Connection {
            identity,
            reply: None,
        })
        .await
        .unwrap();
        recv_show_event(&mut events, ShowProjectionReason::ConnectionMetadata).await;

        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::FailLv1Connection { reply: Some(reply) })
            .await
            .unwrap();
        assert_eq!(rx.await.unwrap(), ShowCommandResult { changed: true });
        let AppEvent::Show(ShowEvent::StateChanged { state, .. }) = events.recv().await.unwrap()
        else {
            panic!("expected Show projection");
        };
        assert_eq!(state.connected_lv1_identity, None);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn show_actor_preserves_identity_across_active_generation_lv1_disconnect() {
        let event_bus = AppEventBus::default();
        let mut show_events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());

        let identity = Lv1SystemIdentity {
            uuid: Some("lv1-a".to_string()),
            host: Some("lv1-a.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 12345,
        };
        show.send(ShowCommand::CompleteLv1Connection {
            identity: identity.clone(),
            reply: None,
        })
        .await
        .unwrap();
        recv_show_event(&mut show_events, ShowProjectionReason::ConnectionMetadata).await;

        event_bus.publish(AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 7 },
        ));
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::Disconnected {
                reason: "network lost".to_string(),
            },
        });

        tokio::task::yield_now().await;
        while let Ok(event) = show_events.try_recv() {
            assert!(!matches!(event, AppEvent::Show(_)));
        }
        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        let state = rx.await.unwrap();
        assert_eq!(state.connected_lv1_identity, Some(identity));
        assert!(show_events.try_recv().is_err());
    }

    #[tokio::test]
    async fn show_actor_ignores_stale_generation_lv1_disconnect() {
        let event_bus = AppEventBus::default();
        let mut show_events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());

        let identity = Lv1SystemIdentity {
            uuid: Some("lv1-a".to_string()),
            host: Some("lv1-a.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 12345,
        };
        show.send(ShowCommand::CompleteLv1Connection {
            identity,
            reply: None,
        })
        .await
        .unwrap();
        recv_show_event(&mut show_events, ShowProjectionReason::ConnectionMetadata).await;

        event_bus.publish(AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 7 },
        ));
        event_bus.publish(AppEvent::Lv1 {
            generation: 6,
            event: Lv1Event::Disconnected {
                reason: "old runtime closed".to_string(),
            },
        });

        tokio::task::yield_now().await;
        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        let state = rx.await.unwrap();
        assert!(state.connected_lv1_identity.is_some());
    }
}
