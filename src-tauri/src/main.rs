mod app_state;
mod commands;

use app_state::ShellState;

fn main() {
    tauri::Builder::default()
        .manage(ShellState::default())
        .invoke_handler(tauri::generate_handler![
            commands::get_app_status,
            commands::connect_lv1,
            commands::disconnect_lv1,
            commands::abort_all_fades,
            commands::finish_fade_now,
            commands::set_lockout,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run LV1 Scene Fade Utility");
}
