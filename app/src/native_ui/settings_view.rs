use gpui_kit::base::Button as BaseButton;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::switch::Switch;
use gpui_kit::{
    AnyElement, Context, Entity, FocusHandle, IntoElement, KeyDownEvent, Render, SharedString,
    Subscription, TestSupportExt as _, Window, div, prelude::*, px, rgb,
};

use crate::projector::AppViewState;
use crate::settings::{AppSettings, KeyboardShortcut, TimeDisplayFormat};

use super::CommandDispatcher;
use super::keyboard::{CaptureResult, ShortcutCapture, normalized_physical_key, shortcuts_equal};
use super::numeric_control::editable_numeric_control;
use super::panel::panel_header;
use super::theme::{
    ACCENT_ORANGE, CONSOLE_CONTROL, CONSOLE_CONTROL_HOVER, CONSOLE_LINE, CONSOLE_MUTED,
    CONSOLE_PANEL, CONSOLE_PRIMARY, STATUS_DANGER,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShortcutAction {
    Go,
    Cue,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NumericSetting {
    Sensitivity,
    SameSceneThreshold,
}

impl NumericSetting {
    const fn index(self) -> usize {
        match self {
            Self::Sensitivity => 0,
            Self::SameSceneThreshold => 1,
        }
    }
}

pub struct SettingsView {
    snapshot: AppViewState,
    draft: Option<AppSettings>,
    pending_command_id: Option<u64>,
    dispatcher: CommandDispatcher,
    focus: FocusHandle,
    capture: ShortcutCapture,
    capture_action: Option<ShortcutAction>,
    shortcut_conflict: Option<(ShortcutAction, String)>,
    sensitivity_input: Entity<InputState>,
    threshold_input: Entity<InputState>,
    numeric_identity: (u8, u64),
    numeric_edit_revisions: [u64; 2],
    _numeric_subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(
        snapshot: AppViewState,
        dispatcher: CommandDispatcher,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let numeric_identity = (
            snapshot.settings.fader_override_sensitivity,
            snapshot.settings.same_scene_recall_threshold_ms,
        );
        let sensitivity_input =
            cx.new(|cx| InputState::new(window, cx).default_value(numeric_identity.0.to_string()));
        let threshold_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(format_threshold_ms(numeric_identity.1))
        });
        let numeric_subscriptions = vec![
            cx.subscribe_in(
                &sensitivity_input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    this.handle_numeric_event(NumericSetting::Sensitivity, event, window, cx)
                },
            ),
            cx.subscribe_in(
                &threshold_input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    this.handle_numeric_event(NumericSetting::SameSceneThreshold, event, window, cx)
                },
            ),
        ];
        Self {
            snapshot,
            draft: None,
            pending_command_id: None,
            dispatcher,
            focus: cx.focus_handle(),
            capture: ShortcutCapture::default(),
            capture_action: None,
            shortcut_conflict: None,
            sensitivity_input,
            threshold_input,
            numeric_identity,
            numeric_edit_revisions: [0; 2],
            _numeric_subscriptions: numeric_subscriptions,
        }
    }

    pub fn capture_active(&self) -> bool {
        self.capture.active_id().is_some()
    }

    pub fn cancel_capture(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.capture_active() {
            return false;
        }
        self.capture = ShortcutCapture::default();
        self.capture_action = None;
        self.shortcut_conflict = None;
        cx.notify();
        true
    }

    /// Accepts projected settings as authoritative and acknowledges a matching optimistic draft.
    pub fn set_snapshot(
        &mut self,
        snapshot: AppViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.draft.as_ref() == Some(&snapshot.settings) {
            self.draft = None;
            self.pending_command_id = None;
        }
        self.snapshot = snapshot;
        let numeric_identity = (
            self.settings().fader_override_sensitivity,
            self.settings().same_scene_recall_threshold_ms,
        );
        if numeric_identity.0 != self.numeric_identity.0 {
            self.reset_numeric(
                NumericSetting::Sensitivity,
                numeric_identity.0.to_string(),
                window,
                cx,
            );
        }
        if numeric_identity.1 != self.numeric_identity.1 {
            self.reset_numeric(
                NumericSetting::SameSceneThreshold,
                format_threshold_ms(numeric_identity.1),
                window,
                cx,
            );
        }
        self.numeric_identity = numeric_identity;
        cx.notify();
    }

    fn settings(&self) -> &AppSettings {
        self.draft.as_ref().unwrap_or(&self.snapshot.settings)
    }

    fn replace(&mut self, settings: AppSettings, cx: &mut Context<Self>) {
        self.draft = Some(settings.clone());
        self.shortcut_conflict = None;
        self.pending_command_id =
            Some(self.dispatcher.dispatch_serial(move |commands| async move {
                commands.replace_app_settings(settings).await.map(|_| ())
            }));
        cx.notify();
    }

    pub fn command_finished(
        &mut self,
        command_id: u64,
        failed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if failed && self.pending_command_id == Some(command_id) {
            self.draft = None;
            self.pending_command_id = None;
            self.sync_numeric_inputs(window, cx);
            cx.notify();
        }
    }

    fn update(&mut self, cx: &mut Context<Self>, update: impl FnOnce(&mut AppSettings)) {
        let mut settings = self.settings().clone();
        update(&mut settings);
        self.replace(settings, cx);
    }

    fn begin_capture(
        &mut self,
        action: ShortcutAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.capture.start(match action {
            ShortcutAction::Go => "go",
            ShortcutAction::Cue => "cue",
        });
        self.capture_action = Some(action);
        self.shortcut_conflict = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let result = self.capture.route(event);
        match result {
            CaptureResult::Inactive => return,
            CaptureResult::Pending => {}
            CaptureResult::Cancelled => self.capture_action = None,
            CaptureResult::Captured(shortcut) => {
                if let Some(action) = self.capture_action.take() {
                    self.apply_shortcut(action, shortcut, cx);
                }
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn apply_shortcut(
        &mut self,
        action: ShortcutAction,
        shortcut: KeyboardShortcut,
        cx: &mut Context<Self>,
    ) {
        if let Some(label) = shortcut_conflict_label(action, &shortcut, self.settings()) {
            self.shortcut_conflict = Some((action, format!("Already assigned to {label}")));
            return;
        }
        self.update(cx, |settings| match action {
            ShortcutAction::Go => settings.keyboard_shortcuts.go = shortcut,
            ShortcutAction::Cue => settings.keyboard_shortcuts.cue = shortcut,
        });
    }

    fn toggle_row(
        &self,
        id: &'static str,
        label: &'static str,
        checked: bool,
        listener: impl Fn(&bool, &mut Window, &mut gpui_kit::App) + 'static,
    ) -> AnyElement {
        setting_row(
            label,
            Switch::new(id)
                .accessibility_label(label)
                .checked(checked)
                .on_change(listener)
                .into_any_element(),
        )
    }

    fn numeric_input(&self, setting: NumericSetting) -> &Entity<InputState> {
        match setting {
            NumericSetting::Sensitivity => &self.sensitivity_input,
            NumericSetting::SameSceneThreshold => &self.threshold_input,
        }
    }

    fn handle_numeric_event(
        &mut self,
        setting: NumericSetting,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            InputEvent::PressEnter { .. } => self.commit_numeric(setting, window, cx),
            InputEvent::Blur => {
                let revision = self.numeric_edit_revisions[setting.index()];
                cx.on_next_frame(window, move |this, window, cx| {
                    if this.numeric_edit_revisions[setting.index()] == revision {
                        this.commit_numeric(setting, window, cx);
                    }
                });
            }
            InputEvent::Change => cx.notify(),
            _ => {}
        }
    }

    fn commit_numeric(
        &mut self,
        setting: NumericSetting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = self.numeric_input(setting).read(cx).value().to_string();
        let mut settings = self.settings().clone();
        let formatted = match setting {
            NumericSetting::Sensitivity => {
                let Some(value) = normalize_sensitivity(&draft) else {
                    self.discard_numeric(setting, window, cx);
                    return;
                };
                settings.fader_override_sensitivity = value;
                value.to_string()
            }
            NumericSetting::SameSceneThreshold => {
                let Some(value) = normalize_threshold_ms(&draft) else {
                    self.discard_numeric(setting, window, cx);
                    return;
                };
                settings.same_scene_recall_threshold_ms = value;
                format_threshold_ms(value)
            }
        };
        self.numeric_edit_revisions[setting.index()] =
            self.numeric_edit_revisions[setting.index()].wrapping_add(1);
        self.reset_numeric(setting, formatted, window, cx);
        if &settings != self.settings() {
            self.numeric_identity = (
                settings.fader_override_sensitivity,
                settings.same_scene_recall_threshold_ms,
            );
            self.replace(settings, cx);
        }
    }

    fn step_numeric(
        &mut self,
        setting: NumericSetting,
        direction: i64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let draft = self.numeric_input(setting).read(cx).value().to_string();
        let current = self.settings().clone();
        let mut next = current.clone();
        let formatted = match setting {
            NumericSetting::Sensitivity => {
                let value =
                    stepped_sensitivity(&draft, current.fader_override_sensitivity, direction);
                next.fader_override_sensitivity = value;
                value.to_string()
            }
            NumericSetting::SameSceneThreshold => {
                let value =
                    stepped_threshold_ms(&draft, current.same_scene_recall_threshold_ms, direction);
                next.same_scene_recall_threshold_ms = value;
                format_threshold_ms(value)
            }
        };
        self.numeric_edit_revisions[setting.index()] =
            self.numeric_edit_revisions[setting.index()].wrapping_add(1);
        self.reset_numeric(setting, formatted, window, cx);
        if next != current {
            self.numeric_identity = (
                next.fader_override_sensitivity,
                next.same_scene_recall_threshold_ms,
            );
            self.replace(next, cx);
        }
    }

    fn discard_numeric(
        &mut self,
        setting: NumericSetting,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.numeric_edit_revisions[setting.index()] =
            self.numeric_edit_revisions[setting.index()].wrapping_add(1);
        let value = match setting {
            NumericSetting::Sensitivity => self.settings().fader_override_sensitivity.to_string(),
            NumericSetting::SameSceneThreshold => {
                format_threshold_ms(self.settings().same_scene_recall_threshold_ms)
            }
        };
        self.reset_numeric(setting, value, window, cx);
    }

    fn reset_numeric(
        &self,
        setting: NumericSetting,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.numeric_input(setting)
            .update(cx, |input, cx| input.set_value(value, window, cx));
    }

    fn sync_numeric_inputs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let identity = (
            self.settings().fader_override_sensitivity,
            self.settings().same_scene_recall_threshold_ms,
        );
        self.reset_numeric(
            NumericSetting::Sensitivity,
            identity.0.to_string(),
            window,
            cx,
        );
        self.reset_numeric(
            NumericSetting::SameSceneThreshold,
            format_threshold_ms(identity.1),
            window,
            cx,
        );
        self.numeric_identity = identity;
    }

    fn numeric_row(
        &self,
        setting: NumericSetting,
        id: &'static str,
        label: &'static str,
        can_decrease: bool,
        can_increase: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        setting_row(
            label,
            editable_numeric_control(
                id,
                label,
                self.numeric_input(setting),
                can_decrease,
                can_increase,
                cx.listener(move |this, _, window, cx| this.step_numeric(setting, -1, window, cx)),
                cx.listener(move |this, _, window, cx| this.step_numeric(setting, 1, window, cx)),
                cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                    if normalized_physical_key(&event.keystroke) == "Escape" {
                        this.discard_numeric(setting, window, cx);
                        cx.stop_propagation();
                    }
                }),
            ),
        )
    }

    fn shortcut_row(
        &self,
        action: ShortcutAction,
        shortcut: &KeyboardShortcut,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, label) = match action {
            ShortcutAction::Go => ("go-shortcut", "GO"),
            ShortcutAction::Cue => ("cue-shortcut", "CUE"),
        };
        let capturing = self.capture_action == Some(action) && self.capture.active_id().is_some();
        let conflict = self
            .shortcut_conflict
            .as_ref()
            .filter(|(conflict_action, _)| *conflict_action == action)
            .map(|(_, message)| message.clone());
        setting_row(
            label,
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    control_button(
                        id,
                        if capturing {
                            SharedString::from("...")
                        } else {
                            format_shortcut(shortcut)
                        },
                        format!("Set {label} keyboard shortcut"),
                        true,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.begin_capture(action, window, cx)
                    })),
                )
                .when_some(conflict, |element, conflict| {
                    element.child(
                        div()
                            .id(format!("{id}-conflict"))
                            .test_support()
                            .text_color(rgb(STATUS_DANGER))
                            .child(conflict),
                    )
                })
                .into_any_element(),
        )
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.settings().clone();
        let sensitivity_draft = self.sensitivity_input.read(cx).value().to_string();
        let sensitivity_base = normalize_sensitivity(&sensitivity_draft)
            .unwrap_or(settings.fader_override_sensitivity);
        let threshold_draft = self.threshold_input.read(cx).value().to_string();
        let threshold_base = normalize_threshold_ms(&threshold_draft)
            .unwrap_or(settings.same_scene_recall_threshold_ms);
        div()
            .id("settings-view")
            .test_support()
            .track_focus(&self.focus)
            .key_context(super::keyboard::shortcut_capture_key_context())
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .bg(rgb(CONSOLE_PANEL))
            .border_1()
            .border_color(rgb(CONSOLE_LINE))
            .text_color(rgb(CONSOLE_PRIMARY))
            .overflow_y_scroll()
            .child(panel_header("SETTINGS"))
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_8()
                    .child(section_title("GENERAL"))
                    .child(self.toggle_row(
                        "auto-load",
                        "Auto load last show file",
                        settings.auto_load_last_show_file,
                        cx.listener(|this, checked, _, cx| {
                            this.update(cx, |s| s.auto_load_last_show_file = *checked)
                        }),
                    ))
                    .child(self.toggle_row(
                        "auto-save",
                        "Auto save sessions",
                        settings.auto_save_sessions,
                        cx.listener(|this, checked, _, cx| {
                            this.update(cx, |s| s.auto_save_sessions = *checked)
                        }),
                    ))
                    .child(setting_row(
                        "Time display",
                        div()
                            .flex()
                            .child(
                                control_button(
                                    "time-12",
                                    "12 hour",
                                    "Use 12 hour time display",
                                    true,
                                )
                                .selected(settings.time_display == TimeDisplayFormat::TwelveHour)
                                .when(
                                    settings.time_display == TimeDisplayFormat::TwelveHour,
                                    selected_control,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.update(cx, |s| {
                                            s.time_display = TimeDisplayFormat::TwelveHour
                                        })
                                    },
                                )),
                            )
                            .child(
                                control_button(
                                    "time-24",
                                    "24 hour",
                                    "Use 24 hour time display",
                                    true,
                                )
                                .selected(
                                    settings.time_display == TimeDisplayFormat::TwentyFourHour,
                                )
                                .when(
                                    settings.time_display == TimeDisplayFormat::TwentyFourHour,
                                    selected_control,
                                )
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.update(cx, |s| {
                                            s.time_display = TimeDisplayFormat::TwentyFourHour
                                        })
                                    },
                                )),
                            )
                            .into_any_element(),
                    ))
                    .child(self.numeric_row(
                        NumericSetting::Sensitivity,
                        "sensitivity",
                        "Fader override sensitivity",
                        sensitivity_base > 1,
                        sensitivity_base < 10,
                        cx,
                    ))
                    .child(self.toggle_row(
                        "same-scene",
                        "Same scene recall finishing",
                        settings.same_scene_recall_enabled,
                        cx.listener(|this, checked, _, cx| {
                            this.update(cx, |s| s.same_scene_recall_enabled = *checked)
                        }),
                    ))
                    .child(self.numeric_row(
                        NumericSetting::SameSceneThreshold,
                        "same-scene-threshold",
                        "Same scene recall threshold",
                        threshold_base > 0,
                        threshold_base < 5_000,
                        cx,
                    ))
                    .child(self.toggle_row(
                        "diagnostics",
                        "Extensive diagnostics",
                        settings.enable_extensive_diagnostics,
                        cx.listener(|this, checked, _, cx| {
                            this.update(cx, |s| s.enable_extensive_diagnostics = *checked)
                        }),
                    ))
                    .child(section_title("KEYBOARD SHORTCUTS"))
                    .child(self.shortcut_row(
                        ShortcutAction::Go,
                        &settings.keyboard_shortcuts.go,
                        cx,
                    ))
                    .child(self.shortcut_row(
                        ShortcutAction::Cue,
                        &settings.keyboard_shortcuts.cue,
                        cx,
                    )),
            )
    }
}

fn section_title(title: &'static str) -> AnyElement {
    div().text_xs().child(title).into_any_element()
}

fn setting_row(label: &'static str, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap_4()
        .child(
            div()
                .w(px(288.))
                .text_color(rgb(CONSOLE_MUTED))
                .child(label),
        )
        .child(control)
        .into_any_element()
}

fn parse_clamped_unsigned(draft: &str, min: u64, max: u64) -> Option<u64> {
    let value = draft.trim();
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parsed = value.bytes().fold(0_u64, |parsed, byte| {
        parsed
            .saturating_mul(10)
            .saturating_add(u64::from(byte - b'0'))
    });
    Some(parsed.clamp(min, max))
}

fn normalize_sensitivity(draft: &str) -> Option<u8> {
    parse_clamped_unsigned(draft, 1, 10).map(|value| value as u8)
}

fn normalize_threshold_ms(draft: &str) -> Option<u64> {
    let lowercase = draft.trim().to_ascii_lowercase();
    let value = lowercase.strip_suffix("ms").unwrap_or(&lowercase);
    parse_clamped_unsigned(value, 0, 5_000)
}

fn stepped_sensitivity(draft: &str, current: u8, direction: i64) -> u8 {
    let value = normalize_sensitivity(draft).unwrap_or(current);
    if direction > 0 {
        value.saturating_add(1).min(10)
    } else {
        value.saturating_sub(1).max(1)
    }
}

fn stepped_threshold_ms(draft: &str, current: u64, direction: i64) -> u64 {
    let value = normalize_threshold_ms(draft).unwrap_or(current);
    if direction > 0 {
        value.saturating_add(100).min(5_000)
    } else {
        value.saturating_sub(100)
    }
}

fn format_threshold_ms(value: u64) -> String {
    format!("{value} ms")
}

fn control_button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
    accessibility_label: impl Into<SharedString>,
    enabled: bool,
) -> BaseButton {
    let label: SharedString = label.into();
    BaseButton::new(id)
        .accessibility_label(accessibility_label)
        .disabled(!enabled)
        .px_3()
        .py_2()
        .border_1()
        .border_color(rgb(CONSOLE_LINE))
        .bg(rgb(CONSOLE_CONTROL))
        .text_color(if enabled {
            rgb(CONSOLE_PRIMARY)
        } else {
            rgb(CONSOLE_MUTED)
        })
        .when(enabled, |button| {
            button
                .cursor_pointer()
                .hover(|style| style.bg(rgb(CONSOLE_CONTROL_HOVER)))
        })
        .child(label.to_uppercase())
}

fn selected_control(control: BaseButton) -> BaseButton {
    control
        .border_color(rgb(ACCENT_ORANGE))
        .text_color(rgb(ACCENT_ORANGE))
}

fn shortcut_conflict_label(
    action: ShortcutAction,
    shortcut: &KeyboardShortcut,
    settings: &AppSettings,
) -> Option<&'static str> {
    let other = match action {
        ShortcutAction::Go => (&settings.keyboard_shortcuts.cue, "Cue"),
        ShortcutAction::Cue => (&settings.keyboard_shortcuts.go, "GO"),
    };
    if shortcuts_equal(shortcut, other.0) {
        return Some(other.1);
    }
    fixed_shortcut_conflicts()
        .into_iter()
        .find(|(_, fixed)| shortcuts_equal(shortcut, fixed))
        .map(|(label, _)| label)
}

fn fixed_shortcut_conflicts() -> Vec<(&'static str, KeyboardShortcut)> {
    let conflicts = vec![
        ("New Session", fixed_file_shortcut("N", false)),
        ("Open Session", fixed_file_shortcut("O", false)),
        ("Save Session", fixed_file_shortcut("S", false)),
        ("Save As", fixed_file_shortcut("S", true)),
        ("Quit Advanced Show Control", fixed_quit_shortcut()),
    ];
    #[cfg(target_os = "macos")]
    let conflicts = {
        let mut conflicts = conflicts;
        conflicts.extend([
            (
                "Hide Advanced Show Control",
                fixed_macos_shortcut("H", false),
            ),
            ("Hide Others", fixed_macos_shortcut("H", true)),
        ]);
        conflicts
    };
    conflicts
}

fn fixed_quit_shortcut() -> KeyboardShortcut {
    KeyboardShortcut {
        key: if cfg!(target_os = "macos") { "Q" } else { "F4" }.into(),
        modifiers: crate::settings::KeyboardShortcutModifiers {
            shift: false,
            control: false,
            alt: !cfg!(target_os = "macos"),
            meta: cfg!(target_os = "macos"),
        },
    }
}

fn fixed_file_shortcut(key: &str, shift: bool) -> KeyboardShortcut {
    KeyboardShortcut {
        key: key.into(),
        modifiers: crate::settings::KeyboardShortcutModifiers {
            shift,
            control: !cfg!(target_os = "macos"),
            alt: false,
            meta: cfg!(target_os = "macos"),
        },
    }
}

#[cfg(target_os = "macos")]
fn fixed_macos_shortcut(key: &str, alt: bool) -> KeyboardShortcut {
    KeyboardShortcut {
        key: key.into(),
        modifiers: crate::settings::KeyboardShortcutModifiers {
            shift: false,
            control: false,
            alt,
            meta: true,
        },
    }
}

fn format_shortcut(shortcut: &KeyboardShortcut) -> SharedString {
    let mut value = String::new();
    if shortcut.modifiers.control {
        value.push_str("Ctrl+");
    }
    if shortcut.modifiers.alt {
        value.push_str("Alt+");
    }
    if shortcut.modifiers.shift {
        value.push_str("Shift+");
    }
    if shortcut.modifiers.meta {
        value.push_str(if cfg!(target_os = "macos") {
            "⌘"
        } else {
            "Meta+"
        });
    }
    value.push_str(&shortcut.key);
    value.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_settings_accept_typed_units_and_clamp_to_domain_ranges() {
        assert_eq!(normalize_sensitivity(" 4 "), Some(4));
        assert_eq!(normalize_sensitivity("0"), Some(1));
        assert_eq!(normalize_sensitivity("99"), Some(10));
        assert_eq!(normalize_sensitivity("999"), Some(10));
        assert_eq!(normalize_sensitivity("999999999999999999999999"), Some(10));
        assert_eq!(normalize_sensitivity("2.5"), None);
        assert_eq!(normalize_sensitivity("-1"), None);

        assert_eq!(normalize_threshold_ms(" 1200 ms "), Some(1_200));
        assert_eq!(normalize_threshold_ms("750MS"), Some(750));
        assert_eq!(normalize_threshold_ms("9999"), Some(5_000));
        assert_eq!(
            normalize_threshold_ms("999999999999999999999999 ms"),
            Some(5_000)
        );
        assert_eq!(normalize_threshold_ms("1.5 ms"), None);
        assert_eq!(normalize_threshold_ms("-100"), None);
    }

    #[test]
    fn numeric_setting_steps_use_the_typed_draft_or_current_value() {
        assert_eq!(stepped_sensitivity("4", 9, 1), 5);
        assert_eq!(stepped_sensitivity("invalid", 9, -1), 8);
        assert_eq!(stepped_sensitivity("10", 9, 1), 10);

        assert_eq!(stepped_threshold_ms("1200 ms", 500, 1), 1_300);
        assert_eq!(stepped_threshold_ms("invalid", 500, -1), 400);
        assert_eq!(stepped_threshold_ms("0", 500, -1), 0);
    }

    #[test]
    fn conflicts_require_key_case_equivalence_and_exact_modifiers() {
        let settings = AppSettings::default();
        let mut cue = settings.keyboard_shortcuts.cue.clone();
        cue.key = "c".into();
        assert_eq!(
            shortcut_conflict_label(ShortcutAction::Go, &cue, &settings),
            Some("Cue")
        );
        cue.modifiers.shift = true;
        assert_eq!(
            shortcut_conflict_label(ShortcutAction::Go, &cue, &settings),
            None
        );
    }

    #[test]
    fn file_conflicts_follow_the_platform_primary_modifier() {
        let save = fixed_file_shortcut("S", false);
        assert_eq!(
            shortcut_conflict_label(ShortcutAction::Go, &save, &AppSettings::default()),
            Some("Save Session")
        );
        assert_eq!(save.modifiers.meta, cfg!(target_os = "macos"));
        assert_eq!(save.modifiers.control, !cfg!(target_os = "macos"));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_quit_shortcut_is_reserved() {
        assert_eq!(
            shortcut_conflict_label(
                ShortcutAction::Go,
                &fixed_quit_shortcut(),
                &AppSettings::default()
            ),
            Some("Quit Advanced Show Control")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_application_shortcuts_are_reserved() {
        for (shortcut, label) in [
            (
                fixed_macos_shortcut("H", false),
                "Hide Advanced Show Control",
            ),
            (fixed_macos_shortcut("H", true), "Hide Others"),
            (
                fixed_macos_shortcut("Q", false),
                "Quit Advanced Show Control",
            ),
        ] {
            assert_eq!(
                shortcut_conflict_label(ShortcutAction::Go, &shortcut, &AppSettings::default()),
                Some(label)
            );
        }
    }

    #[test]
    fn shortcut_display_includes_every_modifier() {
        let shortcut = KeyboardShortcut {
            key: "Enter".into(),
            modifiers: crate::settings::KeyboardShortcutModifiers {
                shift: true,
                control: true,
                alt: true,
                meta: false,
            },
        };
        assert_eq!(format_shortcut(&shortcut).as_ref(), "Ctrl+Alt+Shift+Enter");
    }

    #[test]
    fn captured_events_produce_complete_shortcuts() {
        let event = KeyDownEvent {
            keystroke: gpui_kit::Keystroke {
                key: "q".into(),
                key_char: Some("q".into()),
                modifiers: gpui_kit::Modifiers {
                    control: true,
                    ..Default::default()
                },
            },
            is_held: false,
            prefer_character_input: false,
        };
        let shortcut = super::super::keyboard::shortcut_from_event(&event);
        assert_eq!(shortcut.key, "Q");
        assert!(shortcut.modifiers.control);
        assert!(!shortcut.modifiers.shift);
    }
}
