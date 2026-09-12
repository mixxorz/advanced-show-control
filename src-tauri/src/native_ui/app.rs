use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    ParentElement as _, PathPromptOptions, Render, Styled as _, Window, div,
};
use tokio::sync::mpsc;

use crate::projector::{AppViewState, ProjectionSubscription};

use super::connection::{ConnectionDialogMode, ConnectionState, open_connection_dialog};
use super::cues::CueListsView;
use super::keyboard::{InteractionState, RoutedAction, global_key_context, route_action};
use super::logs::LogsView;
use super::menu::{NewShow, OpenShow, SaveShow, SaveShowAs};
use super::scenes::ScenesView;
use super::settings_view::SettingsView;
use super::shell::AppShell;
use super::{CommandDispatcher, MainTab, PresentationState, UiEvent};

pub struct AppRoot {
    presentation: PresentationState,
    dispatcher: CommandDispatcher,
    shell: Entity<AppShell>,
    cue_lists: Entity<CueListsView>,
    connection: Rc<RefCell<ConnectionState>>,
    latest_snapshot: Rc<RefCell<AppViewState>>,
    go_command_id: Rc<Cell<Option<u64>>>,
}

impl AppRoot {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dispatcher: CommandDispatcher,
        projections: ProjectionSubscription,
        ui_events: mpsc::UnboundedReceiver<UiEvent>,
        scenes: Entity<ScenesView>,
        cue_lists: Entity<CueListsView>,
        settings: Entity<SettingsView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        dispatcher.bridge_projections(projections);
        let initial = AppViewState::default();
        let connection = Rc::new(RefCell::new(ConnectionState::startup()));
        let latest_snapshot = Rc::new(RefCell::new(initial.clone()));
        let go_command_id = Rc::new(Cell::new(None));
        let open_connection_state = connection.clone();
        let open_snapshot = latest_snapshot.clone();
        let open_dispatcher = dispatcher.clone();
        let shell = cx.new(|cx| {
            AppShell::new(
                initial.clone(),
                dispatcher.clone(),
                scenes,
                cue_lists.clone(),
                settings,
                cx.new(|_| LogsView::new(initial)),
                go_command_id.clone(),
                move |window, cx| {
                    open_connection_state.borrow_mut().open_manual();
                    if !window.has_active_dialog(cx) {
                        open_connection_dialog(
                            window,
                            cx,
                            open_snapshot.clone(),
                            open_connection_state.clone(),
                            open_dispatcher.clone(),
                        );
                    }
                },
                cx,
            )
        });

        Self::spawn_event_pump(ui_events, cx);
        Self::spawn_discovery_refresh(dispatcher.clone(), connection.clone(), cx);

        dispatcher.dispatch(|commands| async move {
            commands.startup_auto_connect_lv1().await.map(|_| ())
        });

        window.set_window_title(&super::format_session_window_title(
            "Untitled Session",
            false,
        ));

        Self {
            presentation: PresentationState::default(),
            dispatcher,
            shell,
            cue_lists,
            connection,
            latest_snapshot,
            go_command_id,
        }
    }

    fn spawn_event_pump(mut ui_events: mpsc::UnboundedReceiver<UiEvent>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            while let Some(event) = ui_events.recv().await {
                if this
                    .update_in(cx, |this, window, cx| this.handle_event(event, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    fn spawn_discovery_refresh(
        dispatcher: CommandDispatcher,
        connection: Rc<RefCell<ConnectionState>>,
        cx: &mut Context<Self>,
    ) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |_, _| {
            loop {
                if connection.borrow().is_visible() {
                    dispatcher.dispatch(|commands| async move {
                        commands.refresh_lv1_discovery(None).await.map(|_| ())
                    });
                }
                executor.timer(Duration::from_secs(5)).await;
            }
        })
        .detach();
    }

    fn handle_event(&mut self, event: UiEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            UiEvent::Snapshot(snapshot) => {
                let snapshot = *snapshot;
                if !self.presentation.accept_snapshot(snapshot.clone()) {
                    return;
                }
                let connection_was_visible = self.connection.borrow().is_visible();
                self.connection
                    .borrow_mut()
                    .close_startup_if_connected(&snapshot);
                if connection_was_visible && !self.connection.borrow().is_visible() {
                    window.close_dialog(cx);
                }
                *self.latest_snapshot.borrow_mut() = snapshot.clone();
                window.set_window_title(&self.presentation.window_title());
                self.shell
                    .update(cx, |shell, cx| shell.set_snapshot(snapshot, window, cx));
                self.sync_connection_dialog(window, cx);
            }
            UiEvent::CommandStarted { command_id } => {
                self.presentation.command_started(command_id);
                self.connection.borrow_mut().command_error = None;
                window.refresh();
                cx.notify();
            }
            UiEvent::CommandFinished { command_id, result } => {
                self.connection.borrow_mut().pending_identity = None;
                if self.go_command_id.get() == Some(command_id) {
                    self.go_command_id.set(None);
                    self.shell.update(cx, |_, cx| cx.notify());
                }
                let failed = result.is_err();
                self.presentation.complete_command(command_id, result);
                self.shell.update(cx, |shell, cx| {
                    shell.command_finished(command_id, failed, cx)
                });
                self.connection.borrow_mut().command_error =
                    self.presentation.command_error().map(str::to_string);
                if let Some(error) = self.presentation.command_error() {
                    window.push_notification(Notification::error(error.to_string()), cx);
                }
                self.sync_connection_dialog(window, cx);
                cx.notify();
            }
            UiEvent::LatencyMeasured { identity, result } => {
                self.connection.borrow_mut().set_latency(&identity, result);
                self.sync_connection_dialog(window, cx);
            }
        }
    }

    fn sync_connection_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let visible = self.connection.borrow().is_visible();
        if !visible {
            return;
        }

        if window.has_active_dialog(cx) {
            window.refresh();
            return;
        }
        open_connection_dialog(
            window,
            cx,
            self.latest_snapshot.clone(),
            self.connection.clone(),
            self.dispatcher.clone(),
        );
    }

    pub fn open_connection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.connection.borrow_mut().mode = Some(ConnectionDialogMode::Manual);
        self.sync_connection_dialog(window, cx);
    }

    pub fn new_show(&self) {
        self.dispatcher
            .dispatch(|commands| async move { commands.new_show_file().await.map(|_| ()) });
    }

    pub fn open_show(&self, cx: &mut Context<Self>) {
        let response = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Session".into()),
        });
        let dispatcher = self.dispatcher.clone();
        cx.spawn(async move |this, cx| match response.await {
            Ok(Ok(Some(paths))) => {
                let path = paths.into_iter().next();
                dispatcher.dispatch(move |commands| async move {
                    commands.open_show_file(path).await.map(|_| ())
                });
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => Self::push_async_error(
                &this,
                format!("Could not open the session picker: {error}"),
                cx,
            ),
            Err(error) => Self::push_async_error(
                &this,
                format!("The session picker did not return a result: {error}"),
                cx,
            ),
        })
        .detach();
    }

    pub fn save_show(&self, cx: &mut Context<Self>) {
        if self.presentation.snapshot().show_file_path.is_some() {
            self.dispatcher.dispatch(|commands| async move {
                commands.save_show_file(None).await.map(|_| ())
            });
        } else {
            self.prompt_to_save(false, cx);
        }
    }

    pub fn save_show_as(&self, cx: &mut Context<Self>) {
        self.prompt_to_save(true, cx);
    }

    fn prompt_to_save(&self, save_as: bool, cx: &mut Context<Self>) {
        let folder = crate::show_file::default_show_folder();
        let file_name = if self
            .presentation
            .snapshot()
            .show_file_name
            .ends_with(".ascs")
        {
            self.presentation.snapshot().show_file_name.clone()
        } else {
            format!("{}.ascs", self.presentation.snapshot().show_file_name)
        };
        let response = cx.prompt_for_new_path(&folder, Some(&file_name));
        let dispatcher = self.dispatcher.clone();
        cx.spawn(async move |this, cx| match response.await {
            Ok(Ok(Some(path))) => {
                dispatcher.dispatch(move |commands| async move {
                    if save_as {
                        commands.save_show_file_as(Some(path)).await.map(|_| ())
                    } else {
                        commands.save_show_file(Some(path)).await.map(|_| ())
                    }
                });
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => Self::push_async_error(
                &this,
                format!("Could not open the save picker: {error}"),
                cx,
            ),
            Err(error) => Self::push_async_error(
                &this,
                format!("The save picker did not return a result: {error}"),
                cx,
            ),
        })
        .detach();
    }

    fn push_async_error(
        this: &gpui_kit::WeakEntity<Self>,
        message: String,
        cx: &mut gpui_kit::AsyncApp,
    ) {
        let _ = this.update_in(cx, |_, window, cx| {
            window.push_notification(Notification::error(message), cx)
        });
    }

    fn on_new_show(&mut self, _: &NewShow, _: &mut Window, _: &mut Context<Self>) {
        self.new_show();
    }

    fn on_open_show(&mut self, _: &OpenShow, _: &mut Window, cx: &mut Context<Self>) {
        self.open_show(cx);
    }

    fn on_save_show(&mut self, _: &SaveShow, _: &mut Window, cx: &mut Context<Self>) {
        self.save_show(cx);
    }

    fn on_save_show_as(&mut self, _: &SaveShowAs, _: &mut Window, cx: &mut Context<Self>) {
        self.save_show_as(cx);
    }

    /// @cc [owner:mixxorz,label:safety;keyboard] go-shortcut-routing
    /// A matching GO keydown MUST be consumed, but MUST dispatch at most one recall while a prior
    /// GO recall is unsettled and only when the projected active cue resolves to a projected scene.
    /// Repeats and key events owned by editable controls or dialogs MUST NOT dispatch.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let interaction = InteractionState {
            modal_open: window.has_active_dialog(cx),
            editable_focused: event.prefer_character_input,
        };
        let snapshot = self.presentation.snapshot();
        let Some(action) = route_action(
            event,
            false,
            interaction,
            &snapshot.settings.keyboard_shortcuts.go,
            &snapshot.settings.keyboard_shortcuts.cue,
        ) else {
            return;
        };

        match action {
            RoutedAction::Go => {
                cx.stop_propagation();
                if event.is_held
                    || self.go_command_id.get().is_some()
                    || !self.presentation.cued_scene_is_valid()
                {
                    return;
                }
                self.go_command_id
                    .set(Some(self.dispatcher.dispatch(|commands| async move {
                        commands.recall_cued_cue().await.map(|_| ())
                    })));
                self.shell.update(cx, |_, cx| cx.notify());
            }
            RoutedAction::Cue => {
                if event.is_held || self.shell.read(cx).active_tab() != MainTab::CueLists {
                    return;
                }
                let handled = self
                    .cue_lists
                    .update(cx, |cue_lists, cx| cue_lists.cue_selected(cx));
                if handled {
                    cx.stop_propagation();
                }
            }
        }
    }
}

impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("advanced-show-control")
            .relative()
            .size_full()
            .key_context(global_key_context())
            .on_action(cx.listener(Self::on_new_show))
            .on_action(cx.listener(Self::on_open_show))
            .on_action(cx.listener(Self::on_save_show))
            .on_action(cx.listener(Self::on_save_show_as))
            .on_key_down(cx.listener(Self::on_key_down))
            .child(self.shell.clone())
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}
