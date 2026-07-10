fn main() {
    advanced_show_control::ui::debug::build_debug_app()
        .run(tauri::generate_context!())
        .expect("failed to run Advanced Show Control Debug");
}
