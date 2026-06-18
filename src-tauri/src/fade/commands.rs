use tokio::sync::oneshot;

use crate::fade::types::FadeConfig;
use crate::runtime::commands::AppCommandError;

#[derive(Debug)]
pub enum FadeCommand {
    RecallSceneFade {
        config: FadeConfig,
        expected_generation: Option<u64>,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    AbortAll {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
}
