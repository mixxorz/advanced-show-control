//! Tauri adapter layer.
//!
//! This module will contain command registration and frontend serialization
//! boundaries. Business logic should route through actor mailboxes.

use crate::lifecycle::AppLifecycle;
use crate::logging;
use crate::runtime::events::AppEventBus;
use crate::show::ShowStateHandle;
use tauri::Manager;
use tokio::sync::broadcast;

pub mod commands;

pub type UiLogReceiverState = broadcast::Sender<logging::UiLogEvent>;

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let event_bus = AppEventBus::default();
            let show = ShowStateHandle::new_empty(event_bus.clone());
            let lifecycle = AppLifecycle::new(event_bus, show.clone());
            let logging_runtime = logging::init_logging(app.handle())?;
            app.manage(show);
            app.manage(lifecycle);
            app.manage(logging_runtime.guard);
            app.manage(logging_runtime.ui_logs);
            tracing::info!(event = "app_started", "Starting Advanced Show Control");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::frontend_ready,
            commands::refresh_lv1_discovery,
            commands::new_show_file,
            commands::open_show_file_dialog,
            commands::save_show_file,
            commands::save_show_file_as_dialog,
            commands::set_scene_duration_ms,
            commands::select_scene_config,
            commands::cue_scene,
            commands::recall_scene,
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
        let _ = super::commands::frontend_ready::<tauri::Wry>;
        let _ = super::commands::disconnect_lv1;
        let _ = super::commands::recall_scene;
        let _ = super::commands::set_lockout;
    }

    #[test]
    fn invoke_handler_includes_frontend_ready() {
        let _ = super::commands::frontend_ready::<tauri::Wry>;
    }
}
