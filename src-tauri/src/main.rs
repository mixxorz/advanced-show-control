mod app_state;

use app_state::ShellState;

fn main() {
    tauri::Builder::default()
        .manage(ShellState::default())
        .run(tauri::generate_context!())
        .expect("failed to run LV1 Scene Fade Utility");
}
