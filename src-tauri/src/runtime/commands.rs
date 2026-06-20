use std::sync::Arc;

use thiserror::Error;
use tokio::sync::Mutex;

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

    pub(crate) async fn set_runtime_targets(&self, generation: u64) {
        self.targets.lock().await.generation = generation;
    }

    pub(crate) async fn clear_runtime_targets(&self, generation: u64) {
        let _ = generation;
    }

    pub async fn set_generation(&self, generation: u64) {
        self.targets.lock().await.generation = generation;
    }

    pub async fn get_generation(&self) -> u64 {
        self.targets.lock().await.generation
    }

    pub async fn clear_targets(&self) {
        let mut targets = self.targets.lock().await;
        targets.generation += 1;
    }
}

impl Default for AppCommandBus {
    fn default() -> Self {
        Self::new()
    }
}
