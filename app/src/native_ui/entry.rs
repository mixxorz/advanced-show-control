use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use gpui_kit::component::Root;
use gpui_kit::{
    App, AppContext as _, Bounds, Entity, Window, WindowBounds, WindowOptions, px, size,
};

use crate::projector::AppViewState;

use super::cues::CueListsView;
use super::scenes::ScenesView;
use super::settings_view::SettingsView;
use super::state::GoSubmissionGuard;
use super::{AppRoot, CommandDispatcher, NativeRuntime, menu, theme, ui_event_channel};

pub fn run() -> Result<()> {
    let mut runtime = NativeRuntime::build()?;
    let commands = runtime.commands();
    let runtime_handle = runtime.handle();
    let projections = runtime.take_projections();
    let (ui_events, ui_event_receiver) = ui_event_channel();
    let dispatcher = CommandDispatcher::new(runtime_handle, commands, ui_events);
    let runtime = Rc::new(RefCell::new(Some(runtime)));
    let shutdown_runtime = runtime.clone();
    let startup_error = Rc::new(RefCell::new(None::<String>));
    let startup_error_for_app = startup_error.clone();

    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
            gpui_kit::init(cx);
            if let Err(error) = theme::install(cx) {
                *startup_error_for_app.borrow_mut() = Some(error.to_string());
                cx.quit();
                return;
            }
            menu::install(cx);

            cx.on_app_quit(move |cx| {
                let shutdown = shutdown_runtime.borrow_mut().take().map(|runtime| {
                    // Installation was armed only after explicit confirmation, a session guard,
                    // and successful disconnect. Spawn its helper before AppKit terminates us.
                    if let Some(message) = update_handoff_error(|| runtime.launch_update_on_exit())
                    {
                        show_update_handoff_error(&message);
                    }
                    cx.background_executor()
                        .spawn(async move { runtime.shutdown() })
                });
                async move {
                    if let Some(shutdown) = shutdown {
                        let _ = shutdown.await;
                    }
                }
            })
            .detach();

            let bounds = Bounds::centered(None, size(px(1180.), px(780.)), cx);
            let result = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(960.), px(640.))),
                    app_id: Some(super::APP_IDENTIFIER.to_string()),
                    ..Default::default()
                },
                move |window, cx| {
                    let initial = AppViewState::default();
                    let go_submissions = Rc::new(RefCell::new(GoSubmissionGuard::default()));
                    let scenes = cx
                        .new(|cx| ScenesView::new(initial.clone(), dispatcher.clone(), window, cx));
                    let cue_lists = cx.new(|cx| {
                        CueListsView::new(
                            initial.clone(),
                            dispatcher.clone(),
                            go_submissions.clone(),
                            window,
                            cx,
                        )
                    });
                    let settings =
                        cx.new(|cx| SettingsView::new(initial, dispatcher.clone(), window, cx));
                    let app = cx.new(|cx| {
                        AppRoot::new(
                            dispatcher,
                            projections,
                            ui_event_receiver,
                            scenes,
                            cue_lists,
                            settings,
                            go_submissions,
                            window,
                            cx,
                        )
                    });
                    install_session_close_guard(&app, window, cx);

                    cx.new(|cx| Root::new(app, window, cx))
                },
            );
            if let Err(error) = result {
                *startup_error_for_app.borrow_mut() =
                    Some(format!("failed to open the application window: {error}"));
                cx.quit();
                return;
            }
            cx.activate(true);
        });

    if let Some(runtime) = runtime.borrow_mut().take() {
        if let Some(message) = update_handoff_error(|| runtime.launch_update_on_exit()) {
            show_update_handoff_error(&message);
        }
        runtime.shutdown();
    }
    if let Some(error) = startup_error.borrow_mut().take() {
        anyhow::bail!(error);
    }
    Ok(())
}

fn update_handoff_error(launch: impl FnOnce() -> Result<(), String>) -> Option<String> {
    launch().err().map(|error| format!("The update installer did not start. Advanced Show Control will close without updating or restarting. Reopen the app to retry.\n\n{error}"))
}

#[cfg(target_os = "macos")]
fn show_update_handoff_error(message: &str) {
    let script = r#"on run argv
        display alert "Advanced Show Control update failed" message (item 1 of argv) as critical buttons {"OK"} default button "OK"
    end run"#;
    let shown = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", script, "--", message])
        .status()
        .is_ok_and(|status| status.success());
    if !shown {
        eprintln!("{message}");
    }
}

#[cfg(target_os = "windows")]
fn show_update_handoff_error(message: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let title: Vec<u16> = "Advanced Show Control update failed\0"
        .encode_utf16()
        .collect();
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

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn show_update_handoff_error(message: &str) {
    eprintln!("{message}");
}

/// @cc [owner:mixxorz,label:product;persistence] native-close-uses-session-guard
/// Every platform window-close request MUST be vetoed. While a native prompt, native dialog, or
/// custom modal is active, or another guarded action is pending, the request MUST NOT begin another
/// guard preflight. Otherwise it MUST consult AppRoot's dirty-session guard, and the window may
/// close only through the Quit continuation after
/// an authoritative clean preflight, explicit Discard, or a successful save followed by an
/// authoritative clean recheck.
fn install_session_close_guard(app: &Entity<AppRoot>, window: &mut Window, cx: &mut App) {
    let weak_app = app.downgrade();
    window.on_window_should_close(cx, move |window, cx| {
        weak_app
            .update(cx, |app, cx| app.handle_close_request(window, cx))
            .unwrap_or(true)
    });
}

#[cfg(test)]
mod tests {
    use super::update_handoff_error;

    #[test]
    fn failed_helper_launch_explains_that_update_and_restart_did_not_start() {
        assert_eq!(update_handoff_error(|| Ok(())), None);
        assert_eq!(update_handoff_error(|| Err("Update helper is missing.".to_string())), Some("The update installer did not start. Advanced Show Control will close without updating or restarting. Reopen the app to retry.\n\nUpdate helper is missing.".to_string()));
    }
}
