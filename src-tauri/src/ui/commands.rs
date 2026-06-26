//! Tauri command adapter exports.

pub(crate) mod fade;
pub(crate) mod lifecycle;
pub(crate) mod scenes;
pub(crate) mod settings;
pub(crate) mod show;

pub use fade::abort_all_fades;
pub use lifecycle::{
    attempt_reconnect_lv1, connect_lv1_system, disconnect_lv1, frontend_ready, reconnect_timed_out,
    startup_auto_connect_lv1,
};
pub use scenes::{
    cue_scene, delete_scene_config, link_scene_config, recall_scene, select_scene_config,
    set_all_channels_scoped, set_channel_scoped, set_scene_duration_ms,
    set_scene_scope_faders_enabled, set_scene_scope_pan_enabled, store_scene_config,
};
pub use settings::replace_app_settings;
pub use show::{
    new_show_file, open_show_file_dialog, refresh_lv1_discovery, save_show_file,
    save_show_file_as_dialog, set_lockout,
};

pub(super) fn map_app_command_error(error: crate::runtime::errors::AppCommandError) -> String {
    match error {
        crate::runtime::errors::AppCommandError::CommandFailed(message) => message,
        other => other.to_string(),
    }
}
