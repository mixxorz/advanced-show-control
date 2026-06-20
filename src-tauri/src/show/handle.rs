use tokio::sync::mpsc;

use crate::lv1::Lv1Event;
use crate::runtime::events::{AppEvent, AppEventBus};

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

pub fn spawn_lv1_scene_list_monitor(
    show: ShowStateHandle,
    mut events: tokio::sync::broadcast::Receiver<AppEvent>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            match events.recv().await {
                Ok(AppEvent::Lv1 {
                    event: Lv1Event::SceneListChanged(scenes),
                    ..
                }) => {
                    let _ = show
                        .send(ShowCommand::ReconcileSceneList {
                            scenes,
                            reply: None,
                        })
                        .await;
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(count)) => {
                    crate::runtime::events::log_lagged_subscriber("show-scene-list-monitor", count);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{Lv1Event, SceneListEntry};
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::show::ShowCommand;
    use crate::show::events::{ShowEvent, ShowProjectionReason};
    use crate::show::types::{SceneConfig, SceneScopeToggles, ShowDocument};

    fn scene_config() -> SceneConfig {
        SceneConfig {
            scene_id: "1:Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
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
                assert_eq!(state.show_file_name, "Untitled Show");
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
                scene_configs: vec![scene_config()],
                cued_scene_id: None,
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
    async fn lv1_scene_list_monitor_reconciles_show_state() {
        let event_bus = AppEventBus::default();
        let mut show_events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        show.send(ShowCommand::ReplaceSnapshotForTest {
            snapshot: ShowDocument {
                lockout: false,
                scene_configs: vec![SceneConfig {
                    scene_id: "1::Verse".to_string(),
                    scene_index: 1,
                    scene_name: "Verse".to_string(),
                    duration_ms: 1_500,
                    channel_configs: Vec::new(),
                    scoped_channels: Vec::new(),
                    scope_toggles: SceneScopeToggles::default(),
                }],
                cued_scene_id: None,
            },
            reply: None,
        })
        .await
        .unwrap();
        recv_show_event(&mut show_events, ShowProjectionReason::ShowState).await;

        let monitor = spawn_lv1_scene_list_monitor(show.clone(), event_bus.subscribe());
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
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
        assert_eq!(snapshot.scene_configs[0].scene_id, "1::Verse Big");
        assert_eq!(snapshot.scene_configs[0].duration_ms, 1_500);
        monitor.abort();
    }
}
