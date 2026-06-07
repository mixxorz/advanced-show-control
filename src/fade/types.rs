use tokio::sync::oneshot;

use crate::fade::curve::FadeCurve;
use crate::runtime::commands::AppCommandError;

#[derive(Debug, Clone, PartialEq)]
pub struct FadeTarget {
    pub group: i32,
    pub channel: i32,
    pub target_db: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FadeConfig {
    pub targets: Vec<FadeTarget>,
    pub duration_ms: u64,
    pub curve: FadeCurve,
}

#[derive(Debug)]
pub enum FadeCommand {
    StartFade {
        config: FadeConfig,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    AbortAll {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    FinishNow {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
}

#[derive(Debug, Clone)]
pub enum FadeEvent {
    FadeStarted,
    FadeCompleted,
    FadeAborted,
    ChannelOverride { group: i32, channel: i32 },
    ChannelCancelled { group: i32, channel: i32 },
}
