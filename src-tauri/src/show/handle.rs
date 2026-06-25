use tokio::sync::mpsc;

use crate::runtime::events::AppEventBus;

use super::commands::ShowCommand;

#[derive(Clone)]
pub struct ShowStateHandle {
    tx: mpsc::Sender<ShowCommand>,
}

impl ShowStateHandle {
    pub fn new_empty(event_bus: AppEventBus) -> Self {
        let (handle, task, _peers) = super::actor::build_show_actor(event_bus);
        task.spawn();
        handle
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
    use crate::connection_state::{Lv1SystemIdentity, ReconnectState};
    use crate::lv1::{Lv1Event, SceneListEntry};
    use crate::runtime::events::{AppEvent, AppEventBus, RuntimeLifecycleEvent};
    use crate::scenes::{SceneConfig, SceneScopeToggles};
    use crate::show::events::{ShowEvent, ShowProjectionReason};
    use crate::show::{ShowCommand, ShowDocument};
    use uuid::Uuid;

    fn scene_config(index: Option<i32>, name: &str, internal_scene_id: Uuid) -> SceneConfig {
        SceneConfig {
            internal_scene_id,
            scene_index: index,
            scene_name: name.to_string(),
            duration_ms: 0,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }
    }

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
                assert_eq!(reason, ShowProjectionReason::ShowState);
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

        recv_show_event(&mut events, ShowProjectionReason::ShowState).await;
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
    async fn replace_snapshot_publishes_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        show.send(ShowCommand::ReplaceSnapshotForTest {
            snapshot: ShowDocument {
                lockout: true,
                scene_configs: vec![scene_config(
                    Some(1),
                    "Intro",
                    Uuid::from_u128(0x11111111111141118111111111111111),
                )],
                cued_scene_internal_id: None,
            },
            reply: None,
        })
        .await
        .unwrap();

        recv_show_event(&mut events, ShowProjectionReason::ShowState).await;
    }

    #[tokio::test]
    async fn replace_snapshot_with_identical_state_does_not_publish_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::GetShowDocument { reply })
            .await
            .unwrap();
        let snapshot = rx.await.unwrap();

        show.send(ShowCommand::ReplaceSnapshotForTest {
            snapshot,
            reply: None,
        })
        .await
        .unwrap();

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn clearing_empty_show_does_not_publish_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        show.send(ShowCommand::ClearForTest { reply: None })
            .await
            .unwrap();

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn show_actor_reconciles_active_generation_scene_list_events() {
        let event_bus = AppEventBus::default();
        let mut show_events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        show.send(ShowCommand::ReplaceSnapshotForTest {
            snapshot: ShowDocument {
                lockout: false,
                scene_configs: vec![SceneConfig {
                    internal_scene_id: Uuid::from_u128(0x22222222222242228222222222222222),
                    scene_index: Some(1),
                    scene_name: "Verse".to_string(),
                    duration_ms: 1_500,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                }],
                cued_scene_internal_id: None,
            },
            reply: None,
        })
        .await
        .unwrap();
        recv_show_event(&mut show_events, ShowProjectionReason::ShowState).await;

        event_bus.publish(AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 7 },
        ));
        event_bus.publish(AppEvent::Lv1 {
            generation: 7,
            event: Lv1Event::SceneListChanged(vec![SceneListEntry {
                index: 1,
                name: "Verse Big".to_string(),
            }]),
        });

        recv_show_event(&mut show_events, ShowProjectionReason::ShowState).await;
        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::GetShowDocument { reply })
            .await
            .unwrap();
        let snapshot = rx.await.unwrap();
        assert_eq!(snapshot.scene_configs[0].scene_index, Some(1));
        assert_eq!(snapshot.scene_configs[0].scene_name, "Verse Big");
        assert_eq!(snapshot.scene_configs[0].duration_ms, 1_500);
    }

    #[tokio::test]
    async fn show_actor_ignores_stale_generation_scene_list_events() {
        let event_bus = AppEventBus::default();
        let mut show_events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        show.send(ShowCommand::ReplaceSnapshotForTest {
            snapshot: ShowDocument {
                lockout: false,
                scene_configs: vec![SceneConfig {
                    internal_scene_id: Uuid::from_u128(0x33333333333343338333333333333333),
                    scene_index: Some(1),
                    scene_name: "Verse".to_string(),
                    duration_ms: 1_500,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                }],
                cued_scene_internal_id: None,
            },
            reply: None,
        })
        .await
        .unwrap();
        recv_show_event(&mut show_events, ShowProjectionReason::ShowState).await;

        event_bus.publish(AppEvent::Runtime(
            RuntimeLifecycleEvent::ActiveGenerationChanged { generation: 7 },
        ));
        event_bus.publish(AppEvent::Lv1 {
            generation: 6,
            event: Lv1Event::SceneListChanged(vec![SceneListEntry {
                index: 1,
                name: "Verse Big".to_string(),
            }]),
        });

        tokio::task::yield_now().await;
        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::GetShowDocument { reply })
            .await
            .unwrap();
        let snapshot = rx.await.unwrap();
        assert_eq!(snapshot.scene_configs[0].scene_index, Some(1));
        assert_eq!(snapshot.scene_configs[0].scene_name, "Verse");
        assert_eq!(snapshot.scene_configs[0].duration_ms, 1_500);
    }

    #[tokio::test]
    async fn show_actor_handles_active_generation_lv1_disconnect() {
        let event_bus = AppEventBus::default();
        let mut show_events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());

        let identity = Lv1SystemIdentity {
            uuid: Some("lv1-a".to_string()),
            host: Some("lv1-a.local".to_string()),
            address: "192.0.2.10".to_string(),
            port: 12345,
        };
        show.send(ShowCommand::EstablishConnectedLv1Identity {
            identity,
            reply: None,
        })
        .await
        .unwrap();
        recv_show_event(&mut show_events, ShowProjectionReason::ConnectionMetadata).await;
        show.send(ShowCommand::SetReconnectState {
            reconnect: ReconnectState {
                active: true,
                attempt: 2,
            },
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

        recv_show_event(&mut show_events, ShowProjectionReason::ConnectionMetadata).await;
        let (reply, rx) = tokio::sync::oneshot::channel();
        show.send(ShowCommand::InitialProjectionState { reply })
            .await
            .unwrap();
        let state = rx.await.unwrap();
        assert!(state.connected_lv1_identity.is_none());
        assert_eq!(state.reconnect, ReconnectState::default());
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
        show.send(ShowCommand::EstablishConnectedLv1Identity {
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
