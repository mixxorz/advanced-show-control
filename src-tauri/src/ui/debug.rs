//! Debug-only Tauri app builder for hardware smoke tests.

use super::setup_shared_runtime;
use crate::smoke::SmokeTraceCapture;
use tauri::Manager;

pub(crate) mod commands;

pub fn build_debug_app() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default()
        .setup(|app| {
            let capture = SmokeTraceCapture::new(2048);
            app.manage(capture.clone());
            app.manage(commands::SmokeReport::new());
            setup_shared_runtime(app, Some(capture))
        })
        .invoke_handler(tauri::generate_handler![
            crate::ui::commands::lifecycle::frontend_ready,
            crate::ui::commands::show::refresh_lv1_discovery,
            crate::ui::commands::show::new_show_file,
            crate::ui::commands::show::open_show_file_dialog,
            crate::ui::commands::show::save_show_file,
            crate::ui::commands::show::save_show_file_as_dialog,
            crate::ui::commands::show::set_scene_duration_ms,
            crate::ui::commands::show::select_scene_config,
            crate::ui::commands::show::cue_scene,
            crate::ui::commands::scenes::recall_scene,
            crate::ui::commands::lifecycle::connect_lv1_system,
            crate::ui::commands::lifecycle::attempt_reconnect_lv1,
            crate::ui::commands::lifecycle::startup_auto_connect_lv1,
            crate::ui::commands::lifecycle::disconnect_lv1,
            crate::ui::commands::lifecycle::reconnect_timed_out,
            crate::ui::commands::fade::abort_all_fades,
            crate::ui::commands::show::store_scene_config,
            crate::ui::commands::show::set_channel_scoped,
            crate::ui::commands::show::set_all_channels_scoped,
            crate::ui::commands::show::set_scene_scope_faders_enabled,
            crate::ui::commands::show::set_scene_scope_pan_enabled,
            crate::ui::commands::show::set_lockout,
            commands::debug_smoke_run_connection_test,
            commands::debug_smoke_run_scene_recall_test,
            commands::debug_smoke_run_fade_starts_test,
            commands::debug_smoke_run_fade_completes_test,
            commands::debug_smoke_run_decreasing_xfade_test,
            commands::debug_smoke_run_lockout_blocks_recall_test,
            commands::debug_smoke_finish_suite,
            commands::debug_smoke_exit_app,
            commands::debug_smoke_report_setup,
            commands::debug_smoke_set_channel_gain,
        ])
}
