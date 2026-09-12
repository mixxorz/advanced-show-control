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

/// @cc [owner:mixxorz,label:architecture] production-setup-owns-shared-runtime
/// Production setup MUST create one shared event bus, build and spawn the app-lifetime Show and
/// Settings owners from it, construct Lifecycle from their handles and initial settings, and manage
/// those shared handles plus logging state before commands run. Projector startup MUST remain
/// deferred to `frontend_ready`; setup MUST NOT create a competing projection owner.
pub fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let event_bus = AppEventBus::default();
            let settings_dir = app.path().app_config_dir()?;
            let logging_runtime = logging::init_logging(&settings_dir)?;
            logging_runtime.spawn_settings_watcher(event_bus.subscribe());
            let (show, show_task, show_peers, lockout) = build_show_actor(event_bus.clone());
            let (settings, settings_task, initial_settings) =
                build_settings_actor(settings_dir, event_bus.clone());
            logging_runtime.apply_settings(&initial_settings);
            let lifecycle = AppLifecycle::new(
                event_bus,
                show.clone(),
                show_peers,
                lockout,
                settings.clone(),
                initial_settings,
            );
            show_task.spawn();
            settings_task.spawn();
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
            commands::cue_lists::add_scene_to_active_cue_list,
            commands::cue_lists::create_cue_list,
            commands::cue_lists::cue_entry,
            commands::cue_lists::delete_cue_list,
            commands::cue_lists::recall_cued_cue,
            commands::cue_lists::remove_cue_entry,
            commands::cue_lists::rename_cue_list,
            commands::cue_lists::reorder_cue_entries,
            commands::cue_lists::reorder_cue_lists,
            commands::cue_lists::set_active_cue_list,
            commands::scenes::copy_scene_settings,
            commands::scenes::delete_scene_config,
            commands::scenes::link_scene_config,
            commands::scenes::paste_scene_settings,
            commands::scenes::recall_scene,
            commands::scenes::select_scene_config,
            commands::scenes::set_all_channels_scoped,
            commands::scenes::set_channel_scoped,
            commands::scenes::set_scene_duration_ms,
            commands::scenes::set_scene_scope_faders_enabled,
            commands::scenes::set_scene_scope_pan_enabled,
            commands::scenes::store_scene_config,
            commands::lifecycle::connect_lv1_system,
            commands::lifecycle::probe_lv1_tcp_connect_latency,
            commands::lifecycle::startup_auto_connect_lv1,
            commands::lifecycle::disconnect_lv1,
            commands::fade::abort_all_fades,
            commands::settings::replace_app_settings,
            commands::show::set_lockout,
        ])
        .on_menu_event(|app, event| {
            menu::handle_session_menu_event(app, event);
        })
}
