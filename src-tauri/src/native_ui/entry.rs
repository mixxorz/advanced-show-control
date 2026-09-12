use std::cell::RefCell;
use std::rc::Rc;

use anyhow::Result;
use gpui_kit::component::Root;
use gpui_kit::{AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};

use crate::projector::AppViewState;

use super::cues::CueListsView;
use super::scenes::ScenesView;
use super::settings_view::SettingsView;
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

            cx.on_app_quit(move |_| {
                if let Some(runtime) = shutdown_runtime.borrow_mut().take() {
                    runtime.shutdown();
                }
                async {}
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
                    let scenes = cx
                        .new(|cx| ScenesView::new(initial.clone(), dispatcher.clone(), window, cx));
                    let cue_lists = cx.new(|cx| {
                        CueListsView::new(initial.clone(), dispatcher.clone(), window, cx)
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
                            window,
                            cx,
                        )
                    });

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
