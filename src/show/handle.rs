use std::sync::Arc;

use tokio::sync::Mutex;

use crate::lv1::types::{ChannelInfo, SceneListEntry};

use super::state::ShowState;
use super::types::{SceneConfig, ShowSnapshot};

#[derive(Clone)]
pub struct ShowStateHandle {
    state: Arc<Mutex<ShowState>>,
}

impl ShowStateHandle {
    pub fn new_empty() -> Self {
        Self {
            state: Arc::new(Mutex::new(ShowState::default())),
        }
    }

    pub async fn get_snapshot(&self) -> ShowSnapshot {
        self.state.lock().await.snapshot()
    }

    pub async fn get_scene_config(&self, scene_id: String) -> Option<SceneConfig> {
        self.state.lock().await.get_scene_config(&scene_id)
    }

    pub async fn get_lockout(&self) -> bool {
        self.state.lock().await.lockout
    }

    pub async fn set_lockout(&self, enabled: bool) -> bool {
        self.state.lock().await.set_lockout(enabled)
    }

    pub async fn set_scene_duration(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<bool, String> {
        self.state
            .lock()
            .await
            .set_scene_duration_ms(&scene_id, duration_ms)
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<bool, String> {
        self.state
            .lock()
            .await
            .set_scene_scope_faders_enabled(&scene_id, enabled)
    }

    pub async fn set_scene_scope_pan_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<bool, String> {
        self.state
            .lock()
            .await
            .set_scene_scope_pan_enabled(&scene_id, enabled)
    }

    pub async fn set_channel_scoped(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<bool, String> {
        self.state
            .lock()
            .await
            .set_channel_scoped(&scene_id, group, channel, scoped)
    }

    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<bool, String> {
        self.state
            .lock()
            .await
            .set_all_channels_scoped(&scene_id, scoped)
    }

    pub async fn store_scene_config(
        &self,
        scene_id: String,
        channels: Vec<ChannelInfo>,
    ) -> Result<bool, String> {
        self.state
            .lock()
            .await
            .store_scene_config(&scene_id, &channels)
    }

    pub async fn reconcile_scene_list(&self, scenes: Vec<SceneListEntry>) -> bool {
        self.state
            .lock()
            .await
            .reconcile_scene_fade_configs(&scenes)
    }

    pub async fn scene_reconciliation_diagnostic(&self, scenes: Vec<SceneListEntry>) -> String {
        self.state
            .lock()
            .await
            .scene_reconciliation_diagnostic(&scenes)
    }

    pub async fn replace_snapshot(&self, snapshot: ShowSnapshot) {
        self.state.lock().await.replace_snapshot(snapshot);
    }

    pub async fn clear(&self) {
        self.state.lock().await.clear();
    }
}
