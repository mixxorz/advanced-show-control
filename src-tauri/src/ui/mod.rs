//! Tauri adapter layer.
//!
//! This module will contain command registration and frontend serialization
//! boundaries. Business logic should route through `crate::runtime::commands::AppCommandBus`.

use crate::app_state::ShellState;
use crate::commands;
use crate::lifecycle::AppLifecycle;
use crate::logging;
use tauri::Manager;

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .manage(ShellState::default())
        .manage(AppLifecycle::default())
        .setup(|app| {
            let shell_state = (*app.state::<ShellState>()).clone();
            let logging_guard = logging::init_logging(app.handle(), shell_state.clone())?;
            app.manage(logging_guard);
            tracing::info!(event = "app_started", "Starting Advanced Show Control");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::refresh_lv1_discovery,
            commands::new_show_file,
            commands::open_show_file_dialog,
            commands::save_show_file,
            commands::save_show_file_as_dialog,
            commands::set_scene_duration_ms,
            commands::select_scene_config,
            commands::cue_scene,
            commands::recall_scene,
            commands::connect_lv1,
            commands::connect_lv1_system,
            commands::attempt_reconnect_lv1,
            commands::startup_auto_connect_lv1,
            commands::disconnect_lv1,
            commands::reconnect_timed_out,
            commands::abort_all_fades,
            commands::store_scene_config,
            commands::set_channel_scoped,
            commands::set_all_channels_scoped,
            commands::set_scene_scope_faders_enabled,
            commands::set_scene_scope_pan_enabled,
            commands::set_lockout,
        ])
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_app_constructs_builder() {
        let _builder = super::build_app();
    }
}
