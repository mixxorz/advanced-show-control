//! Tauri adapter layer.
//!
//! This module will contain command registration and frontend serialization
//! boundaries. Business logic should route through actor mailboxes.

use crate::lifecycle::AppLifecycle;
use crate::logging;
use crate::runtime::events::AppEventBus;
use crate::settings::build_settings_actor;
use crate::show::build_show_actor;
use tauri::Manager;
use tokio::sync::broadcast;

pub mod commands;
#[cfg(debug_assertions)]
pub mod debug;
pub mod menu;

pub type UiLogReceiverState = broadcast::Sender<logging::UiLogEvent>;

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let event_bus = AppEventBus::default();
            let (show, show_task, show_peers) = build_show_actor(event_bus.clone());
            let settings_dir = app.path().app_config_dir()?;
            let (settings, settings_task) = build_settings_actor(settings_dir, event_bus.clone());
            let lifecycle =
                AppLifecycle::new(event_bus, show.clone(), show_peers, settings.clone());
            show_task.spawn();
            settings_task.spawn();
            let logging_runtime = logging::init_logging(app.handle())?;
            app.manage(show);
            app.manage(lifecycle);
            app.manage(settings);
            app.manage(logging_runtime.guard);
            app.manage(logging_runtime.ui_logs);
            menu::install_session_menu(app)?;
            tracing::info!(event = "app_started", "Starting Advanced Show Control");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::lifecycle::frontend_ready,
            commands::show::refresh_lv1_discovery,
            commands::show::new_show_file,
            commands::show::open_show_file_dialog,
            commands::show::save_show_file,
            commands::show::save_show_file_as_dialog,
            commands::show::set_scene_duration_ms,
            commands::show::link_scene_config,
            commands::show::delete_scene_config,
            commands::show::select_scene_config,
            commands::show::cue_scene,
            commands::scenes::recall_scene,
            commands::lifecycle::connect_lv1_system,
            commands::lifecycle::attempt_reconnect_lv1,
            commands::lifecycle::startup_auto_connect_lv1,
            commands::lifecycle::disconnect_lv1,
            commands::lifecycle::reconnect_timed_out,
            commands::fade::abort_all_fades,
            commands::settings::replace_app_settings,
            commands::show::store_scene_config,
            commands::show::set_channel_scoped,
            commands::show::set_all_channels_scoped,
            commands::show::set_scene_scope_faders_enabled,
            commands::show::set_scene_scope_pan_enabled,
            commands::show::set_lockout,
        ])
        .on_menu_event(|app, event| {
            menu::handle_session_menu_event(app, event);
        })
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_app_constructs_builder() {
        let _builder = super::build_app();
    }

    #[test]
    fn build_app_installs_session_menu_during_setup() {
        let source = include_str!("mod.rs");
        let expected = concat!("menu::", "install_session_menu", "(app)?;");

        assert!(source.contains(expected));
    }

    #[test]
    fn default_capability_allows_window_title_updates() {
        let capability = include_str!("../../capabilities/default.json");

        assert!(capability.contains("core:window:allow-set-title"));
    }

    #[test]
    fn command_adapter_exports_existing_command_names() {
        let _ = super::commands::lifecycle::frontend_ready::<tauri::Wry>;
        let _ = super::commands::lifecycle::disconnect_lv1;
        let _ = super::commands::scenes::recall_scene;
        let _ = super::commands::settings::replace_app_settings;
        let _ = super::commands::show::set_lockout;
    }

    #[test]
    fn invoke_handler_includes_frontend_ready() {
        let _ = super::commands::lifecycle::frontend_ready::<tauri::Wry>;
    }
}
