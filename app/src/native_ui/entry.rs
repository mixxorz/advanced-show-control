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
        runtime.shutdown();
    }
    if let Some(error) = startup_error.borrow_mut().take() {
        anyhow::bail!(error);
    }
    Ok(())
}

/// @cc [owner:mixxorz,label:product;persistence] native-close-uses-session-guard
/// Every platform window-close request MUST be vetoed. While a native prompt, native dialog, or
/// custom modal is active, the request MUST NOT begin a guard preflight. Otherwise it MUST consult
/// AppRoot's dirty-session guard, and the window may close only through the Quit continuation after
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
