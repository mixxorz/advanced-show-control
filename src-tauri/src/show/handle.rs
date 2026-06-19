use std::sync::Arc;

use tokio::sync::Mutex;

use crate::lv1::types::{ChannelInfo, SceneListEntry};
use crate::runtime::events::{AppEvent, AppEventBus};

use super::events::{ShowEvent, ShowProjectionReason, ShowProjectionState};
use super::state::ShowState;
use super::types::{SceneConfig, ShowSnapshot};

#[derive(Clone)]
pub struct ShowStateHandle {
    state: Arc<Mutex<ShowState>>,
    event_bus: AppEventBus,
}

impl ShowStateHandle {
    pub fn new_empty(event_bus: AppEventBus) -> Self {
        Self {
            state: Arc::new(Mutex::new(ShowState::default())),
            event_bus,
        }
    }

    fn publish_state_changed(&self, reason: ShowProjectionReason, state: &ShowState) {
        self.event_bus
            .publish(AppEvent::Show(ShowEvent::StateChanged {
                reason,
                state: state.projection_state(),
            }));
    }

    pub async fn get_snapshot(&self) -> ShowSnapshot {
        self.state.lock().await.snapshot()
    }

    pub async fn get_scene_config(&self, scene_id: String) -> Option<SceneConfig> {
        self.state.lock().await.get_scene_config(&scene_id)
    }

    pub async fn cue_scene(&self, scene_id: String) -> Result<bool, String> {
        let mut state = self.state.lock().await;
        let changed = state.cue_scene(&scene_id)?;
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        Ok(changed)
    }

    pub async fn get_lockout(&self) -> bool {
        self.state.lock().await.lockout()
    }

    #[allow(dead_code)] // Used by later lifecycle/projector startup tasks.
    pub(crate) async fn initial_projection_state(&self) -> ShowProjectionState {
        let state = self.state.lock().await;
        state.projection_state()
    }

    pub(crate) async fn command_set_lockout(&self, enabled: bool) -> bool {
        let mut state = self.state.lock().await;
        let changed = state.set_lockout(enabled);
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        changed
    }

    pub async fn set_lockout(&self, enabled: bool) -> bool {
        self.command_set_lockout(enabled).await
    }

    pub async fn set_scene_duration(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().await;
        let changed = state.set_scene_duration_ms(&scene_id, duration_ms)?;
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        Ok(changed)
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().await;
        let changed = state.set_scene_scope_faders_enabled(&scene_id, enabled)?;
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        Ok(changed)
    }

    pub async fn set_scene_scope_pan_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().await;
        let changed = state.set_scene_scope_pan_enabled(&scene_id, enabled)?;
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        Ok(changed)
    }

    pub async fn set_channel_scoped(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().await;
        let changed = state.set_channel_scoped(&scene_id, group, channel, scoped)?;
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        Ok(changed)
    }

    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().await;
        let changed = state.set_all_channels_scoped(&scene_id, scoped)?;
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        Ok(changed)
    }

    pub async fn store_scene_config(
        &self,
        scene_id: String,
        channels: Vec<ChannelInfo>,
    ) -> Result<bool, String> {
        let mut state = self.state.lock().await;
        let changed = state.store_scene_config(&scene_id, &channels)?;
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        Ok(changed)
    }

    pub async fn reconcile_scene_list(&self, scenes: Vec<SceneListEntry>) -> bool {
        let mut state = self.state.lock().await;
        let changed = state.reconcile_scene_fade_configs(&scenes);
        if changed {
            self.publish_state_changed(ShowProjectionReason::ShowState, &state);
        }
        changed
    }

    async fn scene_reconciliation_diagnostic(&self, scenes: Vec<SceneListEntry>) -> String {
        self.state
            .lock()
            .await
            .scene_reconciliation_diagnostic(&scenes)
    }

    pub(crate) async fn handle_lv1_scene_list_changed(&self, scenes: Vec<SceneListEntry>) -> bool {
        let _ = self.scene_reconciliation_diagnostic(scenes.clone()).await;
        self.reconcile_scene_list(scenes).await
    }

    pub async fn replace_snapshot(&self, snapshot: ShowSnapshot) {
        let mut state = self.state.lock().await;
        if state.snapshot() == snapshot {
            return;
        }

        state.replace_snapshot(snapshot);
        self.publish_state_changed(ShowProjectionReason::ShowState, &state);
    }

    pub async fn clear(&self) {
        let mut state = self.state.lock().await;
        if state.snapshot() == ShowSnapshot::empty() {
            return;
        }

        state.clear();
        self.publish_state_changed(ShowProjectionReason::ShowState, &state);
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
                    event: crate::lv1::events::Lv1Event::SceneListChanged(scenes),
                    ..
                }) => {
                    show.handle_lv1_scene_list_changed(scenes).await;
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
    use crate::lv1::types::SceneListEntry;
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::show::events::{ShowEvent, ShowProjectionReason};
    use crate::show::types::{SceneConfig, SceneScopeToggles, ShowSnapshot};

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

        show.command_set_lockout(true).await;

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

        assert!(show.set_lockout(true).await);

        recv_show_event(&mut events, ShowProjectionReason::ShowState).await;
    }

    #[tokio::test]
    async fn no_op_lockout_change_does_not_publish_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        assert!(!show.set_lockout(false).await);

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn replace_snapshot_publishes_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        show.replace_snapshot(ShowSnapshot {
            lockout: true,
            scene_configs: vec![scene_config()],
            cued_scene_id: None,
        })
        .await;

        recv_show_event(&mut events, ShowProjectionReason::ShowState).await;
    }

    #[tokio::test]
    async fn replace_snapshot_with_identical_state_does_not_publish_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);
        let snapshot = show.get_snapshot().await;

        show.replace_snapshot(snapshot).await;

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn clearing_empty_show_does_not_publish_show_event() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        show.clear().await;

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn lv1_scene_list_monitor_reconciles_show_state() {
        let event_bus = AppEventBus::default();
        let mut show_events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        show.replace_snapshot(ShowSnapshot {
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
        })
        .await;
        recv_show_event(&mut show_events, ShowProjectionReason::ShowState).await;

        let monitor = spawn_lv1_scene_list_monitor(show.clone(), event_bus.subscribe());
        event_bus.publish(AppEvent::Lv1 {
            generation: 0,
            event: crate::lv1::events::Lv1Event::SceneListChanged(vec![SceneListEntry {
                index: 1,
                name: "Verse Big".to_string(),
            }]),
        });

        recv_show_event(&mut show_events, ShowProjectionReason::ShowState).await;
        let snapshot = show.get_snapshot().await;
        assert_eq!(snapshot.scene_configs[0].scene_id, "1::Verse Big");
        assert_eq!(snapshot.scene_configs[0].duration_ms, 1_500);
        monitor.abort();
    }
}
