fn main() {
    advanced_show_control::ui::build_app()
        .run(tauri::generate_context!())
        .expect("failed to run Advanced Show Control");
}
