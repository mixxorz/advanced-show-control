use gpui_kit::component::switch::Switch;
use gpui_kit::{
    AnyElement, Context, FocusHandle, IntoElement, KeyDownEvent, Render, SharedString, Window, div,
    prelude::*, px, rgb,
};

use crate::projector::AppViewState;
use crate::settings::{AppSettings, KeyboardShortcut, TimeDisplayFormat};

use super::CommandDispatcher;
use super::keyboard::{CaptureResult, ShortcutCapture, shortcuts_equal};
use super::theme::{
    CONSOLE_CONTROL, CONSOLE_CONTROL_HOVER, CONSOLE_LINE, CONSOLE_MUTED, CONSOLE_PANEL,
    CONSOLE_PRIMARY, STATUS_DANGER,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ShortcutAction {
    Go,
    Cue,
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
}

impl SettingsView {
    pub fn new(
        snapshot: AppViewState,
        dispatcher: CommandDispatcher,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            snapshot,
            draft: None,
            pending_command_id: None,
            dispatcher,
            focus: cx.focus_handle(),
            capture: ShortcutCapture::default(),
            capture_action: None,
            shortcut_conflict: None,
        }
    }

    /// Accepts projected settings as authoritative and acknowledges a matching optimistic draft.
    pub fn set_snapshot(
        &mut self,
        snapshot: AppViewState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.draft.as_ref() == Some(&snapshot.settings) {
            self.draft = None;
            self.pending_command_id = None;
        }
        self.snapshot = snapshot;
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

    pub fn command_finished(&mut self, command_id: u64, failed: bool, cx: &mut Context<Self>) {
        if failed && self.pending_command_id == Some(command_id) {
            self.draft = None;
            self.pending_command_id = None;
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
                .checked(checked)
                .on_change(listener)
                .into_any_element(),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn stepper(
        &self,
        id: &'static str,
        label: &'static str,
        value: String,
        can_decrease: bool,
        can_increase: bool,
        decrease: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
        increase: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut gpui_kit::App) + 'static,
    ) -> AnyElement {
        setting_row(
            label,
            div()
                .flex()
                .items_center()
                .child(
                    control_button(format!("{id}-decrease"), "−", can_decrease).on_click(decrease),
                )
                .child(
                    div()
                        .w(px(86.))
                        .text_center()
                        .font_family("Fira Code")
                        .child(value),
                )
                .child(
                    control_button(format!("{id}-increase"), "+", can_increase).on_click(increase),
                )
                .into_any_element(),
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
                        true,
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.begin_capture(action, window, cx)
                    })),
                )
                .when_some(conflict, |element, conflict| {
                    element.child(div().text_color(rgb(STATUS_DANGER)).child(conflict))
                })
                .into_any_element(),
        )
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.settings().clone();
        div()
            .id("settings-view")
            .track_focus(&self.focus)
            .key_context(super::keyboard::shortcut_capture_key_context())
            .on_key_down(cx.listener(Self::on_key_down))
            .size_full()
            .bg(rgb(CONSOLE_PANEL))
            .text_color(rgb(CONSOLE_PRIMARY))
            .overflow_y_scroll()
            .child(
                div()
                    .border_b_1()
                    .border_color(rgb(CONSOLE_LINE))
                    .px_4()
                    .py_3()
                    .text_lg()
                    .child("SETTINGS"),
            )
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
                            .child(control_button("time-12", "12 hour", true).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.update(cx, |s| {
                                        s.time_display = TimeDisplayFormat::TwelveHour
                                    })
                                }),
                            ))
                            .child(control_button("time-24", "24 hour", true).on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.update(cx, |s| {
                                        s.time_display = TimeDisplayFormat::TwentyFourHour
                                    })
                                }),
                            ))
                            .into_any_element(),
                    ))
                    .child(self.stepper(
                        "sensitivity",
                        "Fader override sensitivity",
                        settings.fader_override_sensitivity.to_string(),
                        settings.fader_override_sensitivity > 1,
                        settings.fader_override_sensitivity < 10,
                        cx.listener(|this, _, _, cx| {
                            this.update(cx, |s| {
                                s.fader_override_sensitivity =
                                    s.fader_override_sensitivity.saturating_sub(1).max(1)
                            })
                        }),
                        cx.listener(|this, _, _, cx| {
                            this.update(cx, |s| {
                                s.fader_override_sensitivity =
                                    s.fader_override_sensitivity.saturating_add(1).min(10)
                            })
                        }),
                    ))
                    .child(self.toggle_row(
                        "same-scene",
                        "Same scene recall finishing",
                        settings.same_scene_recall_enabled,
                        cx.listener(|this, checked, _, cx| {
                            this.update(cx, |s| s.same_scene_recall_enabled = *checked)
                        }),
                    ))
                    .child(self.stepper(
                        "same-scene-threshold",
                        "Same scene recall threshold",
                        format!("{} ms", settings.same_scene_recall_threshold_ms),
                        settings.same_scene_recall_threshold_ms > 0,
                        settings.same_scene_recall_threshold_ms < 5_000,
                        cx.listener(|this, _, _, cx| {
                            this.update(cx, |s| {
                                s.same_scene_recall_threshold_ms =
                                    s.same_scene_recall_threshold_ms.saturating_sub(100)
                            })
                        }),
                        cx.listener(|this, _, _, cx| {
                            this.update(cx, |s| {
                                s.same_scene_recall_threshold_ms = s
                                    .same_scene_recall_threshold_ms
                                    .saturating_add(100)
                                    .min(5_000)
                            })
                        }),
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

fn control_button(
    id: impl Into<gpui_kit::ElementId>,
    label: impl Into<SharedString>,
    enabled: bool,
) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
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
        .child(label.into())
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

fn fixed_shortcut_conflicts() -> [(&'static str, KeyboardShortcut); 4] {
    [
        ("New Session", fixed_file_shortcut("N", false)),
        ("Open Session", fixed_file_shortcut("O", false)),
        ("Save Session", fixed_file_shortcut("S", false)),
        ("Save As", fixed_file_shortcut("S", true)),
    ]
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
