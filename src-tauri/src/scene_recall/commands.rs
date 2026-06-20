use tokio::sync::oneshot;

use crate::show::RecallSceneResult;

#[derive(Debug)]
pub enum SceneRecallCommand {
    RecallScene {
        scene_id: String,
        reply:
            oneshot::Sender<Result<RecallSceneResult, crate::runtime::commands::AppCommandError>>,
    },
    Shutdown,
}
