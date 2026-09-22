use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::{Button as BaseButton, StyledExt as _};
use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::{
    Context, Entity, FocusHandle, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestSupportExt as _, Window, div, prelude::FluentBuilder as _, px, relative, rgb,
};

use crate::projector::{AppConnectionState, AppFadeState, AppViewState};
use crate::settings::TimeDisplayFormat;

use super::button::bordered_button;
use super::cues::{CueListsView, displayed_next_cue_entry_id};
use super::logs::LogsView;
use super::menu::session_menu_button;
use super::scenes::ScenesView;
use super::settings_view::SettingsView;
use super::state::GoSubmissionGuard;
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
    go_submissions: Rc<RefCell<GoSubmissionGuard>>,
    action_context: FocusHandle,
    session_menu_open: bool,
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
        go_submissions: Rc<RefCell<GoSubmissionGuard>>,
        action_context: FocusHandle,
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
            go_submissions,
            action_context,
            session_menu_open: false,
            open_connection: Box::new(open_connection),
        }
    }

    pub fn active_tab(&self) -> MainTab {
        self.active_tab
    }

    pub fn session_menu_open(&self) -> bool {
        self.session_menu_open
    }

    pub fn modal_open(&self, cx: &gpui_kit::App) -> bool {
        self.scenes.read(cx).modal_open() || self.cue_lists.read(cx).modal_open()
    }

    pub fn dismiss_modal(&self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self
            .scenes
            .update(cx, |scenes, cx| scenes.dismiss_modal(window, cx))
        {
            return true;
        }
        self.cue_lists
            .update(cx, |cue_lists, cx| cue_lists.dismiss_modal(window, cx))
    }

    pub fn shortcut_capture_active(&self, cx: &gpui_kit::App) -> bool {
        self.settings.read(cx).capture_active()
    }

    pub(super) fn submit_go(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let pending_go_count = self.go_submissions.borrow().presentation_pending_count();
        let Some(next_entry_id) = resolve_next_entry_id(&self.snapshot, pending_go_count) else {
            return false;
        };
        if resolve_next_scene(&self.snapshot, pending_go_count).is_none()
            || !self.go_submissions.borrow().can_submit()
            || self.modal_open(cx)
            || self.session_menu_open
            || self.shortcut_capture_active(cx)
        {
            return false;
        }

        self.action_context.focus(window, cx);
        let command_id = self
            .dispatcher
            .dispatch_cued_cue_recall(self.snapshot.session_revision);
        self.go_submissions.borrow_mut().start(
            command_id,
            self.snapshot.state_version,
            self.snapshot.session_revision,
            next_entry_id,
        );
        self.cue_lists.update(cx, |cues, cx| cues.go_submitted(cx));
        cx.notify();
        true
    }

    pub fn command_finished(
        &self,
        command_id: u64,
        failed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scenes.update(cx, |scenes, cx| {
            scenes.command_finished(command_id, failed, window, cx)
        });
        self.cue_lists.update(cx, |cue_lists, cx| {
            cue_lists.command_finished(command_id, failed, window, cx)
        });
        self.settings.update(cx, |settings, cx| {
            settings.command_finished(command_id, failed, window, cx)
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
        BaseButton::new(format!("tab-{label}"))
            .accessibility_label(format!("{label} tab"))
            .selected(active)
            .disabled(self.modal_open(cx) || self.session_menu_open)
            .px_5()
            .py_4()
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
                if tab != MainTab::Settings {
                    this.settings
                        .update(cx, |settings, cx| settings.cancel_capture(cx));
                }
                this.active_tab = tab;
                cx.notify();
            }))
            .child(label.to_ascii_uppercase())
    }

    fn content(&self) -> gpui_kit::AnyElement {
        match self.active_tab {
            MainTab::Scenes => self.scenes.clone().into_any_element(),
            MainTab::CueLists => self.cue_lists.clone().into_any_element(),
            MainTab::Logs => self.logs.clone().into_any_element(),
            MainTab::Settings => self.settings.clone().into_any_element(),
        }
    }
}

impl Render for AppShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (connection_label, connection_color, connection_label_color) =
            connection_presentation(&self.snapshot.connection);
        let console_name = console_display_name(
            &self.snapshot.connection,
            self.snapshot
                .connected_lv1_identity
                .as_ref()
                .and_then(|identity| identity.host.as_deref()),
        );
        let lockout = self.snapshot.lockout;
        let trigger_disabled = self.modal_open(cx) || self.shortcut_capture_active(cx);
        let actions_blocked = trigger_disabled || self.session_menu_open;
        let lockout_dispatcher = self.dispatcher.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(CONSOLE_BG))
            .text_color(rgb(CONSOLE_PRIMARY))
            .child(
                div()
                    .id("top-bar")
                    .test_support()
                    .mx_3()
                    .mt_3()
                    .flex()
                    .items_stretch()
                    .border_1()
                    .border_color(rgb(CONSOLE_LINE))
                    .bg(rgb(CONSOLE_CHROME))
                    .child(
                        div().flex().flex_1().items_stretch().children([
                            session_menu_button(self.action_context.clone(), trigger_disabled, {
                                let entity = cx.entity();
                                move |open, _, cx| {
                                    entity.update(cx, |shell, cx| {
                                        shell.session_menu_open = *open;
                                        cx.notify();
                                    });
                                }
                            })
                            .into_any_element(),
                            self.tab(MainTab::Scenes, "Scenes", cx).into_any_element(),
                            self.tab(MainTab::CueLists, "Cue Lists", cx)
                                .into_any_element(),
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
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_color(rgb(connection_label_color))
                                    .child(
                                        div()
                                            .id("connection-status-dot")
                                            .test_support()
                                            .size(px(8.))
                                            .rounded_full()
                                            .bg(rgb(connection_color)),
                                    )
                                    .child(
                                        div()
                                            .id("connection-status-label")
                                            .test_support()
                                            .child(connection_label),
                                    ),
                            )
                            .child(
                                bordered_button("open-connection")
                                    .accessibility_label("Open LV1 connection")
                                    .label(console_name.to_uppercase())
                                    .dropdown_caret(true)
                                    .disabled(actions_blocked)
                                    .min_w(px(144.))
                                    .px_3()
                                    .py_2()
                                    .border_color(rgb(CONSOLE_LINE))
                                    .cursor_pointer()
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        (this.open_connection)(window, cx);
                                    })),
                            )
                            .child(
                                bordered_button("toggle-lockout")
                                    .accessibility_label("Toggle safe mode")
                                    .label("SAFE")
                                    .toggled(lockout)
                                    .when(lockout, |button| button.warning())
                                    .disabled(actions_blocked)
                                    .on_click(move |_, _, _| {
                                        lockout_dispatcher.dispatch_serial(
                                            move |commands| async move {
                                                commands.set_lockout(!lockout).await.map(|_| ())
                                            },
                                        );
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

/// @cc [owner:mixxorz,label:safety;product] go-submission-guard
/// GO MUST be disabled without a resolvable cued scene or when eight GO commands are unsettled, but
/// MUST remain available below that capacity while earlier GO recalls are unsettled. Pointer
/// multi-click sequences MUST dispatch only their first click so an accidental double-click cannot
/// enqueue a duplicate recall.
fn bottom_status(shell: &AppShell, cx: &mut Context<AppShell>) -> impl IntoElement {
    let current = shell
        .snapshot
        .current_scene
        .as_ref()
        .map(|scene| scene.name.as_str())
        .unwrap_or("---");
    let pending_go_count = shell.go_submissions.borrow().presentation_pending_count();
    let cued = resolve_next_scene(&shell.snapshot, pending_go_count);
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
    let can_go = cued.is_some()
        && shell.go_submissions.borrow().can_submit()
        && !shell.modal_open(cx)
        && !shell.session_menu_open
        && !shell.shortcut_capture_active(cx);
    let entity = cx.entity();

    div()
        .id("bottom-status")
        .test_support()
        .mx_3()
        .mb_3()
        .h(px(72.))
        .flex()
        .items_stretch()
        .border_1()
        .border_color(rgb(CONSOLE_LINE))
        .bg(rgb(CONSOLE_CHROME))
        .child(
            div()
                .id("go-cell")
                .test_support()
                .flex()
                .flex_none()
                .w(relative(0.14))
                .items_center()
                .p_2()
                .border_r_1()
                .border_color(rgb(CONSOLE_LINE))
                .child(
                    bordered_button("go")
                        .primary()
                        .size_full()
                        .text_xl()
                        .font_semibold()
                        .accessibility_label("Recall cued scene")
                        .label("GO")
                        .disabled(!can_go)
                        .on_click(move |event, window, cx| {
                            if !should_dispatch_go_click(event.click_count()) {
                                return;
                            }
                            entity.update(cx, |shell, cx| {
                                shell.submit_go(window, cx);
                            });
                        }),
                ),
        )
        .child(status_cell(
            "NEXT",
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
            &format_time(
                chrono::Local::now().time(),
                &shell.snapshot.settings.time_display,
            ),
            CONSOLE_PRIMARY,
        ))
}

/// @cc [owner:mixxorz,label:product] cued-scene-resolution
/// The displayed Next scene MUST resolve through the active cue list, advance from the projected
/// cued entry by the presentation-pending GO count, then resolve its referenced scene config. GO
/// commands that a newer cue projection already acknowledges MUST NOT offset Next while they await
/// command completion. A missing link MUST produce no scene rather than falling back to selection.
pub(super) fn resolve_next_entry_id(
    snapshot: &AppViewState,
    pending_go_count: usize,
) -> Option<uuid::Uuid> {
    let active_list_id = snapshot.active_cue_list_id.as_deref()?;
    let projected_next_entry_id = snapshot
        .cued_cue_entry_id
        .as_deref()
        .and_then(|id| uuid::Uuid::parse_str(id).ok());
    let list = snapshot
        .cue_lists
        .iter()
        .find(|list| list.id.to_string() == active_list_id)?;
    displayed_next_cue_entry_id(&list.entries, projected_next_entry_id, pending_go_count)
}

pub(super) fn resolve_next_scene(
    snapshot: &AppViewState,
    pending_go_count: usize,
) -> Option<&crate::scenes::SceneConfig> {
    let active_list_id = snapshot.active_cue_list_id.as_deref()?;
    let next_entry_id = resolve_next_entry_id(snapshot, pending_go_count)?;
    let list = snapshot
        .cue_lists
        .iter()
        .find(|list| list.id.to_string() == active_list_id)?;
    let entry = list
        .entries
        .iter()
        .find(|entry| entry.id == next_entry_id)?;
    snapshot
        .scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id == entry.scene_internal_id)
}

fn connection_presentation(connection: &AppConnectionState) -> (&'static str, u32, u32) {
    match connection {
        AppConnectionState::Connected => ("CONNECTED", STATUS_CUED, CONSOLE_PRIMARY),
        AppConnectionState::Connecting => ("CONNECTING", STATUS_WARNING, CONSOLE_PRIMARY),
        AppConnectionState::Disconnected => ("OFFLINE", STATUS_DANGER, CONSOLE_PRIMARY),
    }
}

fn console_display_name(connection: &AppConnectionState, host: Option<&str>) -> String {
    match connection {
        AppConnectionState::Disconnected => "CONNECT CONSOLE".to_owned(),
        AppConnectionState::Connecting => host.unwrap_or("CONNECTING…").to_owned(),
        AppConnectionState::Connected => host.unwrap_or("CONNECTED CONSOLE").to_owned(),
    }
}

fn should_dispatch_go_click(click_count: usize) -> bool {
    click_count == 1
}

fn format_time(time: chrono::NaiveTime, format: &TimeDisplayFormat) -> String {
    match format {
        TimeDisplayFormat::TwentyFourHour => time.format("%H:%M:%S").to_string(),
        TimeDisplayFormat::TwelveHour => {
            let formatted = time.format("%I:%M:%S %p").to_string();
            formatted
                .strip_prefix('0')
                .unwrap_or(&formatted)
                .to_string()
        }
    }
}

fn status_cell(label: &'static str, value: &str, color: u32) -> impl IntoElement {
    div()
        .id(format!("status-{}", label.to_ascii_lowercase()))
        .test_support()
        .flex_basis(px(0.))
        .flex_grow_1()
        .p_3()
        .when(label != "TIME", |cell| {
            cell.border_r_1().border_color(rgb(CONSOLE_LINE))
        })
        .child(div().text_xs().text_color(rgb(CONSOLE_MUTED)).child(label))
        .child(div().text_color(rgb(color)).child(value.to_string()))
}

#[cfg(test)]
mod tests {
    use chrono::NaiveTime;

    use super::{
        AppConnectionState, AppViewState, CONSOLE_PRIMARY, STATUS_CUED, STATUS_DANGER,
        STATUS_WARNING, TimeDisplayFormat, connection_presentation, console_display_name,
        format_time, resolve_next_scene, should_dispatch_go_click,
    };
    use crate::cue_lists::{CueEntry, CueList};
    use crate::native_ui::state::{GO_SUBMISSION_CAPACITY, GoSubmissionGuard};
    use crate::scenes::SceneConfig;
    use uuid::Uuid;

    #[test]
    fn connection_presentation_maps_projected_state_to_label_and_status_color() {
        assert_eq!(
            connection_presentation(&AppConnectionState::Connected),
            ("CONNECTED", STATUS_CUED, CONSOLE_PRIMARY)
        );
        assert_eq!(
            connection_presentation(&AppConnectionState::Connecting),
            ("CONNECTING", STATUS_WARNING, CONSOLE_PRIMARY)
        );
        assert_eq!(
            connection_presentation(&AppConnectionState::Disconnected),
            ("OFFLINE", STATUS_DANGER, CONSOLE_PRIMARY)
        );
    }

    #[test]
    fn go_submission_guard_allows_eight_unsettled_commands() {
        let mut guard = GoSubmissionGuard::default();

        for command_id in 1..=GO_SUBMISSION_CAPACITY as u64 {
            assert!(guard.can_submit());
            guard.start(command_id, 0, 0, Uuid::from_u128(command_id as u128));
        }
        assert!(!guard.can_submit());

        let snapshot = AppViewState::default();
        assert!(guard.finish(1, None, &snapshot));
        assert!(guard.can_submit());
        guard.start(9, 0, 0, Uuid::from_u128(9));
        assert!(!guard.can_submit());
        assert!(!guard.finish(1, None, &snapshot));
        assert!(!guard.finish(99, None, &snapshot));
    }

    #[test]
    fn next_scene_advances_immediately_for_each_unsettled_go() {
        let list_id = Uuid::from_u128(1);
        let first_entry_id = Uuid::from_u128(2);
        let second_entry_id = Uuid::from_u128(3);
        let first_scene_id = Uuid::from_u128(4);
        let second_scene_id = Uuid::from_u128(5);
        let scene = |id: Uuid, name: &str| SceneConfig {
            internal_scene_id: id,
            scene_index: Some(0),
            scene_name: name.to_string(),
            duration_ms: 0,
            channel_configs: Vec::new(),
            scoped_channels: Vec::new(),
            scope_toggles: Default::default(),
        };
        let snapshot = AppViewState {
            cue_lists: vec![CueList {
                id: list_id,
                name: "Main".to_string(),
                entries: vec![
                    CueEntry {
                        id: first_entry_id,
                        scene_internal_id: first_scene_id,
                    },
                    CueEntry {
                        id: second_entry_id,
                        scene_internal_id: second_scene_id,
                    },
                ],
            }],
            active_cue_list_id: Some(list_id.to_string()),
            cued_cue_entry_id: Some(first_entry_id.to_string()),
            scene_configs: vec![
                scene(first_scene_id, "First"),
                scene(second_scene_id, "Second"),
            ],
            ..Default::default()
        };

        assert_eq!(
            resolve_next_scene(&snapshot, 0).map(|scene| scene.scene_name.as_str()),
            Some("First")
        );
        assert_eq!(
            resolve_next_scene(&snapshot, 1).map(|scene| scene.scene_name.as_str()),
            Some("Second")
        );
        assert!(resolve_next_scene(&snapshot, 2).is_none());
    }

    #[test]
    fn go_click_guard_rejects_double_click_events_but_accepts_distinct_clicks() {
        assert!(should_dispatch_go_click(1));
        assert!(should_dispatch_go_click(1));
        assert!(!should_dispatch_go_click(2));
        assert!(!should_dispatch_go_click(3));
    }

    #[test]
    fn console_display_name_is_actionable_while_offline() {
        assert_eq!(
            console_display_name(&AppConnectionState::Disconnected, None),
            "CONNECT CONSOLE"
        );
        assert_eq!(
            console_display_name(&AppConnectionState::Disconnected, Some("Stale Console")),
            "CONNECT CONSOLE"
        );
        assert_eq!(
            console_display_name(&AppConnectionState::Connected, Some("FOH")),
            "FOH"
        );
    }

    #[test]
    fn clock_respects_the_selected_time_display() {
        let afternoon = NaiveTime::from_hms_opt(13, 5, 9).unwrap();
        assert_eq!(
            format_time(afternoon, &TimeDisplayFormat::TwentyFourHour),
            "13:05:09"
        );
        assert_eq!(
            format_time(afternoon, &TimeDisplayFormat::TwelveHour),
            "1:05:09 PM"
        );
    }
}
