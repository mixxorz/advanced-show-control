//! Show-owned application command handlers.

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::lv1::{ConnectionStatus, Lv1StateSnapshot};
use crate::show::show_file::LoadValidationReport;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::types::{SceneConfig, ShowDocument};

pub enum ShowCommand {
    GetShowDocument {
        reply: oneshot::Sender<ShowDocument>,
    },
    CurrentShowFilePath {
        reply: oneshot::Sender<Option<std::path::PathBuf>>,
    },
    GetLockout {
        reply: oneshot::Sender<bool>,
    },
    GetSceneConfig {
        scene_id: String,
        reply: oneshot::Sender<Option<SceneConfig>>,
    },
    InitialProjectionState {
        reply: oneshot::Sender<super::events::ShowProjectionState>,
    },
    SetLockout {
        enabled: bool,
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    SetSceneDuration {
        scene_id: String,
        duration_ms: u64,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetSceneScopeFadersEnabled {
        scene_id: String,
        enabled: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetSceneScopePanEnabled {
        scene_id: String,
        enabled: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetChannelScoped {
        scene_id: String,
        group: i32,
        channel: i32,
        scoped: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetAllChannelsScoped {
        scene_id: String,
        scoped: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    CueScene {
        scene_id: String,
        reply: Option<oneshot::Sender<Result<CueSceneResult, String>>>,
    },
    SelectSceneConfig {
        scene_id: String,
        reply: Option<oneshot::Sender<Result<SelectedSceneResult, String>>>,
    },
    NewShowFileFromCurrentLv1 {
        reply: Option<oneshot::Sender<Result<NewShowFileResult, String>>>,
    },
    SaveShowFileAs {
        path: std::path::PathBuf,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetDiscoveredLv1Systems {
        systems: Vec<DiscoveredLv1System>,
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    RefreshLv1Discovery {
        timeout_ms: Option<u64>,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetPendingLv1Identity {
        identity: Option<Lv1SystemIdentity>,
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    EstablishConnectedLv1Identity {
        identity: Lv1SystemIdentity,
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    ClearConnectedLv1Identity {
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    SetReconnectState {
        reconnect: ReconnectState,
        reply: Option<oneshot::Sender<ShowCommandResult>>,
    },
    LoadShowFileFromPath {
        path: std::path::PathBuf,
        reply: Option<oneshot::Sender<Result<LoadShowFileResult, String>>>,
    },
    StoreSceneConfigFromCurrentLv1 {
        scene_id: String,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    #[cfg(test)]
    ReplaceSnapshotForTest {
        snapshot: ShowDocument,
        reply: Option<oneshot::Sender<()>>,
    },
    #[cfg(test)]
    ClearForTest {
        reply: Option<oneshot::Sender<()>>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CueSceneResult {
    pub changed: bool,
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedSceneResult {
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewShowFileResult {
    pub selected_scene_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadShowFileResult {
    pub selected_scene_id: Option<String>,
    pub saved_at: String,
    #[serde(skip)]
    pub report: LoadValidationReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecallSceneResult {
    pub scene: SceneConfig,
    pub lv1_scene_index: i32,
}

pub fn validate_recall_scene_request(
    show: &super::types::ShowDocument,
    lv1: &Lv1StateSnapshot,
    scene_id: &str,
) -> Result<RecallSceneResult, String> {
    if show.lockout {
        return Err("Recall blocked: lockout is enabled".to_string());
    }

    let scene = show
        .scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id.to_string() == scene_id)
        .cloned()
        .ok_or_else(|| "Scene config not found".to_string())?;

    if lv1.connection != ConnectionStatus::Connected {
        return Err("Recall blocked: LV1 is disconnected".to_string());
    }

    let lv1_scene = lv1
        .scene_list
        .iter()
        .find(|candidate| {
            scene
                .scene_index
                .map(|scene_index| {
                    candidate.index == scene_index && candidate.name == scene.scene_name
                })
                .unwrap_or(false)
        })
        .ok_or_else(|| "Recall blocked: scene identity mismatch".to_string())?;

    Ok(RecallSceneResult {
        scene,
        lv1_scene_index: lv1_scene.index,
    })
}
