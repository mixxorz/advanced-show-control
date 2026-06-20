use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::lv1::{ChannelInfo, Lv1StateSnapshot};
use crate::show::{
    CueSceneResult, LoadShowFileResult, NewShowFileResult, SceneConfig, SelectedSceneResult,
    ShowCommandResult, ShowDocument, ShowFile, ShowStateHandle,
};

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum AppCommandError {
    #[error("LV1 actor is unavailable")]
    Lv1Unavailable,
    #[error("fade engine is unavailable")]
    FadeUnavailable,
    #[error("show state is unavailable")]
    ShowUnavailable,
    #[error("app command reply channel is closed")]
    ReplyChannelClosed,
    #[error("command failed: {0}")]
    CommandFailed(String),
    #[error("generation is stale")]
    StaleGeneration,
}

#[derive(Clone, Default)]
struct AppCommandTargets {
    show: Option<ShowStateHandle>,
    generation: u64,
}

#[derive(Clone)]
pub struct AppCommandBus {
    targets: Arc<Mutex<AppCommandTargets>>,
}

impl AppCommandBus {
    pub fn new() -> Self {
        Self {
            targets: Arc::new(Mutex::new(AppCommandTargets::default())),
        }
    }

    pub fn new_with_show(show: ShowStateHandle) -> Self {
        Self {
            targets: Arc::new(Mutex::new(AppCommandTargets {
                show: Some(show),
                ..AppCommandTargets::default()
            })),
        }
    }

    pub async fn set_show_target(&self, show: ShowStateHandle) {
        self.targets.lock().await.show = Some(show);
    }

    pub(crate) async fn set_runtime_targets(&self, generation: u64) {
        let mut targets = self.targets.lock().await;
        targets.generation = generation;
    }

    pub(crate) async fn clear_runtime_targets(&self, generation: u64) {
        let _ = generation;
    }

    pub async fn handle_runtime_disconnected(
        &self,
        reason: String,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::handle_runtime_disconnected(&show, reason).await)
    }

    pub async fn set_discovered_lv1_systems(
        &self,
        systems: Vec<DiscoveredLv1System>,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::set_discovered_lv1_systems(&show, systems).await)
    }

    pub async fn set_pending_lv1_identity(
        &self,
        identity: Option<Lv1SystemIdentity>,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::set_pending_lv1_identity(&show, identity).await)
    }

    pub async fn establish_connected_lv1_identity(
        &self,
        identity: Lv1SystemIdentity,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::establish_connected_lv1_identity(&show, identity).await)
    }

    pub async fn clear_connected_lv1_identity(&self) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::clear_connected_lv1_identity(&show).await)
    }

    pub async fn set_reconnect_state(
        &self,
        reconnect: ReconnectState,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::set_reconnect_state(&show, reconnect).await)
    }

    pub async fn set_show(&self, show: Option<ShowStateHandle>) {
        self.targets.lock().await.show = show;
    }

    pub async fn set_generation(&self, generation: u64) {
        self.targets.lock().await.generation = generation;
    }

    pub async fn get_generation(&self) -> u64 {
        self.targets.lock().await.generation
    }

    pub async fn clear_targets(&self) {
        let mut targets = self.targets.lock().await;
        targets.show = None;
        targets.generation += 1;
    }

    pub async fn get_show_document(&self) -> Result<ShowDocument, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => Ok(crate::show::get_show_document(&show).await),
            None => Err(AppCommandError::ShowUnavailable),
        }
    }

    pub async fn current_show_file_path(
        &self,
    ) -> Result<Option<std::path::PathBuf>, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => Ok(crate::show::current_show_file_path(&show).await),
            None => Err(AppCommandError::ShowUnavailable),
        }
    }

    pub async fn get_scene_config(
        &self,
        scene_id: String,
    ) -> Result<Option<SceneConfig>, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => Ok(crate::show::get_scene_config(&show, scene_id).await),
            None => Err(AppCommandError::ShowUnavailable),
        }
    }

    pub async fn get_lockout(&self) -> Result<bool, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => Ok(crate::show::get_lockout(&show).await),
            None => Err(AppCommandError::ShowUnavailable),
        }
    }

    async fn show_target(&self) -> Result<ShowStateHandle, AppCommandError> {
        self.targets
            .lock()
            .await
            .show
            .clone()
            .ok_or(AppCommandError::ShowUnavailable)
    }

    pub async fn set_lockout(&self, enabled: bool) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::set_lockout(&show, enabled).await)
    }

    pub async fn set_scene_duration_ms(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::set_scene_duration_ms(&show, scene_id, duration_ms)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::set_scene_scope_faders_enabled(&show, scene_id, enabled)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_scene_scope_pan_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::set_scene_scope_pan_enabled(&show, scene_id, enabled)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_channel_scoped(
        &self,
        scene_id: String,
        group: i32,
        channel: i32,
        scoped: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::set_channel_scoped(&show, scene_id, group, channel, scoped)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::set_all_channels_scoped(&show, scene_id, scoped)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn cue_scene(&self, scene_id: String) -> Result<CueSceneResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::cue_scene(&show, scene_id)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn select_scene_config(
        &self,
        scene_id: String,
    ) -> Result<SelectedSceneResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::select_scene_config(&show, scene_id)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn store_scene_config(
        &self,
        scene_id: String,
        channels: Vec<ChannelInfo>,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::store_scene_config(&show, scene_id, channels)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn new_show_file(
        &self,
        lv1: Option<Lv1StateSnapshot>,
    ) -> Result<NewShowFileResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::new_show_file(&show, lv1)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn export_show_file_snapshot(
        &self,
        saved_at: String,
    ) -> Result<ShowFile, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::export_show_file_snapshot(&show, saved_at).await)
    }

    pub async fn export_show_file_for_save(
        &self,
        saved_at: String,
    ) -> Result<ShowFile, AppCommandError> {
        self.export_show_file_snapshot(saved_at).await
    }

    pub async fn mark_show_file_saved(
        &self,
        path: std::path::PathBuf,
        saved_at: String,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::mark_show_file_saved(&show, path, saved_at).await)
    }

    pub async fn load_show_file_from_path(
        &self,
        path: std::path::PathBuf,
        file: ShowFile,
        lv1: Lv1StateSnapshot,
    ) -> Result<LoadShowFileResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::load_show_file_from_dto(&show, path, file, Some(lv1))
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn load_show_file_from_dto(
        &self,
        path: std::path::PathBuf,
        file: ShowFile,
        lv1: Lv1StateSnapshot,
    ) -> Result<LoadShowFileResult, AppCommandError> {
        self.load_show_file_from_path(path, file, lv1).await
    }
}

impl Default for AppCommandBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot, SceneListEntry};
    use crate::runtime::events::AppEventBus;
    use crate::show::{
        ChannelConfig, SHOW_FILE_SCHEMA_VERSION, SceneConfig, SceneScopeToggles, ShowDocument,
        ShowFile, ShowFileSafety, ShowFileSceneConfig, ShowFileSceneScopeToggles, ShowStateHandle,
    };
    fn scene_config() -> SceneConfig {
        SceneConfig {
            scene_id: "1::Intro".to_string(),
            scene_index: 1,
            scene_name: "Intro".to_string(),
            duration_ms: 0,
            channel_configs: vec![ChannelConfig {
                group: 0,
                channel: 1,
                fader_db: Some(-12.0),
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            }],
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn channel_info() -> ChannelInfo {
        ChannelInfo {
            group: 0,
            channel: 1,
            name: "Vocal".to_string(),
            gain_db: -12.0,
            muted: false,
            pan: None,
            balance: None,
            width: None,
            pan_mode: None,
        }
    }

    async fn bus_with_show_document(snapshot: ShowDocument) -> (AppCommandBus, AppEventBus) {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        crate::show::replace_show_document_for_test(&show, snapshot).await;
        let bus = AppCommandBus::new();
        bus.set_show(Some(show)).await;
        (bus, event_bus)
    }

    #[tokio::test]
    async fn command_targets_update_after_mutex_contention_releases() {
        let bus = AppCommandBus::new();
        let guard = bus.targets.lock().await;
        let show_bus = bus.clone();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);

        let update = tokio::spawn(async move {
            show_bus.set_show_target(show).await;
        });

        tokio::task::yield_now().await;
        assert!(!update.is_finished());
        drop(guard);

        update.await.unwrap();
        assert!(bus.targets.lock().await.show.is_some());
    }

    #[tokio::test]
    async fn missing_show_returns_show_unavailable() {
        let bus = AppCommandBus::new();

        let err = bus.get_show_document().await.unwrap_err();

        assert_eq!(err, AppCommandError::ShowUnavailable);
    }

    #[tokio::test]
    async fn present_show_returns_snapshot() {
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new();
        bus.set_show(Some(ShowStateHandle::new_empty(event_bus.clone())))
            .await;

        let snapshot = bus.get_show_document().await.unwrap();

        assert!(!snapshot.lockout);
        assert!(snapshot.scene_configs.is_empty());
    }

    #[tokio::test]
    async fn set_lockout_routes_to_show_state() {
        let (bus, event_bus) = bus_with_show_document(ShowDocument::empty()).await;
        let mut events = event_bus.subscribe();

        let result = bus.set_lockout(true).await.unwrap();

        assert!(result.changed);
        assert!(bus.get_show_document().await.unwrap().lockout);
        assert!(matches!(
            events.recv().await.unwrap(),
            crate::runtime::events::AppEvent::Show(_)
        ));
    }

    #[tokio::test]
    async fn no_op_show_command_through_bus_does_not_publish_show_event() {
        let snapshot = ShowDocument {
            lockout: true,
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, event_bus) = bus_with_show_document(snapshot).await;
        let mut events = event_bus.subscribe();

        while events.try_recv().is_ok() {}

        let result = bus.set_lockout(true).await.unwrap();

        assert!(!result.changed);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn set_scene_duration_routes_to_show_state() {
        let snapshot = ShowDocument {
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, _event_bus) = bus_with_show_document(snapshot).await;

        let result = bus
            .set_scene_duration_ms("1::Intro".to_string(), 2_500)
            .await
            .unwrap();

        assert!(result.changed);
        let updated = bus
            .get_scene_config("1::Intro".to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.duration_ms, 2_500);
    }

    #[tokio::test]
    async fn scope_edit_routes_to_show_state() {
        let snapshot = ShowDocument {
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, _event_bus) = bus_with_show_document(snapshot).await;

        let result = bus
            .set_channel_scoped("1::Intro".to_string(), 0, 1, true)
            .await
            .unwrap();

        assert!(result.changed);
        let updated = bus
            .get_scene_config("1::Intro".to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.scoped_channels.len(), 1);
        assert_eq!(updated.scoped_channels[0].group, 0);
        assert_eq!(updated.scoped_channels[0].channel, 1);
    }

    #[tokio::test]
    async fn all_channels_scope_routes_to_show_state() {
        let snapshot = ShowDocument {
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, _event_bus) = bus_with_show_document(snapshot).await;

        let result = bus
            .set_all_channels_scoped("1::Intro".to_string(), true)
            .await
            .unwrap();

        assert!(result.changed);
        let updated = bus
            .get_scene_config("1::Intro".to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.scoped_channels.len(), 1);
    }

    #[tokio::test]
    async fn scene_scope_toggles_route_to_show_state() {
        let snapshot = ShowDocument {
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, _event_bus) = bus_with_show_document(snapshot).await;

        let faders = bus
            .set_scene_scope_faders_enabled("1::Intro".to_string(), false)
            .await
            .unwrap();
        let pan = bus
            .set_scene_scope_pan_enabled("1::Intro".to_string(), true)
            .await
            .unwrap();

        assert!(faders.changed);
        assert!(pan.changed);
        let updated = bus
            .get_scene_config("1::Intro".to_string())
            .await
            .unwrap()
            .unwrap();
        assert!(!updated.scope_toggles.faders);
        assert!(updated.scope_toggles.pan);
    }

    #[tokio::test]
    async fn cue_scene_routes_to_show_state_and_returns_scene() {
        let snapshot = ShowDocument {
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, _event_bus) = bus_with_show_document(snapshot).await;

        let result = bus.cue_scene("1::Intro".to_string()).await.unwrap();

        assert!(result.changed);
        assert_eq!(result.scene.scene_id, "1::Intro");
        assert_eq!(
            bus.get_show_document().await.unwrap().cued_scene_id,
            Some("1::Intro".to_string())
        );
    }

    #[tokio::test]
    async fn select_scene_config_validates_through_show_state() {
        let snapshot = ShowDocument {
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, _event_bus) = bus_with_show_document(snapshot).await;

        let result = bus
            .select_scene_config("1::Intro".to_string())
            .await
            .unwrap();

        assert_eq!(result.scene.scene_id, "1::Intro");
    }

    #[tokio::test]
    async fn store_scene_config_routes_to_show_state_with_lv1_channels() {
        let snapshot = ShowDocument {
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, _event_bus) = bus_with_show_document(snapshot).await;

        let result = bus
            .store_scene_config("1::Intro".to_string(), vec![channel_info()])
            .await
            .unwrap();

        assert!(result.changed);
        let updated = bus
            .get_scene_config("1::Intro".to_string())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(updated.channel_configs.len(), 1);
        assert_eq!(updated.channel_configs[0].fader_db, Some(-12.0));
    }

    #[tokio::test]
    async fn low_risk_show_commands_return_show_unavailable_without_target() {
        let bus = AppCommandBus::new();

        assert_eq!(
            bus.set_lockout(true).await.unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.set_scene_duration_ms("1::Intro".to_string(), 100)
                .await
                .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.set_channel_scoped("1::Intro".to_string(), 0, 1, true)
                .await
                .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.set_all_channels_scoped("1::Intro".to_string(), true)
                .await
                .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.set_scene_scope_faders_enabled("1::Intro".to_string(), false)
                .await
                .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.set_scene_scope_pan_enabled("1::Intro".to_string(), true)
                .await
                .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.cue_scene("1::Intro".to_string()).await.unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.select_scene_config("1::Intro".to_string())
                .await
                .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.store_scene_config("1::Intro".to_string(), vec![channel_info()])
                .await
                .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.new_show_file(None).await.unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        assert_eq!(
            bus.export_show_file_for_save("saved".to_string())
                .await
                .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
        let file = ShowFile {
            schema_version: SHOW_FILE_SCHEMA_VERSION,
            app_version: "0.1.0".to_string(),
            saved_at: "saved".to_string(),
            safety: ShowFileSafety { lockout: true },
            cued_scene_id: None,
            scene_configs: vec![ShowFileSceneConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 1_000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: ShowFileSceneScopeToggles::default(),
            }],
        };
        assert_eq!(
            bus.load_show_file_from_dto(
                std::path::PathBuf::from("/tmp/test.lv1show"),
                file,
                Lv1StateSnapshot {
                    connection: ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![SceneListEntry {
                        index: 1,
                        name: "Intro".to_string()
                    }],
                    channels: Vec::new(),
                }
            )
            .await
            .unwrap_err(),
            AppCommandError::ShowUnavailable
        );
    }

    #[tokio::test]
    async fn new_show_file_routes_through_show_state_and_reconciles_lv1_scenes() {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);
        crate::show::replace_show_document_for_test(
            &show,
            ShowDocument {
                lockout: true,
                scene_configs: vec![scene_config()],
                cued_scene_id: Some("1:Intro".to_string()),
            },
        )
        .await;
        bus.set_show(Some(show)).await;
        let lv1 = Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            }],
            channels: Vec::new(),
        };

        let result = bus.new_show_file(Some(lv1)).await.unwrap();

        assert_eq!(result.selected_scene_id, Some("2::Verse".to_string()));
        let snapshot = bus.get_show_document().await.unwrap();
        assert!(!snapshot.lockout);
        assert_eq!(snapshot.scene_configs[0].scene_id, "2::Verse");
    }

    #[tokio::test]
    async fn export_show_file_snapshot_routes_through_show_state() {
        let snapshot = ShowDocument {
            scene_configs: vec![scene_config()],
            ..ShowDocument::empty()
        };
        let (bus, _event_bus) = bus_with_show_document(snapshot).await;

        let file = bus
            .export_show_file_snapshot("saved".to_string())
            .await
            .unwrap();

        assert_eq!(file.saved_at, "saved");
        assert_eq!(file.scene_configs[0].scene_name, "Intro");
    }

    #[tokio::test]
    async fn load_show_file_from_dto_routes_through_show_state() {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        bus.set_show(Some(ShowStateHandle::new_empty(event_bus)))
            .await;
        let lv1 = Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: Vec::new(),
        };
        let file = ShowFile {
            schema_version: SHOW_FILE_SCHEMA_VERSION,
            app_version: "0.1.0".to_string(),
            saved_at: "saved".to_string(),
            safety: ShowFileSafety { lockout: true },
            cued_scene_id: None,
            scene_configs: vec![ShowFileSceneConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 1_000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: ShowFileSceneScopeToggles::default(),
            }],
        };

        let result = bus
            .load_show_file_from_dto(std::path::PathBuf::from("/tmp/test.lv1show"), file, lv1)
            .await
            .unwrap();

        assert_eq!(result.selected_scene_id, Some("1::Intro".to_string()));
        assert!(!result.report.removed_anything());
        assert!(bus.get_show_document().await.unwrap().lockout);
        assert_eq!(result.saved_at, "saved");
    }

    #[tokio::test]
    async fn load_show_file_from_path_routes_through_show_state() {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        bus.set_show(Some(ShowStateHandle::new_empty(event_bus)))
            .await;
        let lv1 = Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: Vec::new(),
        };
        let file = ShowFile {
            schema_version: SHOW_FILE_SCHEMA_VERSION,
            app_version: "0.1.0".to_string(),
            saved_at: "saved".to_string(),
            safety: ShowFileSafety { lockout: true },
            cued_scene_id: None,
            scene_configs: vec![ShowFileSceneConfig {
                scene_index: 1,
                scene_name: "Intro".to_string(),
                duration_ms: 1_000,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: ShowFileSceneScopeToggles::default(),
            }],
        };

        let result = bus
            .load_show_file_from_path(std::path::PathBuf::from("/tmp/test.lv1show"), file, lv1)
            .await
            .unwrap();

        assert_eq!(result.selected_scene_id, Some("1::Intro".to_string()));
        assert!(!result.report.removed_anything());
        assert!(bus.get_show_document().await.unwrap().lockout);
        assert_eq!(result.saved_at, "saved");
    }
}
