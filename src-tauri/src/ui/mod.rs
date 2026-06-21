//! Tauri adapter layer.
//!
//! This module will contain command registration and frontend serialization
//! boundaries. Business logic should route through actor mailboxes.

use crate::lifecycle::AppLifecycle;
use crate::logging;
use crate::runtime::events::AppEventBus;
use crate::show::build_show_actor;
use tauri::Manager;
use tokio::sync::broadcast;

pub mod commands;
pub mod debug;

pub type UiLogReceiverState = broadcast::Sender<logging::UiLogEvent>;

pub fn build_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| setup_shared_runtime(app, None))
        .invoke_handler(tauri::generate_handler![
            commands::lifecycle::frontend_ready,
            commands::show::refresh_lv1_discovery,
            commands::show::new_show_file,
            commands::show::open_show_file_dialog,
            commands::show::save_show_file,
            commands::show::save_show_file_as_dialog,
            commands::show::set_scene_duration_ms,
            commands::show::select_scene_config,
            commands::show::cue_scene,
            commands::scenes::recall_scene,
            commands::lifecycle::connect_lv1_system,
            commands::lifecycle::attempt_reconnect_lv1,
            commands::lifecycle::startup_auto_connect_lv1,
            commands::lifecycle::disconnect_lv1,
            commands::lifecycle::reconnect_timed_out,
            commands::fade::abort_all_fades,
            commands::show::store_scene_config,
            commands::show::set_channel_scoped,
            commands::show::set_all_channels_scoped,
            commands::show::set_scene_scope_faders_enabled,
            commands::show::set_scene_scope_pan_enabled,
            commands::show::set_lockout,
        ])
}

pub(super) fn setup_shared_runtime<R: tauri::Runtime>(
    app: &mut tauri::App<R>,
    smoke_trace_capture: Option<crate::smoke::SmokeTraceCapture>,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_bus = AppEventBus::default();
    let (show, show_task, show_peers) = build_show_actor(event_bus.clone());
    let lifecycle = AppLifecycle::new(event_bus, show.clone(), show_peers);
    show_task.spawn();
    let logging_runtime = logging::init_logging(app.handle(), smoke_trace_capture)?;
    app.manage(show);
    app.manage(lifecycle);
    app.manage(logging_runtime.guard);
    app.manage(logging_runtime.ui_logs);
    tracing::info!(event = "app_started", "Starting Advanced Show Control");
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_app_constructs_builder() {
        let _builder = super::build_app();
    }

    #[test]
    fn smoke_trace_capture_type_is_exported_for_debug_state() {
        let capture = crate::smoke::SmokeTraceCapture::new(8);
        let _layer = crate::smoke::SmokeTraceLayer::new(capture);
    }

    #[test]
    fn debug_build_app_constructs_builder() {
        let _builder = super::debug::build_debug_app();
    }

    #[test]
    fn command_adapter_exports_existing_command_names() {
        let _ = super::commands::lifecycle::frontend_ready::<tauri::Wry>;
        let _ = super::commands::lifecycle::disconnect_lv1;
        let _ = super::commands::scenes::recall_scene;
        let _ = super::commands::show::set_lockout;
    }

    #[test]
    fn production_builder_keeps_existing_command_exports() {
        let _ = super::commands::lifecycle::frontend_ready::<tauri::Wry>;
        let _ = super::commands::show::set_lockout;
        let _ = super::commands::scenes::recall_scene;
        let _ = super::commands::fade::abort_all_fades;
    }

    #[test]
    fn invoke_handler_includes_frontend_ready() {
        let _ = super::commands::lifecycle::frontend_ready::<tauri::Wry>;
    }

    #[test]
    fn debug_command_adapter_exports_smoke_commands() {
        let _ = super::debug::commands::debug_smoke_run_connection_test::<tauri::Wry>;
        let _ = super::debug::commands::debug_smoke_run_scene_recall_test;
        let _ = super::debug::commands::debug_smoke_run_fade_starts_test;
        let _ = super::debug::commands::debug_smoke_run_fade_completes_test;
        let _ = super::debug::commands::debug_smoke_run_decreasing_xfade_test;
        let _ = super::debug::commands::debug_smoke_run_lockout_blocks_recall_test;
    }
}
