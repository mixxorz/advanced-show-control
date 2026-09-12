#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

/// @cc [owner:mixxorz,label:platform] production-entrypoint-boundary
/// The production binary MUST construct the production GPUI host and terminate startup with an
/// explicit error if either the automation runtime or native window cannot be initialized. Release
/// builds on Windows MUST use the GUI subsystem so launching the app does not create a console
/// window; debug builds MUST retain the console subsystem for development diagnostics.
fn main() {
    if let Err(error) = advanced_show_control::native_ui::run() {
        let message = format!("Advanced Show Control could not start:\n\n{error:#}");
        report_startup_error(&message);
        std::process::exit(1);
    }
}

#[cfg(target_os = "windows")]
fn report_startup_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    let title: Vec<u16> = "Advanced Show Control\0".encode_utf16().collect();
    let message: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(target_os = "macos")]
fn report_startup_error(message: &str) {
    let script = r#"
        on run argv
            display alert "Advanced Show Control could not start" message (item 1 of argv) as critical buttons {"Quit"} default button "Quit"
        end run
    "#;
    let shown = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", script, "--", message])
        .status()
        .is_ok_and(|status| status.success());
    if !shown {
        eprintln!("{message}");
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn report_startup_error(message: &str) {
    eprintln!("{message}");
}
