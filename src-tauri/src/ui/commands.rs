//! Tauri command adapter exports.
//!
//! This module is the frontend command registration surface.

use crate::lifecycle::AppLifecycle;
use crate::ui::UiLogReceiverState;
use tauri::{AppHandle, Manager, Runtime, State};

pub use crate::commands::{
    abort_all_fades, attempt_reconnect_lv1, connect_lv1, connect_lv1_system, cue_scene,
    disconnect_lv1, get_app_status, new_show_file, open_show_file_dialog, recall_scene,
    reconnect_timed_out, refresh_lv1_discovery, save_show_file, save_show_file_as_dialog,
    select_scene_config, set_all_channels_scoped, set_channel_scoped, set_lockout,
    set_scene_duration_ms, set_scene_scope_faders_enabled, set_scene_scope_pan_enabled,
    startup_auto_connect_lv1, store_scene_config,
};

pub use crate::commands::{
    __cmd__abort_all_fades, __cmd__attempt_reconnect_lv1, __cmd__connect_lv1,
    __cmd__connect_lv1_system, __cmd__cue_scene, __cmd__disconnect_lv1, __cmd__get_app_status,
    __cmd__new_show_file, __cmd__open_show_file_dialog, __cmd__recall_scene,
    __cmd__reconnect_timed_out, __cmd__refresh_lv1_discovery, __cmd__save_show_file,
    __cmd__save_show_file_as_dialog, __cmd__select_scene_config, __cmd__set_all_channels_scoped,
    __cmd__set_channel_scoped, __cmd__set_lockout, __cmd__set_scene_duration_ms,
    __cmd__set_scene_scope_faders_enabled, __cmd__set_scene_scope_pan_enabled,
    __cmd__startup_auto_connect_lv1, __cmd__store_scene_config,
};

pub use crate::commands::{
    __tauri_command_name_abort_all_fades, __tauri_command_name_attempt_reconnect_lv1,
    __tauri_command_name_connect_lv1, __tauri_command_name_connect_lv1_system,
    __tauri_command_name_cue_scene, __tauri_command_name_disconnect_lv1,
    __tauri_command_name_get_app_status, __tauri_command_name_new_show_file,
    __tauri_command_name_open_show_file_dialog, __tauri_command_name_recall_scene,
    __tauri_command_name_reconnect_timed_out, __tauri_command_name_refresh_lv1_discovery,
    __tauri_command_name_save_show_file, __tauri_command_name_save_show_file_as_dialog,
    __tauri_command_name_select_scene_config, __tauri_command_name_set_all_channels_scoped,
    __tauri_command_name_set_channel_scoped, __tauri_command_name_set_lockout,
    __tauri_command_name_set_scene_duration_ms,
    __tauri_command_name_set_scene_scope_faders_enabled,
    __tauri_command_name_set_scene_scope_pan_enabled,
    __tauri_command_name_startup_auto_connect_lv1, __tauri_command_name_store_scene_config,
};

#[tauri::command]
pub async fn frontend_ready<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: State<'_, AppLifecycle>,
) -> Result<(), String> {
    let logs = app.state::<UiLogReceiverState>().subscribe();
    lifecycle.frontend_ready(app, logs).await
}
