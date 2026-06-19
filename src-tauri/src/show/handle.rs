use std::sync::Arc;

use tokio::sync::Mutex;

use crate::lv1::types::{ChannelInfo, SceneListEntry};
use crate::runtime::events::{AppEvent, AppEventBus};

use super::events::{ShowEvent, ShowSnapshotChange};
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

    fn publish_snapshot_changed(&self, reason: ShowSnapshotChange) {
        self.event_bus
            .publish(AppEvent::Show(ShowEvent::SnapshotChanged { reason }));
    }

    pub async fn get_snapshot(&self) -> ShowSnapshot {
        self.state.lock().await.snapshot()
    }

    pub async fn get_scene_config(&self, scene_id: String) -> Option<SceneConfig> {
        self.state.lock().await.get_scene_config(&scene_id)
    }

    pub async fn cue_scene(&self, scene_id: String) -> Result<bool, String> {
        let changed = self.state.lock().await.cue_scene(&scene_id)?;
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::CueScene);
        }
        Ok(changed)
    }

    pub async fn get_lockout(&self) -> bool {
        self.state.lock().await.lockout
    }

    pub async fn set_lockout(&self, enabled: bool) -> bool {
        let changed = self.state.lock().await.set_lockout(enabled);
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::Lockout);
        }
        changed
    }

    pub async fn set_scene_duration(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<bool, String> {
        let changed = self
            .state
            .lock()
            .await
            .set_scene_duration_ms(&scene_id, duration_ms)?;
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::SceneDuration);
        }
        Ok(changed)
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<bool, String> {
        let changed = self
            .state
            .lock()
            .await
            .set_scene_scope_faders_enabled(&scene_id, enabled)?;
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::SceneScopeFaders);
        }
        Ok(changed)
    }

    pub async fn set_scene_scope_pan_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<bool, String> {
        let changed = self
            .state
            .lock()
            .await
            .set_scene_scope_pan_enabled(&scene_id, enabled)?;
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::SceneScopePan);
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
        let changed = self
            .state
            .lock()
            .await
            .set_channel_scoped(&scene_id, group, channel, scoped)?;
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::ChannelScope);
        }
        Ok(changed)
    }

    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<bool, String> {
        let changed = self
            .state
            .lock()
            .await
            .set_all_channels_scoped(&scene_id, scoped)?;
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::AllChannelsScope);
        }
        Ok(changed)
    }

    pub async fn store_scene_config(
        &self,
        scene_id: String,
        channels: Vec<ChannelInfo>,
    ) -> Result<bool, String> {
        let changed = self
            .state
            .lock()
            .await
            .store_scene_config(&scene_id, &channels)?;
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::StoreSceneConfig);
        }
        Ok(changed)
    }

    pub async fn reconcile_scene_list(&self, scenes: Vec<SceneListEntry>) -> bool {
        let changed = self
            .state
            .lock()
            .await
            .reconcile_scene_fade_configs(&scenes);
        if changed {
            self.publish_snapshot_changed(ShowSnapshotChange::SceneListReconciled);
        }
        changed
    }

    pub async fn scene_reconciliation_diagnostic(&self, scenes: Vec<SceneListEntry>) -> String {
        self.state
            .lock()
            .await
            .scene_reconciliation_diagnostic(&scenes)
    }

    pub async fn replace_snapshot(&self, snapshot: ShowSnapshot) {
        let mut state = self.state.lock().await;
        if state.snapshot() == snapshot {
            return;
        }

        state.replace_snapshot(snapshot);
        drop(state);
        self.publish_snapshot_changed(ShowSnapshotChange::SnapshotReplaced);
    }

    pub async fn clear(&self) {
        let mut state = self.state.lock().await;
        if state.snapshot() == ShowSnapshot::empty() {
            return;
        }

        state.clear();
        drop(state);
        self.publish_snapshot_changed(ShowSnapshotChange::Cleared);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::show::events::{ShowEvent, ShowSnapshotChange};
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
        expected_reason: ShowSnapshotChange,
    ) {
        let event = events.recv().await.unwrap();
        assert!(matches!(
            event,
            AppEvent::Show(ShowEvent::SnapshotChanged { reason }) if reason == expected_reason
        ));
    }

    #[tokio::test]
    async fn set_lockout_publishes_show_event_when_changed() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let show = ShowStateHandle::new_empty(event_bus);

        assert!(show.set_lockout(true).await);

        recv_show_event(&mut events, ShowSnapshotChange::Lockout).await;
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

        recv_show_event(&mut events, ShowSnapshotChange::SnapshotReplaced).await;
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
}
