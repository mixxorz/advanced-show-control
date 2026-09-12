use super::map_app_command_error;
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use crate::scenes::{RecallSceneResult, ScenesCommand, ScenesCommandResult, SelectedSceneResult};
use tauri::State;
use tokio::sync::oneshot;

#[tauri::command]
pub async fn recall_scene(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<RecallSceneResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::RecallScene {
        internal_scene_id,
        reply,
    })
    .await?
    .map_err(map_app_command_error)
}

#[tauri::command]
pub async fn set_scene_duration_ms(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    duration_ms: u64,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::SetSceneDuration {
        internal_scene_id,
        duration_ms,
        reply: Some(reply),
    })
    .await?
}

#[tauri::command]
pub async fn link_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    source_internal_scene_id: uuid::Uuid,
    target_scene_index: i32,
    overwrite_existing: bool,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::LinkSceneConfig {
        source_internal_scene_id,
        target_scene_index,
        overwrite_existing,
        reply: Some(reply),
    })
    .await?
}

#[tauri::command]
pub async fn delete_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::DeleteSceneConfig {
        internal_scene_id,
        reply: Some(reply),
    })
    .await?
}

#[tauri::command]
pub async fn copy_scene_settings(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::CopySceneSettings {
        source_internal_scene_id: internal_scene_id,
        reply: Some(reply),
    })
    .await?
}

#[tauri::command]
pub async fn paste_scene_settings(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::PasteSceneSettings {
        destination_internal_scene_id: internal_scene_id,
        reply: Some(reply),
    })
    .await?
}

#[tauri::command]
pub async fn select_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<SelectedSceneResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::SelectSceneConfig {
        internal_scene_id,
        reply: Some(reply),
    })
    .await?
}

#[tauri::command]
pub async fn store_scene_config(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| {
        ScenesCommand::StoreSceneConfigFromCurrentLv1 {
            internal_scene_id,
            reply: Some(reply),
        }
    })
    .await?
}

#[cfg(test)]
mod tests {
    #[test]
    fn scenes_unavailable_has_scene_specific_message() {
        assert_eq!(
            super::map_app_command_error(
                crate::runtime::errors::AppCommandError::ScenesUnavailable
            ),
            "scene state is unavailable"
        );
    }
}

#[tauri::command]
pub async fn set_all_channels_scoped(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    scoped: bool,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::SetAllChannelsScoped {
        internal_scene_id,
        scoped,
        reply: Some(reply),
    })
    .await?
}

#[tauri::command]
pub async fn set_scene_scope_faders_enabled(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    enabled: bool,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| {
        ScenesCommand::SetSceneScopeFadersEnabled {
            internal_scene_id,
            enabled,
            reply: Some(reply),
        }
    })
    .await?
}

#[tauri::command]
pub async fn set_scene_scope_pan_enabled(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    enabled: bool,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::SetSceneScopePanEnabled {
        internal_scene_id,
        enabled,
        reply: Some(reply),
    })
    .await?
}

#[tauri::command]
pub async fn set_channel_scoped(
    lifecycle: State<'_, AppLifecycle>,
    internal_scene_id: uuid::Uuid,
    group: i32,
    channel: i32,
    scoped: bool,
) -> Result<ScenesCommandResult, String> {
    send_scene_command(lifecycle, |reply| ScenesCommand::SetChannelScoped {
        internal_scene_id,
        group,
        channel,
        scoped,
        reply: Some(reply),
    })
    .await?
}

/// @cc [owner:mixxorz,label:architecture] scene-commands-use-owner-reply
/// Scene adapters MUST construct an explicit `ScenesCommand`, send it to the app-lifetime Scenes
/// owner, and await the caller-specific reply type; mailbox send and dropped-reply failures MUST be
/// mapped to frontend-safe errors without performing scene validation or mutation in this helper.
async fn send_scene_command<T>(
    lifecycle: State<'_, AppLifecycle>,
    build_command: impl FnOnce(oneshot::Sender<T>) -> ScenesCommand,
) -> Result<T, String> {
    let (reply, response) = oneshot::channel();
    lifecycle
        .scenes_handle()
        .send(build_command(reply))
        .await
        .map_err(|_| AppCommandError::ScenesUnavailable)
        .map_err(map_app_command_error)?;
    response
        .await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)
}
