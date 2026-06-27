//! Debug-only Tauri app builder for hardware smoke tests.

use crate::lifecycle::AppLifecycle;
use crate::logging;
use crate::runtime::events::AppEventBus;
use crate::settings::build_settings_actor;
use crate::show::build_show_actor;
use tauri::Manager;

pub(crate) mod commands;

pub fn build_debug_app() -> tauri::Builder<tauri::Wry> {
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
            app.manage(commands::SmokeReport::new());
            tracing::info!(
                event = "app_started",
                "Starting Advanced Show Control debug smoke"
            );
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::ui::commands::lifecycle::frontend_ready,
            crate::ui::commands::show::refresh_lv1_discovery,
            crate::ui::commands::show::new_show_file,
            crate::ui::commands::show::open_show_file_dialog,
            crate::ui::commands::show::save_show_file,
            crate::ui::commands::show::save_show_file_as_dialog,
            crate::ui::commands::scenes::link_scene_config,
            crate::ui::commands::scenes::recall_scene,
            crate::ui::commands::scenes::set_channel_scoped,
            crate::ui::commands::scenes::set_scene_duration_ms,
            crate::ui::commands::scenes::store_scene_config,
            crate::ui::commands::lifecycle::connect_lv1_system,
            crate::ui::commands::lifecycle::attempt_reconnect_lv1,
            crate::ui::commands::lifecycle::startup_auto_connect_lv1,
            crate::ui::commands::lifecycle::disconnect_lv1,
            crate::ui::commands::lifecycle::reconnect_timed_out,
            crate::ui::commands::fade::abort_all_fades,
            crate::ui::commands::show::set_lockout,
            commands::debug_smoke_log,
            commands::debug_smoke_exit_app,
            commands::debug_smoke_set_channel_gain,
            commands::debug_smoke_recall_lv1_scene,
            commands::debug_smoke_get_channel_gain,
            commands::debug_smoke_load_unlinked_scene_session,
        ])
}
