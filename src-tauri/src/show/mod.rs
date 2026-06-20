mod capture;
mod commands;
mod events;
mod handle;
mod show_file;
mod state;
mod types;

#[cfg(test)]
pub(crate) use commands::replace_show_document_for_test;
pub use commands::{
    ConnectCommandResult, CueSceneResult, LoadShowFileResult, NewShowFileResult, RecallSceneResult,
    SelectedSceneResult, ShowCommandResult, clear_connected_lv1_identity, cue_scene,
    current_show_file_path, establish_connected_lv1_identity, export_show_file_for_save,
    export_show_file_snapshot, get_lockout, get_scene_config, get_show_document,
    handle_runtime_disconnected, load_show_file_from_dto, mark_show_file_saved, new_show_file,
    select_scene_config, set_all_channels_scoped, set_channel_scoped, set_discovered_lv1_systems,
    set_lockout, set_pending_lv1_identity, set_reconnect_state, set_scene_duration_ms,
    set_scene_scope_faders_enabled, set_scene_scope_pan_enabled, store_scene_config,
    validate_recall_scene_request,
};
pub use events::{ShowEvent, ShowProjectionReason, ShowProjectionState};
pub use handle::{ShowStateHandle, spawn_lv1_scene_list_monitor};
pub use show_file::{
    ImportedShowFile, LoadValidationReport, SHOW_FILE_SCHEMA_VERSION, ShowFile,
    ShowFileChannelConfig, ShowFileChannelRef, ShowFileSafety, ShowFileSceneConfig,
    ShowFileSceneScopeToggles, export_show_file, import_show_file, prune_show_file_to_lv1_scenes,
};
pub use state::ShowState;
pub use types::{
    ChannelConfig, ChannelRef, SceneConfig, SceneScopeToggles, ShowDocument, parse_scene_id,
    scene_id,
};
