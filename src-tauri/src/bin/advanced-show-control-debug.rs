fn main() {
    advanced_show_control::ui::debug::build_debug_app()
        .run(tauri::generate_context!("tauri.debug.conf.json"))
        .expect("failed to run Advanced Show Control Debug");
}
