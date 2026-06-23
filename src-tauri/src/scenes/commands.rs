use tokio::sync::oneshot;

use crate::runtime::errors::AppCommandError;
use crate::show::RecallSceneResult;
use uuid::Uuid;

#[derive(Debug)]
pub enum ScenesCommand {
    RecallScene {
        internal_scene_id: Uuid,
        reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
    },
    Shutdown,
}
