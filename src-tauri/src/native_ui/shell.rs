use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
    rgb,
};

use crate::projector::{AppConnectionState, AppFadeState, AppViewState};

use super::cues::CueListsView;
use super::logs::LogsView;
use super::scenes::ScenesView;
use super::settings_view::SettingsView;
use super::theme::{
    ACCENT_ORANGE, CONSOLE_BG, CONSOLE_CHROME, CONSOLE_LINE, CONSOLE_MUTED, CONSOLE_PRIMARY,
    CONSOLE_SECONDARY, STATUS_CUED, STATUS_CURRENT, STATUS_DANGER, STATUS_WARNING,
};
use super::{CommandDispatcher, MainTab};

type OpenConnection = dyn Fn(&mut Window, &mut gpui_kit::App);

pub struct AppShell {
    active_tab: MainTab,
    snapshot: AppViewState,
    dispatcher: CommandDispatcher,
    scenes: Entity<ScenesView>,
    cue_lists: Entity<CueListsView>,
    settings: Entity<SettingsView>,
    logs: Entity<LogsView>,
    go_command_id: Rc<Cell<Option<u64>>>,
    open_connection: Box<OpenConnection>,
}

impl AppShell {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        snapshot: AppViewState,
        dispatcher: CommandDispatcher,
        scenes: Entity<ScenesView>,
        cue_lists: Entity<CueListsView>,
        settings: Entity<SettingsView>,
        logs: Entity<LogsView>,
        go_command_id: Rc<Cell<Option<u64>>>,
        open_connection: impl Fn(&mut Window, &mut gpui_kit::App) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                executor.timer(Duration::from_secs(1)).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            active_tab: MainTab::Scenes,
            snapshot,
            dispatcher,
            scenes,
            cue_lists,
            settings,
            logs,
            go_command_id,
            open_connection: Box::new(open_connection),
        }
    }

    pub fn active_tab(&self) -> MainTab {
        self.active_tab
    }

    pub fn command_finished(&self, command_id: u64, failed: bool, cx: &mut Context<Self>) {
        self.cue_lists.update(cx, |cue_lists, cx| {
            cue_lists.command_finished(command_id, failed, cx)
        });
        self.settings.update(cx, |settings, cx| {
            settings.command_finished(command_id, failed, cx)
        });
    }

    pub fn set_snapshot(
        &mut self,
        snapshot: AppViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snapshot = snapshot.clone();
        self.scenes.update(cx, |view, cx| {
            view.set_snapshot(snapshot.clone(), window, cx)
        });
        self.cue_lists.update(cx, |view, cx| {
            view.set_snapshot(snapshot.clone(), window, cx)
        });
        self.settings.update(cx, |view, cx| {
            view.set_snapshot(snapshot.clone(), window, cx)
        });
        self.logs
            .update(cx, |view, cx| view.set_snapshot(snapshot, window, cx));
        cx.notify();
    }

    fn tab(&self, tab: MainTab, label: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_tab == tab;
        div()
            .id(format!("tab-{label}"))
            .px_5()
            .py_3()
            .border_b_2()
            .border_color(rgb(if active {
                ACCENT_ORANGE
            } else {
                CONSOLE_CHROME
            }))
            .text_color(rgb(if active {
                ACCENT_ORANGE
            } else {
                CONSOLE_SECONDARY
            }))
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
                this.active_tab = tab;
                cx.notify();
            }))
            .child(label)
    }

    fn content(&self) -> gpui_kit::AnyElement {
        match self.active_tab {
            MainTab::Scenes => self.scenes.clone().into_any_element(),
            MainTab::CueLists => self.cue_lists.clone().into_any_element(),
            MainTab::Events => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(CONSOLE_MUTED))
                .child("Events")
                .into_any_element(),
            MainTab::Logs => self.logs.clone().into_any_element(),
            MainTab::Settings => self.settings.clone().into_any_element(),
        }
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let connection_label = match self.snapshot.connection {
            AppConnectionState::Connected => "CONNECTED",
            AppConnectionState::Connecting => "CONNECTING",
            AppConnectionState::Disconnected => "OFFLINE",
        };
        let connection_color = match self.snapshot.connection {
            AppConnectionState::Connected => STATUS_CUED,
            AppConnectionState::Connecting => CONSOLE_SECONDARY,
            AppConnectionState::Disconnected => STATUS_DANGER,
        };
        let console_name = self
            .snapshot
            .connected_lv1_identity
            .as_ref()
            .and_then(|identity| identity.host.clone())
            .unwrap_or_else(|| "Console A".to_string());
        let lockout = self.snapshot.lockout;
        let lockout_dispatcher = self.dispatcher.clone();
        let abort_dispatcher = self.dispatcher.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(CONSOLE_BG))
            .text_color(rgb(CONSOLE_PRIMARY))
            .child(
                div()
                    .mx_3()
                    .mt_3()
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(rgb(CONSOLE_LINE))
                    .bg(rgb(CONSOLE_CHROME))
                    .child(
                        div().flex().flex_1().children([
                            self.tab(MainTab::Scenes, "Scenes", cx).into_any_element(),
                            self.tab(MainTab::CueLists, "Cue Lists", cx)
                                .into_any_element(),
                            self.tab(MainTab::Events, "Events", cx).into_any_element(),
                            self.tab(MainTab::Logs, "Logs", cx).into_any_element(),
                            self.tab(MainTab::Settings, "Settings", cx)
                                .into_any_element(),
                        ]),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .px_4()
                            .child(
                                div()
                                    .text_color(rgb(connection_color))
                                    .child(connection_label),
                            )
                            .child(
                                div()
                                    .id("open-connection")
                                    .min_w(px(144.))
                                    .px_3()
                                    .py_2()
                                    .border_1()
                                    .border_color(rgb(CONSOLE_LINE))
                                    .cursor_pointer()
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        (this.open_connection)(window, cx);
                                    }))
                                    .child(console_name),
                            )
                            .child(
                                Button::new("toggle-lockout")
                                    .label("SAFE")
                                    .when(lockout, |button| button.warning())
                                    .on_click(move |_, _, _| {
                                        lockout_dispatcher.dispatch(move |commands| async move {
                                            commands.set_lockout(!lockout).await.map(|_| ())
                                        });
                                    }),
                            )
                            .child(
                                Button::new("abort-all")
                                    .danger()
                                    .label("Abort All")
                                    .on_click(move |_, _, _| {
                                        abort_dispatcher.dispatch(|commands| async move {
                                            commands.abort_all_fades().await
                                        });
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p_3()
                    .overflow_hidden()
                    .child(self.content()),
            )
            .child(bottom_status(self, cx))
    }
}

/// @cc [owner:mixxorz,label:safety;product] go-single-flight
/// GO MUST be disabled without a resolvable cued scene and while a recall is pending, submit at
/// most one recall concurrently, and clear its pending guard after success or failure.
fn bottom_status(shell: &AppShell, cx: &mut Context<AppShell>) -> impl IntoElement {
    let current = shell
        .snapshot
        .current_scene
        .as_ref()
        .map(|scene| scene.name.as_str())
        .unwrap_or("---");
    let cued = resolve_cued_scene(&shell.snapshot);
    let mode = if shell.snapshot.connection != AppConnectionState::Connected {
        ("Offline", CONSOLE_SECONDARY)
    } else if shell.snapshot.lockout {
        ("Safe", STATUS_WARNING)
    } else if shell.snapshot.fade_state == AppFadeState::Running {
        let pulse = chrono::Local::now().timestamp_subsec_millis() < 500;
        ("Fading", if pulse { STATUS_WARNING } else { CONSOLE_MUTED })
    } else {
        ("Ready", STATUS_CUED)
    };
    let can_go = cued.is_some() && shell.go_command_id.get().is_none();
    let go_dispatcher = shell.dispatcher.clone();
    let go_command_id = shell.go_command_id.clone();
    let entity = cx.entity();

    div()
        .mx_3()
        .mb_3()
        .flex()
        .items_stretch()
        .border_1()
        .border_color(rgb(CONSOLE_LINE))
        .bg(rgb(CONSOLE_CHROME))
        .child(
            div()
                .flex()
                .flex_1()
                .items_center()
                .p_3()
                .border_r_1()
                .border_color(rgb(CONSOLE_LINE))
                .child(
                    Button::new("go")
                        .primary()
                        .label("GO")
                        .disabled(!can_go)
                        .on_click(move |_, _, cx| {
                            if go_command_id.get().is_some() {
                                return;
                            }
                            let id = go_dispatcher.dispatch(|commands| async move {
                                commands.recall_cued_cue().await.map(|_| ())
                            });
                            go_command_id.set(Some(id));
                            entity.update(cx, |_, cx| cx.notify());
                        }),
                ),
        )
        .child(status_cell(
            "CUED",
            cued.map(|scene| scene.scene_name.as_str()).unwrap_or("---"),
            if cued.is_some() {
                STATUS_CUED
            } else {
                CONSOLE_PRIMARY
            },
        ))
        .child(status_cell("CURRENT", current, STATUS_CURRENT))
        .child(status_cell("MODE", mode.0, mode.1))
        .child(status_cell(
            "TIME",
            &chrono::Local::now().format("%H:%M:%S").to_string(),
            CONSOLE_PRIMARY,
        ))
}

/// @cc [owner:mixxorz,label:product] cued-scene-resolution
/// A cued scene MUST resolve through the active cue list, then the cued entry, then its referenced
/// scene config; a missing link MUST produce no scene rather than falling back to selection.
fn resolve_cued_scene(snapshot: &AppViewState) -> Option<&crate::scenes::SceneConfig> {
    let active_list_id = snapshot.active_cue_list_id.as_deref()?;
    let cued_entry_id = snapshot.cued_cue_entry_id.as_deref()?;
    let entry = snapshot
        .cue_lists
        .iter()
        .find(|list| list.id.to_string() == active_list_id)?
        .entries
        .iter()
        .find(|entry| entry.id.to_string() == cued_entry_id)?;
    snapshot
        .scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id == entry.scene_internal_id)
}

fn status_cell(label: &'static str, value: &str, color: u32) -> impl IntoElement {
    div()
        .flex_1()
        .p_3()
        .border_r_1()
        .border_color(rgb(CONSOLE_LINE))
        .child(div().text_xs().text_color(rgb(CONSOLE_MUTED)).child(label))
        .child(div().text_color(rgb(color)).child(value.to_string()))
}
