#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

/// @cc [owner:mixxorz,label:platform] production-entrypoint-boundary
/// The production binary MUST construct the production Tauri builder, run it with the generated
/// application context, and terminate startup with an explicit error if the platform runtime fails.
/// Release builds on Windows MUST use the GUI subsystem so launching the app does not create a
/// console window; debug builds MUST retain the console subsystem for development diagnostics.
fn main() {
    advanced_show_control::ui::build_app()
        .run(tauri::generate_context!())
        .expect("failed to run Advanced Show Control");
}
