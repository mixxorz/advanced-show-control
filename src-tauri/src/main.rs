mod app_state;
mod commands;
mod show_file;

use app_state::ShellState;

fn main() {
    tauri::Builder::default()
        .manage(ShellState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::new_show_file,
            commands::open_show_file_dialog,
            commands::save_show_file,
            commands::save_show_file_as_dialog,
            commands::set_scene_duration_ms,
            commands::select_scene_config,
            commands::connect_lv1,
            commands::disconnect_lv1,
            commands::abort_all_fades,
            commands::finish_fade_now,
            commands::store_scene_config,
            commands::set_channel_scoped,
            commands::set_all_channels_scoped,
            commands::set_lockout,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LV1 Scene Fade Utility");
}
