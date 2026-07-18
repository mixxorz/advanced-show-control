use tokio::sync::oneshot;

use crate::fade::types::FadeConfig;
use crate::runtime::errors::AppCommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SameSceneRecallBehavior {
    FinishActiveTargets,
    OverrideMatchingTargets,
}

#[derive(Debug)]
pub enum FadeCommand {
    RecallSceneFade {
        config: FadeConfig,
        same_scene_behavior: SameSceneRecallBehavior,
        expected_generation: Option<u64>,
        reply: Option<oneshot::Sender<Result<(), AppCommandError>>>,
    },
    AbortAll {
        reply: Option<oneshot::Sender<Result<(), AppCommandError>>>,
    },
}
