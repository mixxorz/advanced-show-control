/// @cc [owner:mixxorz,label:platform] production-entrypoint-boundary
/// The production binary MUST construct the production Tauri builder, run it with the generated
/// application context, and terminate startup with an explicit error if the platform runtime fails.
fn main() {
    advanced_show_control::ui::build_app()
        .run(tauri::generate_context!())
        .expect("failed to run Advanced Show Control");
}
