use tokio::sync::oneshot;

use crate::runtime::errors::AppCommandError;
use crate::show::RecallSceneResult;

#[derive(Debug)]
pub enum ScenesCommand {
    RecallScene {
        scene_id: String,
        reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
    },
    Shutdown,
}
