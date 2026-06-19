//! Show-owned application command handlers.

use crate::lv1::types::ChannelInfo;
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
