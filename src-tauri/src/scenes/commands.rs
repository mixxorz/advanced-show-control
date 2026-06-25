use tokio::sync::oneshot;

use crate::runtime::errors::AppCommandError;
use crate::scenes::{SceneConfig, SceneDocument};
use uuid::Uuid;

use super::events::ScenesProjectionReason;

#[derive(Debug)]
pub enum ScenesCommand {
    GetSceneDocument {
        reply: oneshot::Sender<SceneDocument>,
    },
    GetSceneConfig {
        internal_scene_id: Uuid,
        reply: oneshot::Sender<Option<SceneConfig>>,
    },
    InitialProjectionState {
        reply: oneshot::Sender<crate::scenes::ScenesProjectionState>,
    },
    SetSceneDuration {
        internal_scene_id: Uuid,
        duration_ms: u64,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    SetSceneScopeFadersEnabled {
        internal_scene_id: Uuid,
        enabled: bool,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    SetSceneScopePanEnabled {
        internal_scene_id: Uuid,
        enabled: bool,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    LinkSceneConfig {
        source_internal_scene_id: Uuid,
        target_scene_index: i32,
        overwrite_existing: bool,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    DeleteSceneConfig {
        internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    SetChannelScoped {
        internal_scene_id: Uuid,
        group: i32,
        channel: i32,
        scoped: bool,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    SetAllChannelsScoped {
        internal_scene_id: Uuid,
        scoped: bool,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    CueScene {
        internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<CueSceneResult, String>>>,
    },
    SelectSceneConfig {
        internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<SelectedSceneResult, String>>>,
    },
    StoreSceneConfigFromCurrentLv1 {
        internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    ReplaceSceneDocument {
        document: SceneDocument,
        selected_scene_internal_id: Option<String>,
        reason: ScenesProjectionReason,
        persisted_scene_edit: bool,
        reply: Option<oneshot::Sender<ScenesCommandResult>>,
    },
    RecallScene {
        internal_scene_id: Uuid,
        reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScenesCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CueSceneResult {
    pub changed: bool,
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelectedSceneResult {
    pub scene: SceneConfig,
}

pub type RecallSceneResult = crate::show::RecallSceneResult;
