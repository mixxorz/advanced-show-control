#[cfg(debug_assertions)]
fn main() {
    advanced_show_control::ui::debug::build_debug_app()
        .run(tauri::generate_context!("tauri.debug.conf.json"))
        .expect("failed to run Advanced Show Control Debug");
}

#[cfg(not(debug_assertions))]
fn main() {
    panic!("advanced-show-control-debug is only available in debug builds");
}
