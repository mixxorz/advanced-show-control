use super::map_app_command_error;
use crate::cue_lists::{CueListsCommand, CueListsCommandResult, CueRecallResult};
use crate::lifecycle::AppLifecycle;
use crate::runtime::errors::AppCommandError;
use tauri::State;
use tokio::sync::oneshot;
use uuid::Uuid;

#[tauri::command]
pub async fn create_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    name: String,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| CueListsCommand::CreateCueList {
        name,
        reply: Some(reply),
    })
    .await
}

#[tauri::command]
pub async fn rename_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    cue_list_id: Uuid,
    name: String,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| CueListsCommand::RenameCueList {
        cue_list_id,
        name,
        reply: Some(reply),
    })
    .await
}

#[tauri::command]
pub async fn delete_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    cue_list_id: Uuid,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| CueListsCommand::DeleteCueList {
        cue_list_id,
        reply: Some(reply),
    })
    .await
}

#[tauri::command]
pub async fn reorder_cue_lists(
    lifecycle: State<'_, AppLifecycle>,
    ordered_ids: Vec<Uuid>,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| CueListsCommand::ReorderCueLists {
        ordered_ids,
        reply: Some(reply),
    })
    .await
}

#[tauri::command]
pub async fn set_active_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    cue_list_id: Option<Uuid>,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| CueListsCommand::SetActiveCueList {
        cue_list_id,
        reply: Some(reply),
    })
    .await
}

#[tauri::command]
pub async fn add_scene_to_active_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    scene_internal_id: Uuid,
    insert_index: usize,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| {
        CueListsCommand::AddSceneToActiveCueList {
            scene_internal_id,
            insert_index,
            reply: Some(reply),
        }
    })
    .await
}

#[tauri::command]
pub async fn remove_cue_entry(
    lifecycle: State<'_, AppLifecycle>,
    cue_entry_id: Uuid,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| CueListsCommand::RemoveCueEntry {
        cue_entry_id,
        reply: Some(reply),
    })
    .await
}

#[tauri::command]
pub async fn reorder_cue_entries(
    lifecycle: State<'_, AppLifecycle>,
    ordered_entry_ids: Vec<Uuid>,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| CueListsCommand::ReorderCueEntries {
        ordered_entry_ids,
        reply: Some(reply),
    })
    .await
}

#[tauri::command]
pub async fn cue_entry(
    lifecycle: State<'_, AppLifecycle>,
    cue_entry_id: Option<Uuid>,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(lifecycle, |reply| CueListsCommand::CueEntry {
        cue_entry_id,
        reply: Some(reply),
    })
    .await
}

#[tauri::command]
pub async fn recall_cued_cue(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<CueRecallResult, String> {
    let cue_lists = lifecycle.cue_lists_handle();
    let (reply, rx) = oneshot::channel();
    cue_lists
        .send(CueListsCommand::RecallCuedCue { reply })
        .await
        .map_err(|_| AppCommandError::CommandFailed("cue lists are unavailable".to_string()))
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
        .map_err(map_app_command_error)
}

async fn send_cue_lists_mutation(
    lifecycle: State<'_, AppLifecycle>,
    build_command: impl FnOnce(
        oneshot::Sender<Result<CueListsCommandResult, String>>,
    ) -> CueListsCommand,
) -> Result<CueListsCommandResult, String> {
    let cue_lists = lifecycle.cue_lists_handle();
    let (reply, rx) = oneshot::channel();
    cue_lists
        .send(build_command(reply))
        .await
        .map_err(|_| AppCommandError::CommandFailed("cue lists are unavailable".to_string()))
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}
