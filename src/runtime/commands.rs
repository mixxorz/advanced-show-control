use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

use crate::fade::handle::FadeEngineHandle;
use crate::fade::types::FadeConfig;
use crate::lv1::events::Lv1ActorError;
use crate::lv1::handle::Lv1ActorHandle;
use crate::lv1::types::Lv1StateSnapshot;
use crate::runtime::events::{AppEvent, AppEventBus};
use crate::show::handle::{ShowActorError, ShowStateHandle};
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
    event_bus: AppEventBus,
}

impl AppCommandBus {
    pub fn new(event_bus: AppEventBus) -> Self {
        Self {
            targets: Arc::new(Mutex::new(AppCommandTargets::default())),
            event_bus,
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

    pub async fn clear_targets(&self) {
        let mut targets = self.targets.lock().await;
        targets.lv1 = None;
        targets.fade = None;
        targets.show = None;
    }

    pub async fn get_show_snapshot(&self) -> Result<ShowSnapshot, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => show.get_snapshot().await.map_err(map_show_error),
            None => Err(AppCommandError::ShowUnavailable),
        }
    }

    pub async fn get_scene_config(
        &self,
        scene_id: String,
    ) -> Result<Option<SceneConfig>, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => show
                .get_scene_config(scene_id)
                .await
                .map_err(map_show_error),
            None => Err(AppCommandError::ShowUnavailable),
        }
    }

    pub async fn get_lockout(&self) -> Result<bool, AppCommandError> {
        let show = self.targets.lock().await.show.clone();
        match show {
            Some(show) => show.get_lockout().await.map_err(map_show_error),
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
        publish_failure(&self.event_bus, "set_gain", &result);
        result
    }

    pub async fn start_fade(&self, config: FadeConfig) -> Result<(), AppCommandError> {
        let fade = self.targets.lock().await.fade.clone();
        let result = match fade {
            Some(fade) => fade.start_fade(config).await,
            None => Err(AppCommandError::FadeUnavailable),
        };
        publish_failure(&self.event_bus, "start_fade", &result);
        result
    }

    pub async fn abort_all_fades(&self) -> Result<(), AppCommandError> {
        let fade = self.targets.lock().await.fade.clone();
        let result = match fade {
            Some(fade) => fade.abort_all().await,
            None => Err(AppCommandError::FadeUnavailable),
        };
        publish_failure(&self.event_bus, "abort_all_fades", &result);
        result
    }
}

fn map_lv1_error(error: Lv1ActorError) -> AppCommandError {
    AppCommandError::CommandFailed(error.to_string())
}

fn map_show_error(error: ShowActorError) -> AppCommandError {
    match error {
        ShowActorError::CommandChannelClosed => AppCommandError::ShowUnavailable,
        ShowActorError::ReplyChannelClosed => AppCommandError::ReplyChannelClosed,
    }
}

fn publish_failure(event_bus: &AppEventBus, command: &str, result: &Result<(), AppCommandError>) {
    if let Err(error) = result {
        event_bus.publish(AppEvent::CommandFailed {
            command: command.to_string(),
            message: error.to_string(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::commands::FadeCommand;
    use crate::fade::curve::FadeCurve;
    use crate::fade::types::{FadeConfig, FadeSceneIdentity, FadeTarget};
    use crate::runtime::events::{AppEvent, AppEventBus};
    use crate::show::commands::ShowCommand;
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
                target_db: -12.0,
            }],
            duration_ms: 1_000,
            curve: FadeCurve::Linear,
        };

        let err = bus.start_fade(config).await.unwrap_err();

        assert_eq!(err, AppCommandError::FadeUnavailable);

        match events.recv().await.unwrap() {
            AppEvent::CommandFailed { command, message } => {
                assert_eq!(command, "start_fade");
                assert_eq!(message, AppCommandError::FadeUnavailable.to_string());
            }
            other => panic!("unexpected event: {other:?}"),
        }
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
                target_db: -12.0,
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
    async fn dropped_show_actor_maps_to_show_unavailable() {
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        drop(rx);
        bus.set_show(Some(ShowStateHandle::new(tx))).await;

        let err = bus.get_show_snapshot().await.unwrap_err();

        assert_eq!(err, AppCommandError::ShowUnavailable);
    }

    #[tokio::test]
    async fn dropped_show_reply_maps_to_reply_channel_closed() {
        let event_bus = AppEventBus::default();
        let bus = AppCommandBus::new(event_bus);
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            if let Some(ShowCommand::GetSnapshot { reply }) = rx.recv().await {
                drop(reply);
            }
        });
        bus.set_show(Some(ShowStateHandle::new(tx))).await;

        let err = bus.get_show_snapshot().await.unwrap_err();

        assert_eq!(err, AppCommandError::ReplyChannelClosed);
    }
}
