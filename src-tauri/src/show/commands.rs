//! Show-owned application command handlers.

use crate::connection_state::{DiscoveredLv1System, Lv1SystemIdentity, ReconnectState};
use crate::lv1::{ConnectionStatus, Lv1StateSnapshot};
use crate::show::show_file::LoadValidationReport;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;
use uuid::Uuid;

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
        internal_scene_id: Uuid,
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
        internal_scene_id: Uuid,
        duration_ms: u64,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetSceneScopeFadersEnabled {
        internal_scene_id: Uuid,
        enabled: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetSceneScopePanEnabled {
        internal_scene_id: Uuid,
        enabled: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    LinkSceneConfig {
        source_internal_scene_id: Uuid,
        target_scene_index: i32,
        overwrite_existing: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    DeleteSceneConfig {
        internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetChannelScoped {
        internal_scene_id: Uuid,
        group: i32,
        channel: i32,
        scoped: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    SetAllChannelsScoped {
        internal_scene_id: Uuid,
        scoped: bool,
        reply: Option<oneshot::Sender<Result<ShowCommandResult, String>>>,
    },
    CueScene {
        internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<CueSceneResult, String>>>,
    },
    SelectSceneConfig {
        internal_scene_id: Uuid,
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
        internal_scene_id: Uuid,
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
    internal_scene_id: Uuid,
) -> Result<RecallSceneResult, String> {
    if show.lockout {
        return Err("Recall blocked: lockout is enabled".to_string());
    }

    let scene = show
        .scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id == internal_scene_id)
        .cloned()
        .ok_or_else(|| "Scene config not found".to_string())?;

    let Some(scene_index) = scene.scene_index else {
        return Err("Recall blocked: scene is unlinked".to_string());
    };

    if lv1.connection != ConnectionStatus::Connected {
        return Err("Recall blocked: LV1 is disconnected".to_string());
    }

    let lv1_scene = lv1
        .scene_list
        .iter()
        .find(|candidate| candidate.index == scene_index && candidate.name == scene.scene_name)
        .ok_or_else(|| "Recall blocked: scene identity mismatch".to_string())?;

    Ok(RecallSceneResult {
        scene,
        lv1_scene_index: lv1_scene.index,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::{ConnectionStatus, SceneListEntry};
    use crate::show::{SceneScopeToggles, ShowDocument};

    fn scene_config(internal_scene_id: Uuid, scene_index: Option<i32>) -> SceneConfig {
        SceneConfig {
            internal_scene_id,
            scene_index,
            scene_name: "Intro".to_string(),
            duration_ms: 1_000,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    fn lv1_snapshot() -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection: ConnectionStatus::Connected,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: "Intro".to_string(),
            }],
            channels: Vec::new(),
        }
    }

    #[test]
    fn validate_recall_scene_request_uses_internal_scene_id() {
        let id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        let show = ShowDocument {
            lockout: false,
            cued_scene_internal_id: None,
            scene_configs: vec![scene_config(id, Some(1))],
        };

        let result = validate_recall_scene_request(&show, &lv1_snapshot(), id).unwrap();

        assert_eq!(result.lv1_scene_index, 1);
    }

    #[test]
    fn validate_recall_scene_request_rejects_unlinked_scene() {
        let id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
        let show = ShowDocument {
            lockout: false,
            cued_scene_internal_id: None,
            scene_configs: vec![scene_config(id, None)],
        };

        let err = validate_recall_scene_request(&show, &lv1_snapshot(), id).unwrap_err();

        assert_eq!(err, "Recall blocked: scene is unlinked");
    }
}
