use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::notification::Notification;
use gpui_kit::component::{Root, WindowExt as _};
use gpui_kit::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, PathPromptOptions, PromptLevel, Render, Styled as _, Window,
    div,
};
use tokio::sync::mpsc;

use crate::projector::{AppViewState, ProjectionSubscription};

use super::connection::{ConnectionState, open_connection_dialog};
use super::cues::CueListsView;
use super::keyboard::{
    InteractionState, RoutedAction, global_key_context, normalized_physical_key, route_action,
};
use super::logs::LogsView;
use super::menu::{About, NewShow, NewShowFromTemplate, OpenShow, Quit, SaveShow, SaveShowAs};
#[cfg(target_os = "macos")]
use super::menu::{Hide, HideOthers};
use super::scenes::ScenesView;
use super::session_guard::{GuardChoice, GuardEffect, SessionAction, SessionGuard, SessionStatus};
use super::settings_view::SettingsView;
use super::shell::AppShell;
use super::state::GoSubmissionGuard;
use super::{CommandDispatcher, MainTab, PresentationState, UiEvent};

pub struct AppRoot {
    presentation: PresentationState,
    dispatcher: CommandDispatcher,
    focus: FocusHandle,
    shell: Entity<AppShell>,
    cue_lists: Entity<CueListsView>,
    connection: Rc<RefCell<ConnectionState>>,
    latest_snapshot: Rc<RefCell<AppViewState>>,
    go_submissions: Rc<RefCell<GoSubmissionGuard>>,
    pending_save_command_id: Cell<Option<u64>>,
    session_guard: RefCell<SessionGuard>,
}

impl AppRoot {
    /// @cc [owner:mixxorz,label:accessibility;keyboard] session-action-focus
    /// AppRoot MUST establish a live tracked action context before a startup dialog can open. After
    /// that dialog closes, fixed session and Quit shortcuts and in-app session-menu actions MUST
    /// reach AppRoot without requiring another pointer or focus event.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        dispatcher: CommandDispatcher,
        projections: ProjectionSubscription,
        ui_events: mpsc::UnboundedReceiver<UiEvent>,
        scenes: Entity<ScenesView>,
        cue_lists: Entity<CueListsView>,
        settings: Entity<SettingsView>,
        go_submissions: Rc<RefCell<GoSubmissionGuard>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        dispatcher.bridge_projections(projections);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let initial = AppViewState::default();
        let connection = Rc::new(RefCell::new(ConnectionState::startup()));
        let latest_snapshot = Rc::new(RefCell::new(initial.clone()));
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
                go_submissions.clone(),
                focus.clone(),
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
            focus,
            shell,
            cue_lists,
            connection,
            latest_snapshot,
            go_submissions,
            pending_save_command_id: Cell::new(None),
            session_guard: RefCell::new(SessionGuard::default()),
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

    /// @cc [owner:mixxorz,label:persistence;ordering] query-epoch-check-and-continuation-atomic
    /// A session-state query result MUST compare both its captured persisted-edit submission epoch
    /// and authoritative owner-side persisted revision with their current values, then either
    /// enqueue a retry or apply the guard result in this same GPUI callback, without yielding to a
    /// UI submission or owner fact between comparison and continuation dispatch. A clean result
    /// MUST carry its validated owner revision into guarded replacement or quit admission; this UI
    /// comparison alone MUST NOT authorize the destructive action. If any modal surface is active,
    /// it MUST cancel instead.
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
                self.go_submissions.borrow_mut().observe_snapshot(&snapshot);
                *self.latest_snapshot.borrow_mut() = snapshot.clone();
                window.set_window_title(&self.presentation.window_title());
                self.shell
                    .update(cx, |shell, cx| shell.set_snapshot(snapshot, window, cx));
                self.sync_connection_dialog(window, cx);
            }
            UiEvent::CommandStarted { command_id } => {
                self.presentation.command_started(command_id);
                let mut connection = self.connection.borrow_mut();
                if connection.pending_command_id == Some(command_id) {
                    connection.command_error = None;
                }
                drop(connection);
                window.refresh();
                cx.notify();
            }
            UiEvent::CommandFinished { command_id, result } => {
                let guard_effect = self
                    .session_guard
                    .borrow_mut()
                    .command_finished(command_id, result.is_ok());
                self.finish_command_event(command_id, result, window, cx);
                self.apply_guard_effect(guard_effect, window, cx);
            }
            UiEvent::CueRecallFinished {
                command_id,
                session_revision,
                canceled_by_session_replacement,
                result,
            } => {
                let snapshot = self.latest_snapshot.borrow();
                if !cue_completion_matches_session(session_revision, &snapshot) {
                    return;
                }
                if canceled_by_session_replacement {
                    drop(snapshot);
                    self.go_submissions.borrow_mut().invalidate();
                    self.cue_lists.update(cx, |_, cx| cx.notify());
                    self.shell.update(cx, |_, cx| cx.notify());
                    self.finish_command_event(command_id, Ok(()), window, cx);
                    return;
                }
                let recalled_entry_id = result.as_ref().ok().map(|result| result.recalled_entry_id);
                if !self.go_submissions.borrow_mut().finish(
                    command_id,
                    recalled_entry_id,
                    &snapshot,
                ) {
                    return;
                }
                drop(snapshot);
                self.cue_lists.update(cx, |_, cx| cx.notify());
                self.shell.update(cx, |_, cx| cx.notify());
                self.finish_command_event(command_id, result.map(|_| ()), window, cx);
            }
            UiEvent::SaveDestinationRequired { command_id } => {
                if self.pending_save_command_id.get() == Some(command_id) {
                    self.pending_save_command_id.set(None);
                    if !window.has_active_prompt()
                        && !self.shell.read(cx).shortcut_capture_active(cx)
                        && !self.modal_open(window, cx)
                    {
                        self.prompt_to_save(false, cx);
                    }
                }
            }
            UiEvent::SessionStateQueryFinished {
                query_id,
                persisted_edit_epoch,
                result,
            } => {
                if modal_surface_active(
                    window.has_active_prompt(),
                    window.has_active_dialog(cx),
                    self.shell.read(cx).modal_open(cx),
                ) {
                    if self.session_guard.borrow_mut().cancel_pending() {
                        window.push_notification(
                            Notification::error(
                                "The session action was cancelled because another dialog opened.",
                            ),
                            cx,
                        );
                    }
                    return;
                }
                let effect = if session_query_is_stale(
                    persisted_edit_epoch,
                    self.dispatcher.persisted_edit_epoch(),
                    &result,
                    self.dispatcher.persisted_session_revision(),
                ) {
                    self.session_guard.borrow_mut().state_query_stale(query_id)
                } else {
                    let result = result.map(|state| SessionStatus {
                        path: state.show_file_path,
                        dirty: state.show_file_dirty,
                        persisted_session_revision: state.persisted_session_revision,
                    });
                    self.session_guard
                        .borrow_mut()
                        .state_query_finished(query_id, result)
                };
                self.apply_guard_effect(effect, window, cx);
            }
            UiEvent::LatencyMeasured {
                session_id,
                identity,
                result,
            } => {
                self.connection
                    .borrow_mut()
                    .set_latency(session_id, &identity, result);
                self.sync_connection_dialog(window, cx);
            }
        }
    }

    fn finish_command_event(
        &mut self,
        command_id: u64,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_save_command_id.get() == Some(command_id) {
            self.pending_save_command_id.set(None);
        }
        let was_connection_command =
            self.connection.borrow().pending_command_id == Some(command_id);
        let completed_error = result.as_ref().err().cloned();
        self.connection.borrow_mut().finish_command(command_id);
        let failed = result.is_err();
        let is_latest_command = self.presentation.complete_command(command_id, result);
        self.shell.update(cx, |shell, cx| {
            shell.command_finished(command_id, failed, window, cx)
        });
        if was_connection_command {
            self.connection.borrow_mut().command_error = completed_error.clone();
        }
        if is_latest_command && let Some(error) = completed_error {
            window.push_notification(Notification::error(error), cx);
        }
        self.sync_connection_dialog(window, cx);
        cx.notify();
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
        self.connection.borrow_mut().open_manual();
        self.sync_connection_dialog(window, cx);
    }

    pub(super) fn handle_close_request(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if modal_surface_active(
            window.has_active_prompt(),
            window.has_active_dialog(cx),
            self.shell.read(cx).modal_open(cx),
        ) {
            return false;
        }
        let (accept, effect) = self.session_guard.borrow_mut().request_close();
        self.apply_guard_effect(effect, window, cx);
        accept
    }

    fn request_session_action(
        &mut self,
        action: SessionAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effect = self.session_guard.borrow_mut().request(action);
        self.apply_guard_effect(effect, window, cx);
    }

    fn apply_guard_effect(
        &mut self,
        effect: GuardEffect,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match effect {
            GuardEffect::None => {}
            GuardEffect::QueryState => {
                let query_id = self.dispatcher.query_show_session_state();
                self.session_guard.borrow_mut().query_started(query_id);
            }
            GuardEffect::Prompt => self.prompt_for_dirty_session(window, cx),
            GuardEffect::ChooseSaveDestination => self.prompt_for_guarded_save_destination(cx),
            GuardEffect::SaveCurrent => {
                let command_id = self.dispatcher.dispatch_serial(|commands| async move {
                    if commands.save_show_file(None).await?.is_none() {
                        return Err("The session no longer has a save destination.".to_string());
                    }
                    Ok(())
                });
                self.session_guard.borrow_mut().save_started(command_id);
            }
            GuardEffect::SaveTo(path) => {
                let command_id = self.dispatcher.dispatch_serial(move |commands| async move {
                    commands.save_show_file(Some(path)).await.map(|_| ())
                });
                self.session_guard.borrow_mut().save_started(command_id);
            }
            GuardEffect::Continue {
                action,
                expected_persisted_revision,
            } => match (action, expected_persisted_revision) {
                (SessionAction::New, Some(revision)) => self.guarded_new_show(revision),
                (SessionAction::NewFromTemplate, Some(revision)) => {
                    self.guarded_new_show_from_template(revision, cx)
                }
                (SessionAction::Open, Some(revision)) => self.guarded_open_show(revision, cx),
                (SessionAction::Quit, Some(revision)) => {
                    if !self.dispatcher.admit_guarded_quit(revision, || cx.quit()) {
                        let effect = self.session_guard.borrow_mut().request(SessionAction::Quit);
                        self.apply_guard_effect(effect, window, cx);
                    }
                }
                (SessionAction::New, None) => self.new_show(),
                (SessionAction::NewFromTemplate, None) => self.new_show_from_template(cx),
                (SessionAction::Open, None) => self.open_show(cx),
                (SessionAction::Quit, None) => cx.quit(),
            },
            GuardEffect::Error(message) => {
                window.push_notification(Notification::error(message), cx);
            }
        }
    }

    fn prompt_for_dirty_session(&self, window: &mut Window, cx: &mut Context<Self>) {
        let response = window.prompt(
            PromptLevel::Warning,
            "Save changes before continuing?",
            Some("Your unsaved session changes will be lost if you discard them."),
            &["Save", "Discard", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            let choice = match response.await {
                Ok(0) => GuardChoice::Save,
                Ok(1) => GuardChoice::Discard,
                _ => GuardChoice::Cancel,
            };
            let _ = this.update_in(cx, |this, window, cx| {
                let effect = this.session_guard.borrow_mut().choose(choice);
                this.apply_guard_effect(effect, window, cx);
            });
        })
        .detach();
    }

    fn prompt_for_guarded_save_destination(&self, cx: &mut Context<Self>) {
        let folder = crate::show_file::default_show_folder();
        let file_name = suggested_save_file_name(self.presentation.snapshot(), false);
        let response = cx.prompt_for_new_path(&folder, Some(&file_name));
        cx.spawn(async move |this, cx| match response.await {
            Ok(Ok(path)) => {
                let path = path.map(ensure_show_file_extension);
                let _ = this.update_in(cx, |this, window, cx| {
                    let effect = this.session_guard.borrow_mut().save_destination(path);
                    this.apply_guard_effect(effect, window, cx);
                });
            }
            Ok(Err(error)) => {
                let _ = this.update_in(cx, |this, window, cx| {
                    let effect = this.session_guard.borrow_mut().save_destination(None);
                    this.apply_guard_effect(effect, window, cx);
                    window.push_notification(
                        Notification::error(format!("Could not open the save picker: {error}")),
                        cx,
                    );
                });
            }
            Err(error) => {
                let _ = this.update_in(cx, |this, window, cx| {
                    let effect = this.session_guard.borrow_mut().save_destination(None);
                    this.apply_guard_effect(effect, window, cx);
                    window.push_notification(
                        Notification::error(format!(
                            "The save picker did not return a result: {error}"
                        )),
                        cx,
                    );
                });
            }
        })
        .detach();
    }

    pub fn new_show(&self) {
        self.pending_save_command_id.set(None);
        self.dispatcher
            .dispatch_serial(|commands| async move { commands.new_show_file().await.map(|_| ()) });
    }

    fn guarded_new_show(&self, expected_persisted_revision: u64) {
        self.pending_save_command_id.set(None);
        self.dispatcher.dispatch_serial(move |commands| async move {
            commands
                .guarded_new_show_file(expected_persisted_revision)
                .await
                .map(|_| ())
        });
    }

    pub fn new_show_from_template(&self, cx: &mut Context<Self>) {
        self.new_show_from_template_with_revision(None, cx);
    }

    fn guarded_new_show_from_template(
        &self,
        expected_persisted_revision: u64,
        cx: &mut Context<Self>,
    ) {
        self.new_show_from_template_with_revision(Some(expected_persisted_revision), cx);
    }

    fn new_show_from_template_with_revision(
        &self,
        expected_persisted_revision: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        self.pending_save_command_id.set(None);
        let response = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Template".into()),
        });
        let dispatcher = self.dispatcher.clone();
        cx.spawn(async move |this, cx| match response.await {
            Ok(Ok(Some(paths))) => {
                let path = paths.into_iter().next();
                if path.as_ref().is_some_and(|path| !is_show_file_path(path)) {
                    Self::push_async_error(
                        &this,
                        "Select an Advanced Show Control template (.ascs).".to_string(),
                        cx,
                    );
                    return;
                }
                dispatcher.dispatch_serial(move |commands| async move {
                    match expected_persisted_revision {
                        Some(revision) => commands
                            .guarded_new_show_file_from_template(path, revision)
                            .await
                            .map(|_| ()),
                        None => commands.new_show_file_from_template(path).await.map(|_| ()),
                    }
                });
            }
            Ok(Ok(None)) => {}
            Ok(Err(error)) => Self::push_async_error(
                &this,
                format!("Could not open the template picker: {error}"),
                cx,
            ),
            Err(error) => Self::push_async_error(
                &this,
                format!("The template picker did not return a result: {error}"),
                cx,
            ),
        })
        .detach();
    }

    pub fn open_show(&self, cx: &mut Context<Self>) {
        self.open_show_with_revision(None, cx);
    }

    fn guarded_open_show(&self, expected_persisted_revision: u64, cx: &mut Context<Self>) {
        self.open_show_with_revision(Some(expected_persisted_revision), cx);
    }

    fn open_show_with_revision(
        &self,
        expected_persisted_revision: Option<u64>,
        cx: &mut Context<Self>,
    ) {
        self.pending_save_command_id.set(None);
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
                if path.as_ref().is_some_and(|path| !is_show_file_path(path)) {
                    Self::push_async_error(
                        &this,
                        "Select an Advanced Show Control session (.ascs).".to_string(),
                        cx,
                    );
                    return;
                }
                dispatcher.dispatch_serial(move |commands| async move {
                    match expected_persisted_revision {
                        Some(revision) => commands
                            .guarded_open_show_file(path, revision)
                            .await
                            .map(|_| ()),
                        None => commands.open_show_file(path).await.map(|_| ()),
                    }
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

    pub fn save_show(&self) {
        self.pending_save_command_id
            .set(Some(self.dispatcher.save_show()));
    }

    pub fn save_show_as(&self, cx: &mut Context<Self>) {
        self.pending_save_command_id.set(None);
        self.prompt_to_save(true, cx);
    }

    fn prompt_to_save(&self, save_as: bool, cx: &mut Context<Self>) {
        let folder = crate::show_file::default_show_folder();
        let file_name = suggested_save_file_name(self.presentation.snapshot(), save_as);
        let response = cx.prompt_for_new_path(&folder, Some(&file_name));
        let dispatcher = self.dispatcher.clone();
        cx.spawn(async move |this, cx| match response.await {
            Ok(Ok(Some(path))) => {
                let path = ensure_show_file_extension(path);
                dispatcher.dispatch_serial(move |commands| async move {
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

    fn modal_open(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        window.has_active_dialog(cx) || self.shell.read(cx).modal_open(cx)
    }

    /// Session actions MUST remain blocked while shortcut capture, a native prompt/dialog, an app
    /// modal, or another destructive-session guard is active.
    fn action_blocked(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.shell.read(cx).shortcut_capture_active(cx) {
            // Let Settings capture the key without running the menu action.
            cx.propagate();
            return true;
        }
        window.has_active_prompt()
            || self.modal_open(window, cx)
            || self.session_guard.borrow().is_pending()
    }

    fn on_about(&mut self, _: &About, window: &mut Window, cx: &mut Context<Self>) {
        if !window.has_active_prompt() && !self.action_blocked(window, cx) {
            let _response = window.prompt(
                PromptLevel::Info,
                "Advanced Show Control",
                Some(about_detail()),
                &["OK"],
                cx,
            );
        }
    }

    fn on_new_show(&mut self, _: &NewShow, window: &mut Window, cx: &mut Context<Self>) {
        if !self.action_blocked(window, cx) {
            self.request_session_action(SessionAction::New, window, cx);
        }
    }

    fn on_new_show_from_template(
        &mut self,
        _: &NewShowFromTemplate,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.action_blocked(window, cx) {
            self.request_session_action(SessionAction::NewFromTemplate, window, cx);
        }
    }

    fn on_open_show(&mut self, _: &OpenShow, window: &mut Window, cx: &mut Context<Self>) {
        if !self.action_blocked(window, cx) {
            self.request_session_action(SessionAction::Open, window, cx);
        }
    }

    fn on_save_show(&mut self, _: &SaveShow, window: &mut Window, cx: &mut Context<Self>) {
        if !self.action_blocked(window, cx) {
            self.save_show();
        }
    }

    fn on_save_show_as(&mut self, _: &SaveShowAs, window: &mut Window, cx: &mut Context<Self>) {
        if !self.action_blocked(window, cx) {
            self.save_show_as(cx);
        }
    }

    #[cfg(target_os = "macos")]
    fn on_hide(&mut self, _: &Hide, _: &mut Window, cx: &mut Context<Self>) {
        if self.shell.read(cx).shortcut_capture_active(cx) {
            cx.propagate();
        } else {
            cx.hide();
        }
    }

    #[cfg(target_os = "macos")]
    fn on_hide_others(&mut self, _: &HideOthers, _: &mut Window, cx: &mut Context<Self>) {
        if self.shell.read(cx).shortcut_capture_active(cx) {
            cx.propagate();
        } else {
            cx.hide_other_apps();
        }
    }

    fn on_quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if self.shell.read(cx).shortcut_capture_active(cx) {
            cx.propagate();
        } else if !self.action_blocked(window, cx) {
            self.request_session_action(SessionAction::Quit, window, cx);
        }
    }

    /// @cc [owner:mixxorz,label:safety;keyboard] go-shortcut-routing
    /// A matching GO keydown MUST be consumed and each distinct non-held press with a resolvable
    /// projected cue MUST dispatch while fewer than eight GO commands are unsettled, including while
    /// earlier recalls are pending. Repeats and key events owned by editable controls, dialogs, or an
    /// open session menu MUST NOT dispatch.
    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let custom_modal_open = self.shell.read(cx).modal_open(cx);
        if custom_modal_open && normalized_physical_key(&event.keystroke) == "Escape" {
            self.shell.update(cx, |shell, cx| {
                shell.dismiss_modal(window, cx);
            });
            cx.stop_propagation();
            return;
        }
        let interaction = InteractionState {
            modal_open: window.has_active_dialog(cx)
                || custom_modal_open
                || self.shell.read(cx).session_menu_open(),
            editable_focused: event.prefer_character_input,
        };
        let capture_active = self.shell.read(cx).shortcut_capture_active(cx);
        let snapshot = self.presentation.snapshot();
        let Some(action) = route_action(
            event,
            capture_active,
            interaction,
            &snapshot.settings.keyboard_shortcuts.go,
            &snapshot.settings.keyboard_shortcuts.cue,
        ) else {
            return;
        };

        match action {
            RoutedAction::Go => {
                cx.stop_propagation();
                if event.is_held {
                    return;
                }
                self.shell.update(cx, |shell, cx| {
                    shell.submit_go(window, cx);
                });
            }
            RoutedAction::Cue => {
                cx.stop_propagation();
                if event.is_held || self.shell.read(cx).active_tab() != MainTab::CueLists {
                    return;
                }
                self.cue_lists
                    .update(cx, |cue_lists, cx| cue_lists.cue_selected(cx));
            }
        }
    }
}

fn session_query_is_stale(
    query_edit_epoch: u64,
    current_edit_epoch: u64,
    result: &Result<crate::show::ShowSessionState, String>,
    current_persisted_revision: u64,
) -> bool {
    query_edit_epoch != current_edit_epoch
        || result
            .as_ref()
            .is_ok_and(|state| state.persisted_session_revision != current_persisted_revision)
}

fn modal_surface_active(native_prompt: bool, native_dialog: bool, custom_modal: bool) -> bool {
    native_prompt || native_dialog || custom_modal
}

fn cue_completion_matches_session(session_revision: u64, snapshot: &AppViewState) -> bool {
    session_revision == snapshot.session_revision
}

fn about_detail() -> &'static str {
    concat!(
        "Version ",
        env!("CARGO_PKG_VERSION"),
        "\n\nGPUI Kit desktop control for Waves eMotion LV1 scene fades.",
        "\n\nGPL-3.0-or-later",
        "\nhttps://mitchel.me/advanced-show-control/"
    )
}

fn is_show_file_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ascs"))
}

fn suggested_save_file_name(snapshot: &AppViewState, save_as: bool) -> String {
    if !save_as {
        return "Untitled Session.ascs".to_string();
    }
    if snapshot.show_file_name.ends_with(".ascs") {
        snapshot.show_file_name.clone()
    } else {
        format!("{}.ascs", snapshot.show_file_name)
    }
}

fn ensure_show_file_extension(mut path: std::path::PathBuf) -> std::path::PathBuf {
    if !is_show_file_path(&path) {
        path.set_extension("ascs");
    }
    path
}

impl Render for AppRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div()
            .id("advanced-show-control")
            .relative()
            .size_full()
            .key_context(global_key_context())
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::on_about))
            .on_action(cx.listener(Self::on_new_show))
            .on_action(cx.listener(Self::on_new_show_from_template))
            .on_action(cx.listener(Self::on_open_show))
            .on_action(cx.listener(Self::on_save_show))
            .on_action(cx.listener(Self::on_save_show_as))
            .on_action(cx.listener(Self::on_quit));
        #[cfg(target_os = "macos")]
        let root = root
            .on_action(cx.listener(Self::on_hide))
            .on_action(cx.listener(Self::on_hide_others));
        root.on_key_down(cx.listener(Self::on_key_down))
            .child(self.shell.clone())
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        about_detail, cue_completion_matches_session, ensure_show_file_extension,
        is_show_file_path, modal_surface_active, session_query_is_stale, suggested_save_file_name,
    };
    use crate::projector::AppViewState;

    #[test]
    fn owner_revision_change_makes_successful_query_stale() {
        let result = Ok(crate::show::ShowSessionState {
            show_file_path: None,
            show_file_dirty: false,
            persisted_session_revision: 4,
        });

        assert!(!session_query_is_stale(2, 2, &result, 4));
        assert!(session_query_is_stale(2, 2, &result, 5));
        assert!(session_query_is_stale(2, 3, &result, 4));
    }

    #[test]
    fn close_request_is_blocked_while_any_modal_surface_is_active() {
        assert!(!modal_surface_active(false, false, false));
        assert!(modal_surface_active(true, false, false));
        assert!(modal_surface_active(false, true, false));
        assert!(modal_surface_active(false, false, true));
    }

    #[test]
    fn stale_cue_completion_cannot_invalidate_a_new_session() {
        let snapshot = AppViewState {
            session_revision: 2,
            ..Default::default()
        };

        assert!(!cue_completion_matches_session(1, &snapshot));
        assert!(cue_completion_matches_session(2, &snapshot));
    }

    #[test]
    fn show_file_paths_require_the_session_extension_case_insensitively() {
        assert!(is_show_file_path(Path::new("show.ascs")));
        assert!(is_show_file_path(Path::new("show.ASCS")));
        assert!(!is_show_file_path(Path::new("show.json")));
        assert!(!is_show_file_path(Path::new("show")));
    }

    #[test]
    fn save_paths_gain_the_session_extension_only_when_missing() {
        assert_eq!(
            ensure_show_file_extension(PathBuf::from("show")),
            PathBuf::from("show.ascs")
        );
        assert_eq!(
            ensure_show_file_extension(PathBuf::from("show.txt")),
            PathBuf::from("show.ascs")
        );
    }

    #[test]
    fn ordinary_save_uses_an_untitled_suggestion_when_the_authoritative_path_is_empty() {
        let stale_snapshot = AppViewState {
            show_file_name: "Source Template.ascs".to_string(),
            ..Default::default()
        };

        assert_eq!(
            suggested_save_file_name(&stale_snapshot, false),
            "Untitled Session.ascs"
        );
        assert_eq!(
            suggested_save_file_name(&stale_snapshot, true),
            "Source Template.ascs"
        );
    }

    #[test]
    fn about_detail_identifies_the_project_and_license() {
        assert_eq!(
            about_detail(),
            concat!(
                "Version ",
                env!("CARGO_PKG_VERSION"),
                "\n\nGPUI Kit desktop control for Waves eMotion LV1 scene fades.",
                "\n\nGPL-3.0-or-later",
                "\nhttps://mitchel.me/advanced-show-control/"
            )
        );
    }
}
