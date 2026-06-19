//! Show-owned application command handlers.

use crate::lv1::types::{ChannelInfo, ConnectionStatus, Lv1StateSnapshot};
use crate::show::show_file::{LoadValidationReport, ShowFile, export_show_file, import_show_file};

use super::handle::ShowStateHandle;
use super::types::SceneConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShowCommandResult {
    pub changed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CueSceneResult {
    pub changed: bool,
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SelectedSceneResult {
    pub scene: SceneConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewShowFileResult {
    pub selected_scene_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadShowFileResult {
    pub selected_scene_id: Option<String>,
    pub saved_at: String,
    pub report: LoadValidationReport,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecallSceneResult {
    pub scene: SceneConfig,
    pub lv1_scene_index: i32,
}

pub async fn set_lockout(show: &ShowStateHandle, enabled: bool) -> ShowCommandResult {
    ShowCommandResult {
        changed: show.set_lockout(enabled).await,
    }
}

pub async fn set_scene_duration_ms(
    show: &ShowStateHandle,
    scene_id: String,
    duration_ms: u64,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show.set_scene_duration(scene_id, duration_ms).await?,
    })
}

pub async fn set_scene_scope_faders_enabled(
    show: &ShowStateHandle,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show
            .set_scene_scope_faders_enabled(scene_id, enabled)
            .await?,
    })
}

pub async fn set_scene_scope_pan_enabled(
    show: &ShowStateHandle,
    scene_id: String,
    enabled: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show.set_scene_scope_pan_enabled(scene_id, enabled).await?,
    })
}

pub async fn set_channel_scoped(
    show: &ShowStateHandle,
    scene_id: String,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show
            .set_channel_scoped(scene_id, group, channel, scoped)
            .await?,
    })
}

pub async fn set_all_channels_scoped(
    show: &ShowStateHandle,
    scene_id: String,
    scoped: bool,
) -> Result<ShowCommandResult, String> {
    Ok(ShowCommandResult {
        changed: show.set_all_channels_scoped(scene_id, scoped).await?,
    })
}

pub async fn cue_scene(show: &ShowStateHandle, scene_id: String) -> Result<CueSceneResult, String> {
    let scene = show
        .get_scene_config(scene_id.clone())
        .await
        .ok_or_else(|| "Scene config not found".to_string())?;
    Ok(CueSceneResult {
        changed: show.cue_scene(scene_id).await?,
        scene,
    })
}

pub async fn select_scene_config(
    show: &ShowStateHandle,
    scene_id: String,
) -> Result<SelectedSceneResult, String> {
    let scene = show
        .get_scene_config(scene_id)
        .await
        .ok_or_else(|| "Scene config not found".to_string())?;
    Ok(SelectedSceneResult { scene })
}

pub fn validate_recall_scene_request(
    show: &super::types::ShowSnapshot,
    lv1: &Lv1StateSnapshot,
    scene_id: &str,
) -> Result<RecallSceneResult, String> {
    if show.lockout {
        return Err("Recall blocked: lockout is enabled".to_string());
    }

    let scene = show
        .scene_configs
        .iter()
        .find(|scene| scene.scene_id == scene_id)
        .cloned()
        .ok_or_else(|| "Scene config not found".to_string())?;

    if lv1.connection != ConnectionStatus::Connected {
        return Err("Recall blocked: LV1 is disconnected".to_string());
    }

    let lv1_scene = lv1
        .scene_list
        .iter()
        .find(|candidate| {
            candidate.index == scene.scene_index && candidate.name == scene.scene_name
        })
        .ok_or_else(|| "Recall blocked: scene identity mismatch".to_string())?;

    Ok(RecallSceneResult {
        scene,
        lv1_scene_index: lv1_scene.index,
    })
}

pub async fn store_scene_config(
    show: &ShowStateHandle,
    scene_id: String,
    channels: Vec<ChannelInfo>,
) -> Result<ShowCommandResult, String> {
    if show.get_scene_config(scene_id.clone()).await.is_none() {
        return Err("Scene config not found".to_string());
    }

    Ok(ShowCommandResult {
        changed: show.store_scene_config(scene_id, channels).await?,
    })
}

pub async fn new_show_file(
    show: &ShowStateHandle,
    lv1: Option<crate::lv1::types::Lv1StateSnapshot>,
) -> Result<NewShowFileResult, String> {
    show.clear().await;
    if let Some(lv1) = lv1
        && !lv1.scene_list.is_empty()
    {
        show.reconcile_scene_list(lv1.scene_list).await;
    }

    let selected_scene_id = show
        .get_snapshot()
        .await
        .scene_configs
        .first()
        .map(|scene| scene.scene_id.clone());

    Ok(NewShowFileResult { selected_scene_id })
}

pub async fn export_show_file_for_save(show: &ShowStateHandle, saved_at: String) -> ShowFile {
    export_show_file(show.get_snapshot().await, saved_at)
}

pub async fn load_show_file_from_dto(
    show: &ShowStateHandle,
    file: &mut ShowFile,
    lv1: crate::lv1::types::Lv1StateSnapshot,
) -> Result<LoadShowFileResult, String> {
    let imported = import_show_file(file, &lv1)?;
    show.replace_snapshot(imported.snapshot).await;

    Ok(LoadShowFileResult {
        selected_scene_id: imported.selected_scene_id,
        saved_at: file.saved_at.clone(),
        report: imported.report,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lv1::types::{ConnectionStatus, Lv1StateSnapshot, SceneListEntry};
    use crate::show::types::{SceneConfig, SceneScopeToggles, ShowSnapshot};

    fn recall_show(lockout: bool) -> ShowSnapshot {
        ShowSnapshot {
            lockout,
            scene_configs: vec![SceneConfig {
                scene_id: "1::Verse".to_string(),
                scene_index: 1,
                scene_name: "Verse".to_string(),
                duration_ms: 0,
                channel_configs: Vec::new(),
                scoped_channels: Vec::new(),
                scope_toggles: SceneScopeToggles::default(),
            }],
            cued_scene_id: Some("1::Verse".to_string()),
        }
    }

    fn recall_lv1(connection: ConnectionStatus, name: &str) -> Lv1StateSnapshot {
        Lv1StateSnapshot {
            connection,
            scene: None,
            scene_list: vec![SceneListEntry {
                index: 1,
                name: name.to_string(),
            }],
            channels: Vec::new(),
        }
    }

    #[test]
    fn validate_recall_scene_request_blocks_lockout_before_lv1_identity() {
        let show = recall_show(true);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Different");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: lockout is enabled");
    }

    #[test]
    fn validate_recall_scene_request_blocks_missing_scene_config() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Verse");

        let err = validate_recall_scene_request(&show, &lv1, "2::Chorus").unwrap_err();

        assert_eq!(err, "Scene config not found");
    }

    #[test]
    fn validate_recall_scene_request_blocks_disconnected_lv1() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Disconnected, "Verse");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: LV1 is disconnected");
    }

    #[test]
    fn validate_recall_scene_request_blocks_scene_identity_mismatch() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Different");

        let err = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap_err();

        assert_eq!(err, "Recall blocked: scene identity mismatch");
    }

    #[test]
    fn validate_recall_scene_request_returns_matching_lv1_scene_index() {
        let show = recall_show(false);
        let lv1 = recall_lv1(ConnectionStatus::Connected, "Verse");

        let result = validate_recall_scene_request(&show, &lv1, "1::Verse").unwrap();

        assert_eq!(result.scene.scene_id, "1::Verse");
        assert_eq!(result.lv1_scene_index, 1);
    }
}
