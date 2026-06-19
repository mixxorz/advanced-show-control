use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use crate::fade::handle::FadeEngineHandle;
use crate::fade::types::FadeConfig;
use crate::lv1::commands::Lv1ParameterWrite;
use crate::lv1::events::Lv1ActorError;
use crate::lv1::handle::Lv1ActorHandle;
use crate::lv1::types::{ChannelInfo, Lv1StateSnapshot};
use crate::show::commands::{
    CueSceneResult, LoadShowFileResult, NewShowFileResult, RecallSceneResult, SelectedSceneResult,
    ShowCommandResult,
};
use crate::show::handle::ShowStateHandle;
use crate::show::show_file::ShowFile;
use crate::show::types::{SceneConfig, ShowSnapshot};

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
    lv1: Option<Lv1ActorHandle>,
    fade: Option<FadeEngineHandle>,
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

    pub async fn set_lv1(&self, lv1: Option<Lv1ActorHandle>) {
        self.targets.lock().await.lv1 = lv1;
    }

    pub async fn set_fade(&self, fade: Option<FadeEngineHandle>) {
        self.targets.lock().await.fade = fade;
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

    pub async fn start_fade_if_generation(
        &self,
        expected: u64,
        config: FadeConfig,
    ) -> Result<(), AppCommandError> {
        let targets = self.targets.lock().await;
        if targets.generation != expected {
            let result = Err(AppCommandError::StaleGeneration);
            log_failure("start_fade_if_generation", &result);
            return result;
        }
        let fade = targets.fade.clone().ok_or(AppCommandError::FadeUnavailable);
        drop(targets);
        let result = match fade {
            Ok(fade) => fade
                .start_fade_if_generation(expected, config)
                .await
                .map_err(|_| AppCommandError::FadeUnavailable),
            Err(error) => Err(error),
        };
        log_failure("start_fade_if_generation", &result);
        result
    }

    pub async fn clear_targets(&self) {
        let mut targets = self.targets.lock().await;
        targets.lv1 = None;
        targets.fade = None;
        targets.show = None;
        targets.generation += 1;
    }

    pub async fn get_show_snapshot(&self) -> Result<ShowSnapshot, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => Ok(show.get_snapshot().await),
            None => Err(AppCommandError::ShowUnavailable),
        }
    }

    pub async fn get_scene_config(
        &self,
        scene_id: String,
    ) -> Result<Option<SceneConfig>, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => Ok(show.get_scene_config(scene_id).await),
            None => Err(AppCommandError::ShowUnavailable),
        }
    }

    pub async fn get_lockout(&self) -> Result<bool, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => Ok(show.get_lockout().await),
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
        Ok(crate::show::commands::set_lockout(&show, enabled).await)
    }

    pub async fn set_scene_duration_ms(
        &self,
        scene_id: String,
        duration_ms: u64,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_scene_duration_ms(&show, scene_id, duration_ms)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_scene_scope_faders_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_scene_scope_faders_enabled(&show, scene_id, enabled)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_scene_scope_pan_enabled(
        &self,
        scene_id: String,
        enabled: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_scene_scope_pan_enabled(&show, scene_id, enabled)
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
        crate::show::commands::set_channel_scoped(&show, scene_id, group, channel, scoped)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn set_all_channels_scoped(
        &self,
        scene_id: String,
        scoped: bool,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::set_all_channels_scoped(&show, scene_id, scoped)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn cue_scene(&self, scene_id: String) -> Result<CueSceneResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::cue_scene(&show, scene_id)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn select_scene_config(
        &self,
        scene_id: String,
    ) -> Result<SelectedSceneResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::select_scene_config(&show, scene_id)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn store_scene_config(
        &self,
        scene_id: String,
        channels: Vec<ChannelInfo>,
    ) -> Result<ShowCommandResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::store_scene_config(&show, scene_id, channels)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn new_show_file(
        &self,
        lv1: Option<Lv1StateSnapshot>,
    ) -> Result<NewShowFileResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::new_show_file(&show, lv1)
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn export_show_file_for_save(
        &self,
        saved_at: String,
    ) -> Result<ShowFile, AppCommandError> {
        let show = self.show_target().await?;
        Ok(crate::show::commands::export_show_file_for_save(&show, saved_at).await)
    }

    pub async fn load_show_file_from_dto(
        &self,
        path: std::path::PathBuf,
        file: ShowFile,
        lv1: Lv1StateSnapshot,
    ) -> Result<LoadShowFileResult, AppCommandError> {
        let show = self.show_target().await?;
        crate::show::commands::load_show_file_from_dto(&show, path, file, Some(lv1))
            .await
            .map_err(AppCommandError::CommandFailed)
    }

    pub async fn recall_scene_by_id(
        &self,
        scene_id: String,
    ) -> Result<RecallSceneResult, AppCommandError> {
        let show = self.show_target().await?;
        let lv1 = self.get_lv1_state().await.map_err(|error| match error {
            AppCommandError::Lv1Unavailable => AppCommandError::CommandFailed(
                "Recall blocked: LV1 state is unavailable".to_string(),
            ),
            other => other,
        })?;
        let show_snapshot = show.get_snapshot().await;
        let result =
            crate::show::commands::validate_recall_scene_request(&show_snapshot, &lv1, &scene_id)
                .map_err(AppCommandError::CommandFailed)?;

        self.recall_scene(result.lv1_scene_index).await?;
        Ok(result)
    }

    pub async fn get_lv1_state(&self) -> Result<Lv1StateSnapshot, AppCommandError> {
        let lv1 = self.targets.lock().await.lv1.clone();
        match lv1 {
            Some(lv1) => Ok(lv1.get_state().await),
            None => Err(AppCommandError::Lv1Unavailable),
        }
    }

    pub async fn set_gain(
        &self,
        group: i32,
        channel: i32,
        gain_db: f64,
    ) -> Result<(), AppCommandError> {
        let lv1 = self.targets.lock().await.lv1.clone();
        let result = match lv1 {
            Some(lv1) => lv1
                .set_gain(group, channel, gain_db)
                .await
                .map_err(map_lv1_error),
            None => Err(AppCommandError::Lv1Unavailable),
        };
        log_failure("set_gain", &result);
        result
    }

    pub async fn set_pan(
        &self,
        group: i32,
        channel: i32,
        value: f64,
    ) -> Result<(), AppCommandError> {
        let lv1 = self.targets.lock().await.lv1.clone();
        let result = match lv1 {
            Some(lv1) => lv1
                .set_pan(group, channel, value)
                .await
                .map_err(map_lv1_error),
            None => Err(AppCommandError::Lv1Unavailable),
        };
        log_failure("set_pan", &result);
        result
    }

    pub async fn set_balance(
        &self,
        group: i32,
        channel: i32,
        value: f64,
    ) -> Result<(), AppCommandError> {
        let lv1 = self.targets.lock().await.lv1.clone();
        let result = match lv1 {
            Some(lv1) => lv1
                .set_balance(group, channel, value)
                .await
                .map_err(map_lv1_error),
            None => Err(AppCommandError::Lv1Unavailable),
        };
        log_failure("set_balance", &result);
        result
    }

    pub async fn set_width(
        &self,
        group: i32,
        channel: i32,
        value: f64,
    ) -> Result<(), AppCommandError> {
        let lv1 = self.targets.lock().await.lv1.clone();
        let result = match lv1 {
            Some(lv1) => lv1
                .set_width(group, channel, value)
                .await
                .map_err(map_lv1_error),
            None => Err(AppCommandError::Lv1Unavailable),
        };
        log_failure("set_width", &result);
        result
    }

    pub async fn recall_scene(&self, scene_index: i32) -> Result<(), AppCommandError> {
        let lv1 = self.targets.lock().await.lv1.clone();
        let result = match lv1 {
            Some(lv1) => lv1.recall_scene(scene_index).await.map_err(map_lv1_error),
            None => Err(AppCommandError::Lv1Unavailable),
        };
        log_failure("recall_scene", &result);
        result
    }

    pub async fn write_batch(&self, writes: Vec<Lv1ParameterWrite>) -> Result<(), AppCommandError> {
        self.write_batch_with_generation(None, writes).await
    }

    pub async fn write_batch_if_generation(
        &self,
        expected: u64,
        writes: Vec<Lv1ParameterWrite>,
    ) -> Result<(), AppCommandError> {
        self.write_batch_with_generation(Some(expected), writes)
            .await
    }

    /// Clones the current LV1 handle only after the optional generation check passes.
    ///
    /// The actual `lv1.write_batch(...)` call still happens after the mutex guard is dropped,
    /// because the LV1 actor API is async and must be awaited outside the command-target lock.
    /// This closes the stale-target selection window, but not the in-flight actor write itself;
    /// fully eliminating that remaining gap would require moving generation ownership into the
    /// LV1 actor/write path.
    async fn write_batch_with_generation(
        &self,
        expected: Option<u64>,
        writes: Vec<Lv1ParameterWrite>,
    ) -> Result<(), AppCommandError> {
        if writes.is_empty() {
            return Ok(());
        }

        let lv1 = {
            let targets = self.targets.lock().await;
            if let Some(expected) = expected
                && targets.generation != expected
            {
                let result = Err(AppCommandError::StaleGeneration);
                log_failure("write_batch_if_generation", &result);
                return result;
            }
            targets.lv1.clone()
        };

        let result = match lv1 {
            Some(lv1) => lv1.write_batch(writes).await.map_err(map_lv1_error),
            None => Err(AppCommandError::Lv1Unavailable),
        };
        log_failure(
            expected.map_or("write_batch", |_| "write_batch_if_generation"),
            &result,
        );
        result
    }

    /// Starts a fade without any generation check.
    ///
    /// Scene recall automation must prefer `start_fade_if_generation` so stale recall tasks
    /// cannot dispatch after disconnect/reconnect.
    pub async fn start_fade(&self, config: FadeConfig) -> Result<(), AppCommandError> {
        let fade = self.targets.lock().await.fade.clone();
        let result = match fade {
            Some(fade) => fade.start_fade(config).await,
            None => Err(AppCommandError::FadeUnavailable),
        };
        log_failure("start_fade", &result);
        result
    }

    pub async fn abort_all_fades(&self) -> Result<(), AppCommandError> {
        let fade = self.targets.lock().await.fade.clone();
        let result = match fade {
            Some(fade) => fade.abort_all().await,
            None => Err(AppCommandError::FadeUnavailable),
        };
        log_failure("abort_all_fades", &result);
        result
    }
}

impl Default for AppCommandBus {
    fn default() -> Self {
        Self::new()
    }
}

fn map_lv1_error(error: Lv1ActorError) -> AppCommandError {
    match error {
        Lv1ActorError::NotConnected => AppCommandError::Lv1Unavailable,
        other => AppCommandError::CommandFailed(other.to_string()),
    }
}

fn log_failure(command: &str, result: &Result<(), AppCommandError>) {
    if let Err(error) = result {
        tracing::error!(
            event = "command_failed",
            command,
            error = %error,
            "Command failed: {command}: {error}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::commands::FadeCommand;
    use crate::fade::curve::FadeCurve;
    use crate::fade::types::{FadeConfig, FadeParameter, FadeSceneIdentity, FadeTarget};
    use crate::lv1::types::ChannelInfo;
    use crate::runtime::events::AppEventBus;
    use crate::show::handle::ShowStateHandle;
    use crate::show::show_file::{
        SHOW_FILE_SCHEMA_VERSION, ShowFile, ShowFileSafety, ShowFileSceneConfig,
        ShowFileSceneScopeToggles,
    };
    use crate::show::types::{ChannelConfig, SceneConfig, SceneScopeToggles, ShowSnapshot};

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

    async fn bus_with_show_snapshot(snapshot: ShowSnapshot) -> (AppCommandBus, AppEventBus) {
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus.clone());
        show.replace_snapshot(snapshot).await;
        let bus = AppCommandBus::new();
        bus.set_show(Some(show)).await;
        (bus, event_bus)
    }

    #[tokio::test]
    async fn command_bus_constructs_without_event_bus() {
        let bus = AppCommandBus::new();

        let err = bus.get_lv1_state().await.unwrap_err();

        assert_eq!(err, AppCommandError::Lv1Unavailable);
    }

    #[tokio::test]
    async fn missing_lv1_returns_lv1_unavailable() {
        let bus = AppCommandBus::new();

        let err = bus.get_lv1_state().await.unwrap_err();

        assert_eq!(err, AppCommandError::Lv1Unavailable);
    }

    #[tokio::test]
    async fn missing_fade_returns_fade_unavailable_without_event_bus_log() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();
        let config = FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -12.0,
            }],
            duration_ms: 1_000,
            curve: FadeCurve::Linear,
        };

        let err = bus.start_fade(config).await.unwrap_err();

        assert_eq!(err, AppCommandError::FadeUnavailable);

        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn stale_generation_rejects_before_sending_fade_command() {
        let bus = AppCommandBus::new();
        let (fade_tx, mut fade_rx) = tokio::sync::mpsc::channel(1);
        bus.set_fade(Some(FadeEngineHandle::new(fade_tx))).await;
        bus.set_generation(2).await;

        let config = FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -12.0,
            }],
            duration_ms: 1_000,
            curve: FadeCurve::Linear,
        };

        let err = bus.start_fade_if_generation(1, config).await.unwrap_err();

        assert_eq!(err, AppCommandError::StaleGeneration);
        assert!(fade_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn start_fade_if_generation_sends_expected_generation_to_fade_engine() {
        let bus = AppCommandBus::new();
        let (fade_tx, mut fade_rx) = tokio::sync::mpsc::channel(1);
        bus.set_fade(Some(FadeEngineHandle::new(fade_tx))).await;
        bus.set_generation(2).await;

        let config = FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -12.0,
            }],
            duration_ms: 1_000,
            curve: FadeCurve::Linear,
        };

        tokio::spawn(async move {
            if let Some(FadeCommand::RecallSceneFade {
                expected_generation,
                reply,
                ..
            }) = fade_rx.recv().await
            {
                assert_eq!(expected_generation, Some(2));
                let _ = reply.send(Ok(()));
            }
        });

        assert_eq!(bus.start_fade_if_generation(2, config).await, Ok(()));
    }

    #[tokio::test]
    async fn clearing_targets_invalidates_cloned_bus_handles() {
        let bus = AppCommandBus::new();
        let (fade_tx, mut fade_rx) = tokio::sync::mpsc::channel(1);
        let fade = FadeEngineHandle::new(fade_tx);

        tokio::spawn(async move {
            if let Some(FadeCommand::RecallSceneFade { reply, .. }) = fade_rx.recv().await {
                let _ = reply.send(Ok(()));
            }
        });

        bus.set_fade(Some(fade)).await;
        let cloned_bus = bus.clone();

        let config = FadeConfig {
            scene: FadeSceneIdentity {
                index: 1,
                name: "Intro".to_string(),
            },
            targets: vec![FadeTarget {
                group: 0,
                channel: 1,
                parameter: FadeParameter::FaderDb,
                target: -12.0,
            }],
            duration_ms: 1_000,
            curve: FadeCurve::Linear,
        };

        assert_eq!(cloned_bus.start_fade(config.clone()).await, Ok(()));

        bus.clear_targets().await;

        assert_eq!(
            cloned_bus.start_fade(config).await,
            Err(AppCommandError::FadeUnavailable)
        );
    }

    #[tokio::test]
    async fn missing_show_returns_show_unavailable() {
        let bus = AppCommandBus::new();

        let err = bus.get_show_snapshot().await.unwrap_err();

        assert_eq!(err, AppCommandError::ShowUnavailable);
    }

    #[tokio::test]
    async fn present_show_returns_snapshot() {
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new();
        bus.set_show(Some(ShowStateHandle::new_empty(event_bus.clone())))
            .await;

        let snapshot = bus.get_show_snapshot().await.unwrap();

        assert!(!snapshot.lockout);
        assert!(snapshot.scene_configs.is_empty());
    }

    #[tokio::test]
    async fn set_lockout_routes_to_show_state() {
        let (bus, event_bus) = bus_with_show_snapshot(ShowSnapshot::empty()).await;
        let mut events = event_bus.subscribe();

        let result = bus.set_lockout(true).await.unwrap();

        assert!(result.changed);
        assert!(bus.get_show_snapshot().await.unwrap().lockout);
        assert!(matches!(
            events.recv().await.unwrap(),
            crate::runtime::events::AppEvent::Show(_)
        ));
    }

    #[tokio::test]
    async fn no_op_show_command_through_bus_does_not_publish_show_event() {
        let snapshot = ShowSnapshot {
            lockout: true,
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, event_bus) = bus_with_show_snapshot(snapshot).await;
        let mut events = event_bus.subscribe();

        while events.try_recv().is_ok() {}

        let result = bus.set_lockout(true).await.unwrap();

        assert!(!result.changed);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn set_scene_duration_routes_to_show_state() {
        let snapshot = ShowSnapshot {
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

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
        let snapshot = ShowSnapshot {
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

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
        let snapshot = ShowSnapshot {
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

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
        let snapshot = ShowSnapshot {
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

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
        let snapshot = ShowSnapshot {
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

        let result = bus.cue_scene("1::Intro".to_string()).await.unwrap();

        assert!(result.changed);
        assert_eq!(result.scene.scene_id, "1::Intro");
        assert_eq!(
            bus.get_show_snapshot().await.unwrap().cued_scene_id,
            Some("1::Intro".to_string())
        );
    }

    #[tokio::test]
    async fn select_scene_config_validates_through_show_state() {
        let snapshot = ShowSnapshot {
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

        let result = bus
            .select_scene_config("1::Intro".to_string())
            .await
            .unwrap();

        assert_eq!(result.scene.scene_id, "1::Intro");
    }

    #[tokio::test]
    async fn store_scene_config_routes_to_show_state_with_lv1_channels() {
        let snapshot = ShowSnapshot {
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

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
                    connection: crate::lv1::types::ConnectionStatus::Connected,
                    scene: None,
                    scene_list: vec![crate::lv1::types::SceneListEntry {
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
        show.replace_snapshot(ShowSnapshot {
            lockout: true,
            scene_configs: vec![scene_config()],
            cued_scene_id: Some("1:Intro".to_string()),
        })
        .await;
        bus.set_show(Some(show)).await;
        let lv1 = Lv1StateSnapshot {
            connection: crate::lv1::types::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![crate::lv1::types::SceneListEntry {
                index: 2,
                name: "Verse".to_string(),
            }],
            channels: Vec::new(),
        };

        let result = bus.new_show_file(Some(lv1)).await.unwrap();

        assert_eq!(result.selected_scene_id, Some("2::Verse".to_string()));
        let snapshot = bus.get_show_snapshot().await.unwrap();
        assert!(!snapshot.lockout);
        assert_eq!(snapshot.scene_configs[0].scene_id, "2::Verse");
    }

    #[tokio::test]
    async fn export_show_file_for_save_routes_through_show_state() {
        let snapshot = ShowSnapshot {
            scene_configs: vec![scene_config()],
            ..ShowSnapshot::empty()
        };
        let (bus, _event_bus) = bus_with_show_snapshot(snapshot).await;

        let file = bus
            .export_show_file_for_save("saved".to_string())
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
            connection: crate::lv1::types::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![crate::lv1::types::SceneListEntry {
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
        assert!(bus.get_show_snapshot().await.unwrap().lockout);
        assert_eq!(result.saved_at, "saved");
    }

    #[tokio::test]
    async fn set_pan_routes_to_lv1_without_event_bus_log() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;

        tokio::spawn(async move {
            if let Some(crate::lv1::commands::Lv1Command::SetPan {
                group,
                channel,
                value,
                reply,
            }) = lv1_rx.recv().await
            {
                assert_eq!((group, channel, value), (2, 7, -18.5));
                let _ = reply.send(Ok(()));
            }
        });

        assert_eq!(bus.set_pan(2, 7, -18.5).await, Ok(()));
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn recall_scene_sends_index_to_lv1_actor() {
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;

        let recall = tokio::spawn({
            let bus = bus.clone();
            async move { bus.recall_scene(4).await }
        });

        if let Some(crate::lv1::commands::Lv1Command::RecallScene { scene_index, reply }) =
            lv1_rx.recv().await
        {
            assert_eq!(scene_index, 4);
            let _ = reply.send(Ok(()));
        } else {
            panic!("expected RecallScene command");
        }

        assert!(recall.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn recall_scene_by_id_validates_show_and_sends_matching_lv1_index() {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);
        show.replace_snapshot(ShowSnapshot {
            lockout: false,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: Default::default(),
            }],
            cued_scene_id: Some("1::Verse".to_string()),
        })
        .await;
        bus.set_show(Some(show)).await;

        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(2);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        let lv1 = Lv1StateSnapshot {
            connection: crate::lv1::types::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![crate::lv1::types::SceneListEntry {
                index: 1,
                name: "Verse".to_string(),
            }],
            channels: Vec::new(),
        };

        let (recall_tx, mut recall_rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::commands::Lv1Command::GetState { reply } => {
                        let _ = reply.send(lv1.clone());
                    }
                    crate::lv1::commands::Lv1Command::RecallScene { scene_index, reply } => {
                        let _ = recall_tx.send(scene_index).await;
                        let _ = reply.send(Ok(()));
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let result = bus
            .recall_scene_by_id("1::Verse".to_string())
            .await
            .unwrap();

        assert_eq!(recall_rx.recv().await, Some(1));
        assert_eq!(result.scene.scene_id, "1::Verse");
        assert_eq!(result.lv1_scene_index, 1);
    }

    #[tokio::test]
    async fn recall_scene_by_id_blocks_lockout_before_sending_to_lv1() {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);
        show.replace_snapshot(ShowSnapshot {
            lockout: true,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: Default::default(),
            }],
            cued_scene_id: Some("1::Verse".to_string()),
        })
        .await;
        bus.set_show(Some(show)).await;

        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(2);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        let lv1 = Lv1StateSnapshot {
            connection: crate::lv1::types::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![crate::lv1::types::SceneListEntry {
                index: 1,
                name: "Verse".to_string(),
            }],
            channels: Vec::new(),
        };
        let (recall_tx, mut recall_rx) = tokio::sync::mpsc::channel(1);

        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::commands::Lv1Command::GetState { reply } => {
                        let _ = reply.send(lv1.clone());
                    }
                    crate::lv1::commands::Lv1Command::RecallScene { scene_index, reply } => {
                        let _ = recall_tx.send(scene_index).await;
                        let _ = reply.send(Ok(()));
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let err = bus
            .recall_scene_by_id("1::Verse".to_string())
            .await
            .unwrap_err();

        assert_eq!(
            err,
            AppCommandError::CommandFailed("Recall blocked: lockout is enabled".to_string())
        );
        assert!(recall_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn recall_scene_by_id_blocks_identity_mismatch_before_sending_to_lv1() {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);
        show.replace_snapshot(ShowSnapshot {
            lockout: false,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: Default::default(),
            }],
            cued_scene_id: Some("1::Verse".to_string()),
        })
        .await;
        bus.set_show(Some(show)).await;

        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(2);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        let lv1 = Lv1StateSnapshot {
            connection: crate::lv1::types::ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![crate::lv1::types::SceneListEntry {
                index: 1,
                name: "Different".to_string(),
            }],
            channels: Vec::new(),
        };
        let (recall_tx, mut recall_rx) = tokio::sync::mpsc::channel(1);

        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::commands::Lv1Command::GetState { reply } => {
                        let _ = reply.send(lv1.clone());
                    }
                    crate::lv1::commands::Lv1Command::RecallScene { scene_index, reply } => {
                        let _ = recall_tx.send(scene_index).await;
                        let _ = reply.send(Ok(()));
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let err = bus
            .recall_scene_by_id("1::Verse".to_string())
            .await
            .unwrap_err();

        assert_eq!(
            err,
            AppCommandError::CommandFailed("Recall blocked: scene identity mismatch".to_string())
        );
        assert!(recall_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn recall_scene_by_id_returns_lv1_recall_failure_without_show_mutation() {
        let bus = AppCommandBus::new();
        let event_bus = AppEventBus::default();
        let show = ShowStateHandle::new_empty(event_bus);
        show.replace_snapshot(ShowSnapshot {
            lockout: false,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: Default::default(),
            }],
            cued_scene_id: Some("1::Verse".to_string()),
        })
        .await;
        bus.set_show(Some(show)).await;

        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(2);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        tokio::spawn(async move {
            while let Some(command) = lv1_rx.recv().await {
                match command {
                    crate::lv1::commands::Lv1Command::GetState { reply } => {
                        let _ = reply.send(Lv1StateSnapshot {
                            connection: crate::lv1::types::ConnectionStatus::Connected,
                            scene: None,
                            scene_list: vec![crate::lv1::types::SceneListEntry {
                                index: 1,
                                name: "Verse".to_string(),
                            }],
                            channels: Vec::new(),
                        });
                    }
                    crate::lv1::commands::Lv1Command::RecallScene { reply, .. } => {
                        let _ = reply.send(Err(crate::lv1::events::Lv1ActorError::NotConnected));
                    }
                    _ => panic!("unexpected LV1 command"),
                }
            }
        });

        let err = bus
            .recall_scene_by_id("1::Verse".to_string())
            .await
            .unwrap_err();

        assert_eq!(err, AppCommandError::Lv1Unavailable);
    }

    #[tokio::test]
    async fn missing_lv1_for_balance_returns_error_without_event_bus_log() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();

        let err = bus.set_balance(1, 3, 0.25).await.unwrap_err();

        assert_eq!(err, AppCommandError::Lv1Unavailable);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn write_batch_routes_to_lv1_without_reply() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;

        let writes = vec![Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: crate::lv1::commands::Lv1WriteParameter::FaderDb,
            value: -18.0,
        }];
        let expected = writes.clone();

        tokio::spawn(async move {
            if let Some(crate::lv1::commands::Lv1Command::WriteBatch(received)) =
                lv1_rx.recv().await
            {
                assert_eq!(received, expected);
            }
        });

        assert_eq!(bus.write_batch(writes).await, Ok(()));
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn write_batch_if_generation_rejects_stale_generation_before_sending() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        bus.set_generation(2).await;

        let writes = vec![Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: crate::lv1::commands::Lv1WriteParameter::FaderDb,
            value: -18.0,
        }];

        let err = bus.write_batch_if_generation(1, writes).await.unwrap_err();

        assert_eq!(err, AppCommandError::StaleGeneration);
        assert!(lv1_rx.try_recv().is_err());
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn write_batch_if_generation_routes_fresh_writes() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        bus.set_generation(2).await;

        let writes = vec![Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: crate::lv1::commands::Lv1WriteParameter::FaderDb,
            value: -18.0,
        }];
        let expected = writes.clone();

        tokio::spawn(async move {
            if let Some(crate::lv1::commands::Lv1Command::WriteBatch(received)) =
                lv1_rx.recv().await
            {
                assert_eq!(received, expected);
            }
        });

        assert_eq!(bus.write_batch_if_generation(2, writes).await, Ok(()));
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn write_batch_if_generation_leaves_lv1_unlocked_during_write() {
        let bus = AppCommandBus::new();
        let (lv1_tx, mut lv1_rx) = tokio::sync::mpsc::channel(1);
        bus.set_lv1(Some(Lv1ActorHandle::new(lv1_tx))).await;
        bus.set_generation(2).await;

        let writes = vec![Lv1ParameterWrite {
            group: 0,
            channel: 1,
            parameter: crate::lv1::commands::Lv1WriteParameter::FaderDb,
            value: -18.0,
        }];

        let write = tokio::spawn(async move {
            let _ = bus.write_batch_if_generation(2, writes).await;
        });

        let command = tokio::time::timeout(std::time::Duration::from_secs(1), lv1_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(matches!(
            command,
            crate::lv1::commands::Lv1Command::WriteBatch(_)
        ));
        write.await.unwrap();
    }
}
