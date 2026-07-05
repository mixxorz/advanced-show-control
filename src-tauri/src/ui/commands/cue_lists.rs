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
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::CreateCueList { name, reply: None },
    )
    .await
}

#[tauri::command]
pub async fn rename_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    cue_list_id: Uuid,
    name: String,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::RenameCueList {
            cue_list_id,
            name,
            reply: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    cue_list_id: Uuid,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::DeleteCueList {
            cue_list_id,
            reply: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn reorder_cue_lists(
    lifecycle: State<'_, AppLifecycle>,
    ordered_ids: Vec<Uuid>,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::ReorderCueLists {
            ordered_ids,
            reply: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn set_active_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    cue_list_id: Option<Uuid>,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::SetActiveCueList {
            cue_list_id,
            reply: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn add_scene_to_active_cue_list(
    lifecycle: State<'_, AppLifecycle>,
    scene_internal_id: Uuid,
    insert_index: usize,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::AddSceneToActiveCueList {
            scene_internal_id,
            insert_index,
            reply: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn remove_cue_entry(
    lifecycle: State<'_, AppLifecycle>,
    cue_entry_id: Uuid,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::RemoveCueEntry {
            cue_entry_id,
            reply: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn reorder_cue_entries(
    lifecycle: State<'_, AppLifecycle>,
    ordered_entry_ids: Vec<Uuid>,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::ReorderCueEntries {
            ordered_entry_ids,
            reply: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn cue_entry(
    lifecycle: State<'_, AppLifecycle>,
    cue_entry_id: Option<Uuid>,
) -> Result<CueListsCommandResult, String> {
    send_cue_lists_mutation(
        lifecycle,
        CueListsCommand::CueEntry {
            cue_entry_id,
            reply: None,
        },
    )
    .await
}

#[tauri::command]
pub async fn recall_cued_cue(
    lifecycle: State<'_, AppLifecycle>,
) -> Result<CueRecallResult, String> {
    let cue_lists = lifecycle
        .current_cue_lists()
        .await
        .ok_or(AppCommandError::CommandFailed(
            "cue lists are unavailable".to_string(),
        ))
        .map_err(map_app_command_error)?;
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
    command: CueListsCommand,
) -> Result<CueListsCommandResult, String> {
    let cue_lists = lifecycle
        .current_cue_lists()
        .await
        .ok_or(AppCommandError::CommandFailed(
            "cue lists are unavailable".to_string(),
        ))
        .map_err(map_app_command_error)?;
    let (reply, rx) = oneshot::channel();
    let command = match command {
        CueListsCommand::CreateCueList { name, .. } => CueListsCommand::CreateCueList {
            name,
            reply: Some(reply),
        },
        CueListsCommand::RenameCueList {
            cue_list_id, name, ..
        } => CueListsCommand::RenameCueList {
            cue_list_id,
            name,
            reply: Some(reply),
        },
        CueListsCommand::DeleteCueList { cue_list_id, .. } => CueListsCommand::DeleteCueList {
            cue_list_id,
            reply: Some(reply),
        },
        CueListsCommand::ReorderCueLists { ordered_ids, .. } => CueListsCommand::ReorderCueLists {
            ordered_ids,
            reply: Some(reply),
        },
        CueListsCommand::SetActiveCueList { cue_list_id, .. } => {
            CueListsCommand::SetActiveCueList {
                cue_list_id,
                reply: Some(reply),
            }
        }
        CueListsCommand::AddSceneToActiveCueList {
            scene_internal_id,
            insert_index,
            ..
        } => CueListsCommand::AddSceneToActiveCueList {
            scene_internal_id,
            insert_index,
            reply: Some(reply),
        },
        CueListsCommand::RemoveCueEntry { cue_entry_id, .. } => CueListsCommand::RemoveCueEntry {
            cue_entry_id,
            reply: Some(reply),
        },
        CueListsCommand::ReorderCueEntries {
            ordered_entry_ids, ..
        } => CueListsCommand::ReorderCueEntries {
            ordered_entry_ids,
            reply: Some(reply),
        },
        CueListsCommand::CueEntry { cue_entry_id, .. } => CueListsCommand::CueEntry {
            cue_entry_id,
            reply: Some(reply),
        },
        _ => unreachable!(),
    };
    cue_lists
        .send(command)
        .await
        .map_err(|_| AppCommandError::CommandFailed("cue lists are unavailable".to_string()))
        .map_err(map_app_command_error)?;
    rx.await
        .map_err(|_| AppCommandError::ReplyChannelClosed)
        .map_err(map_app_command_error)?
}
