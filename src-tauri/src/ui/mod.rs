//! Tauri adapter layer.
//!
//! This module will contain command registration and frontend serialization
//! boundaries. Business logic should route through `crate::runtime::commands::AppCommandBus`.

use crate::app_state::ShellState;
use crate::lifecycle::AppLifecycle;
use crate::logging;
use crate::runtime::events::AppEventBus;
use tauri::Manager;
use tokio::sync::mpsc;

pub mod commands;

pub type UiLogReceiverState = std::sync::Mutex<Option<mpsc::Receiver<logging::UiLogEvent>>>;

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    let event_bus = AppEventBus::default();
    tauri::Builder::default()
        .manage(ShellState::new(event_bus.clone()))
        .manage(AppLifecycle::default())
        .setup(|app| {
            let logging_runtime = logging::init_logging(app.handle())?;
            app.manage(logging_runtime.guard);
            app.manage(std::sync::Mutex::new(Some(logging_runtime.ui_logs)));
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

    #[test]
    fn command_adapter_exports_existing_command_names() {
        let _ = super::commands::get_app_status;
        let _ = super::commands::connect_lv1;
        let _ = super::commands::disconnect_lv1;
        let _ = super::commands::recall_scene;
        let _ = super::commands::set_lockout;
    }
}
