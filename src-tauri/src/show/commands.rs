//! Show-owned application command handlers.

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::scenes::SceneConfig;
use crate::show::show_file::LoadValidationReport;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::types::ShowDocument;

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
    InitialProjectionState {
        reply: oneshot::Sender<super::events::ShowProjectionState>,
    },
    SetLockout {
        enabled: bool,
        reply: Option<oneshot::Sender<ShowCommandResult>>,
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
#[allow(dead_code)]
pub struct CueSceneResult {
    pub changed: bool,
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct SelectedSceneResult {
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewShowFileResult {
    pub selected_scene_internal_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoadShowFileResult {
    pub selected_scene_internal_id: Option<String>,
    pub saved_at: String,
    #[serde(skip)]
    pub report: LoadValidationReport,
}
