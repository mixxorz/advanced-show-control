use tokio::sync::oneshot;
use tokio::time::Instant;

use crate::fade::types::FadeConfig;
use crate::runtime::errors::AppCommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSceneRecallBehavior {
    FinishActiveTargets,
    OverrideMatchingTargets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecallReadinessCancellation {
    Aborted,
    Disconnected,
    GenerationChanged,
    Superseded,
    ActorStopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecallReadinessError {
    TimedOut {
        generation: u64,
        scene_index: i32,
        scene_name: String,
        observed_ping_count: u8,
    },
    Cancelled(RecallReadinessCancellation),
}

#[derive(Debug)]
pub struct RecallReadinessRequest {
    pub deadline: Instant,
    pub completion: Option<oneshot::Sender<Result<(), RecallReadinessError>>>,
}

impl RecallReadinessRequest {
    pub fn detached(deadline: Instant) -> Self {
        Self {
            deadline,
            completion: None,
        }
    }
}

#[derive(Debug)]
pub enum FadeCommand {
    RecallSceneFade {
        config: FadeConfig,
        same_scene_behavior: SameSceneRecallBehavior,
        expected_generation: Option<u64>,
        readiness: RecallReadinessRequest,
        reply: Option<oneshot::Sender<Result<(), AppCommandError>>>,
    },
    WaitForRecallReadiness {
        scene: crate::fade::types::FadeSceneIdentity,
        expected_generation: u64,
        readiness: RecallReadinessRequest,
        reply: Option<oneshot::Sender<Result<(), AppCommandError>>>,
    },
    AbortAll {
        reply: Option<oneshot::Sender<Result<(), AppCommandError>>>,
    },
}
