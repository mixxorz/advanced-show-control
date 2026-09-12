use tokio::sync::oneshot;

use crate::lv1::{ConnectionStatus, Lv1StateSnapshot};
use crate::runtime::errors::AppCommandError;
use crate::scenes::{SceneConfig, SceneDocument};
use uuid::Uuid;

#[derive(Debug)]
pub enum ScenesCommand {
    GetSessionDocument {
        reply: oneshot::Sender<crate::session::SessionDocument>,
    },
    /// @cc [owner:mixxorz,label:persistence] session-replacement-single-owner-turn
    /// Session replacement MUST commit scene and cue documents in one owner turn under the
    /// replacement ticket and expected-generation check, cancel queued recall and cue-advance
    /// intent without aborting an active fade, and publish one combined `SessionReplaced`
    /// projection.
    ReplaceSessionDocument {
        replacement: crate::session::SessionReplacement,
        expected_generation: u64,
        reply: oneshot::Sender<Result<crate::session::SessionDocument, String>>,
    },
    GetSceneConfig {
        internal_scene_id: Uuid,
        reply: oneshot::Sender<Option<SceneConfig>>,
    },
    InitialProjectionState {
        reply: oneshot::Sender<crate::scenes::ScenesProjectionState>,
    },
    /// @cc [owner:mixxorz,label:safety] runtime-readiness-handoff
    /// Readiness MUST be accepted only when the supplied generation is still authoritative and a
    /// complete peer pair for that generation is installed; a same-generation scene list cached
    /// before handoff MUST supersede the initial list supplied by lifecycle.
    RuntimePeersReady {
        generation: u64,
        initial_scene_list: Vec<crate::lv1::SceneListEntry>,
        reply: oneshot::Sender<Result<(), AppCommandError>>,
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
    SelectSceneConfig {
        internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<SelectedSceneResult, String>>>,
    },
    CopySceneSettings {
        source_internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    PasteSceneSettings {
        destination_internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    StoreSceneConfigFromCurrentLv1 {
        internal_scene_id: Uuid,
        reply: Option<oneshot::Sender<Result<ScenesCommandResult, String>>>,
    },
    RecallScene {
        internal_scene_id: Uuid,
        reply: oneshot::Sender<Result<RecallSceneResult, AppCommandError>>,
    },
    AbortAll {
        reply: oneshot::Sender<Result<(), AppCommandError>>,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ScenesCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SelectedSceneResult {
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecallSceneResult {
    pub scene: SceneConfig,
    pub lv1_scene_index: i32,
}

/// @cc [owner:mixxorz,label:safety] explicit-recall-exact-identity
/// A recall request MUST be rejected unless its durable UUID resolves to a linked config and the
/// connected LV1 scene list contains the config's exact index-and-name identity; lockout MUST also
/// reject the request.
pub fn validate_recall_scene_request(
    lockout: bool,
    scene_document: &SceneDocument,
    lv1: &Lv1StateSnapshot,
    internal_scene_id: Uuid,
) -> Result<RecallSceneResult, String> {
    if lockout {
        return Err("Recall blocked: lockout is enabled".to_string());
    }

    let scene = scene_document
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
    use crate::scenes::SceneScopeToggles;

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
            ping_sequence: 0,
        }
    }

    #[test]
    fn validate_recall_scene_request_uses_internal_scene_id() {
        let id = Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap();
        let scenes = SceneDocument {
            selected_scene_internal_id: None,
            scene_configs: vec![scene_config(id, Some(1))],
        };

        let result = validate_recall_scene_request(false, &scenes, &lv1_snapshot(), id).unwrap();

        assert_eq!(result.lv1_scene_index, 1);
    }

    #[test]
    fn validate_recall_scene_request_rejects_unlinked_scene() {
        let id = Uuid::parse_str("55555555-5555-4555-8555-555555555555").unwrap();
        let scenes = SceneDocument {
            selected_scene_internal_id: None,
            scene_configs: vec![scene_config(id, None)],
        };

        let err = validate_recall_scene_request(false, &scenes, &lv1_snapshot(), id).unwrap_err();

        assert_eq!(err, "Recall blocked: scene is unlinked");
    }
}
