use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use crate::fade::handle::FadeEngineHandle;
use crate::fade::types::FadeConfig;
use crate::lv1::commands::Lv1ParameterWrite;
use crate::lv1::events::Lv1ActorError;
use crate::lv1::handle::Lv1ActorHandle;
use crate::lv1::types::Lv1StateSnapshot;
use crate::runtime::events::AppEventBus;
use crate::show::handle::ShowStateHandle;
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
    pub fn new(_event_bus: AppEventBus) -> Self {
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
            return Err(AppCommandError::StaleGeneration);
        }
        let fade = targets
            .fade
            .clone()
            .ok_or(AppCommandError::FadeUnavailable)?;
        drop(targets);
        fade.start_fade_if_generation(expected, config)
            .await
            .map_err(|_| AppCommandError::FadeUnavailable)
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

fn map_lv1_error(error: Lv1ActorError) -> AppCommandError {
    AppCommandError::CommandFailed(error.to_string())
}

fn log_failure(command: &str, result: &Result<(), AppCommandError>) {
    if let Err(error) = result {
        tracing::error!(event = "command_failed", command, error = %error, "Command failed: {command}: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::commands::FadeCommand;
    use crate::fade::curve::FadeCurve;
    use crate::fade::types::{FadeConfig, FadeParameter, FadeSceneIdentity, FadeTarget};
    use crate::runtime::events::AppEventBus;
    use crate::show::handle::ShowStateHandle;

    #[tokio::test]
    async fn missing_lv1_returns_lv1_unavailable() {
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);

        let err = bus.get_lv1_state().await.unwrap_err();

        assert_eq!(err, AppCommandError::Lv1Unavailable);
    }

    #[tokio::test]
    async fn missing_fade_returns_fade_unavailable_and_publishes_failure() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new(event_bus.clone());
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
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);
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
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);
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
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus.clone());
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
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);

        let err = bus.get_show_snapshot().await.unwrap_err();

        assert_eq!(err, AppCommandError::ShowUnavailable);
    }

    #[tokio::test]
    async fn present_show_returns_snapshot() {
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);
        bus.set_show(Some(ShowStateHandle::new_empty())).await;

        let snapshot = bus.get_show_snapshot().await.unwrap();

        assert!(!snapshot.lockout);
        assert!(snapshot.scene_configs.is_empty());
    }

    #[tokio::test]
    async fn set_pan_routes_to_lv1_and_publishes_no_failure() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new(event_bus);
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
    async fn missing_lv1_for_balance_publishes_command_failed() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new(event_bus);

        let err = bus.set_balance(1, 3, 0.25).await.unwrap_err();

        assert_eq!(err, AppCommandError::Lv1Unavailable);
        assert!(events.try_recv().is_err());
    }

    #[tokio::test]
    async fn write_batch_routes_to_lv1_without_reply() {
        let event_bus = AppEventBus::default();
        let mut events = event_bus.subscribe();
        let bus = AppCommandBus::new(event_bus);
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
        let bus = AppCommandBus::new(event_bus);
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
        let bus = AppCommandBus::new(event_bus);
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
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);
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
