use gpui_kit::{KeyContext, KeyDownEvent, Keystroke, Modifiers};

use crate::settings::{KeyboardShortcut, KeyboardShortcutModifiers};

pub const GLOBAL_KEY_CONTEXT: &str = "AscGlobal";
pub const SHORTCUT_CAPTURE_KEY_CONTEXT: &str = "AscShortcutCapture";

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InteractionState {
    pub modal_open: bool,
    pub editable_focused: bool,
}

impl InteractionState {
    pub fn suppresses_actions(self) -> bool {
        self.modal_open || self.editable_focused
    }
}

pub fn global_key_context() -> KeyContext {
    KeyContext::try_from(GLOBAL_KEY_CONTEXT).expect("static global key context must parse")
}

pub fn shortcut_capture_key_context() -> KeyContext {
    KeyContext::try_from(SHORTCUT_CAPTURE_KEY_CONTEXT)
        .expect("static shortcut capture key context must parse")
}

/// Converts GPUI's layout-independent key into the persisted shortcut label.
pub fn normalized_physical_key(keystroke: &Keystroke) -> String {
    let key = match keystroke.key.as_str() {
        "space" | " " => "Space",
        "enter" | "return" => "Enter",
        "escape" | "esc" => "Escape",
        "tab" => "Tab",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "up" | "arrowup" => "ArrowUp",
        "down" | "arrowdown" => "ArrowDown",
        "left" | "arrowleft" => "ArrowLeft",
        "right" | "arrowright" => "ArrowRight",
        key => key,
    };
    if key.chars().count() == 1 {
        key.to_uppercase()
    } else {
        key.to_string()
    }
}

pub fn shortcut_modifiers(modifiers: Modifiers) -> KeyboardShortcutModifiers {
    KeyboardShortcutModifiers {
        shift: modifiers.shift,
        control: modifiers.control,
        alt: modifiers.alt,
        meta: modifiers.platform,
    }
}

pub fn shortcut_from_event(event: &KeyDownEvent) -> KeyboardShortcut {
    KeyboardShortcut {
        key: normalized_physical_key(&event.keystroke),
        modifiers: shortcut_modifiers(event.keystroke.modifiers),
    }
}

pub fn shortcut_keys_equal(left: &str, right: &str) -> bool {
    left.to_uppercase() == right.to_uppercase()
}

pub fn shortcuts_equal(left: &KeyboardShortcut, right: &KeyboardShortcut) -> bool {
    shortcut_keys_equal(&left.key, &right.key) && left.modifiers == right.modifiers
}

pub fn shortcut_matches(shortcut: &KeyboardShortcut, event: &KeyDownEvent) -> bool {
    let physical = normalized_physical_key(&event.keystroke);
    shortcut_keys_equal(&shortcut.key, &physical)
        && shortcut.modifiers == shortcut_modifiers(event.keystroke.modifiers)
}

pub fn is_modifier_only(keystroke: &Keystroke) -> bool {
    matches!(
        keystroke.key.as_str(),
        "shift" | "control" | "ctrl" | "alt" | "meta" | "cmd" | "command" | "super"
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CaptureResult {
    Inactive,
    Pending,
    Cancelled,
    Captured(KeyboardShortcut),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ShortcutCapture {
    active_id: Option<String>,
}

impl ShortcutCapture {
    pub fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    pub fn start(&mut self, id: impl Into<String>) {
        self.active_id = Some(id.into());
    }

    /// Capture is intentionally evaluated before GO/CUE routing.
    pub fn route(&mut self, event: &KeyDownEvent) -> CaptureResult {
        if self.active_id.is_none() {
            return CaptureResult::Inactive;
        }
        if normalized_physical_key(&event.keystroke) == "Escape" {
            self.active_id = None;
            return CaptureResult::Cancelled;
        }
        if is_modifier_only(&event.keystroke) {
            return CaptureResult::Pending;
        }
        self.active_id = None;
        CaptureResult::Captured(shortcut_from_event(event))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutedAction {
    Go,
    Cue,
}

/// Returns an action only when capture, modal, and editable contexts permit global routing.
pub fn route_action(
    event: &KeyDownEvent,
    capture_active: bool,
    interaction: InteractionState,
    go: &KeyboardShortcut,
    cue: &KeyboardShortcut,
) -> Option<RoutedAction> {
    if capture_active || interaction.suppresses_actions() {
        return None;
    }
    if shortcut_matches(go, event) {
        Some(RoutedAction::Go)
    } else if shortcut_matches(cue, event) {
        Some(RoutedAction::Cue)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: &str, key_char: Option<&str>, modifiers: Modifiers) -> KeyDownEvent {
        KeyDownEvent {
            keystroke: Keystroke {
                key: key.into(),
                key_char: key_char.map(str::to_string),
                modifiers,
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    #[test]
    fn normalization_uses_physical_key_and_stable_labels() {
        assert_eq!(
            normalized_physical_key(&key("2", Some("@"), Modifiers::default()).keystroke),
            "2"
        );
        assert_eq!(
            normalized_physical_key(&key("space", Some(" "), Modifiers::default()).keystroke),
            "Space"
        );
        assert_eq!(
            normalized_physical_key(&key("q", Some("a"), Modifiers::default()).keystroke),
            "Q"
        );
    }

    #[test]
    fn matching_requires_all_four_persisted_modifiers_exactly() {
        let shortcut = KeyboardShortcut {
            key: "S".into(),
            modifiers: KeyboardShortcutModifiers {
                shift: true,
                control: true,
                alt: false,
                meta: false,
            },
        };
        assert!(shortcut_matches(
            &shortcut,
            &key(
                "s",
                Some("S"),
                Modifiers {
                    shift: true,
                    control: true,
                    ..Default::default()
                }
            )
        ));
        assert!(!shortcut_matches(
            &shortcut,
            &key(
                "s",
                Some("s"),
                Modifiers {
                    control: true,
                    ..Default::default()
                }
            )
        ));
    }

    #[test]
    fn matching_ignores_layout_dependent_character_output() {
        let shortcut = KeyboardShortcut {
            key: "A".into(),
            modifiers: KeyboardShortcutModifiers::default(),
        };
        assert!(!shortcut_matches(
            &shortcut,
            &key("q", Some("a"), Modifiers::default())
        ));
    }

    #[test]
    fn capture_preempts_actions_and_waits_for_modifier_only_keys() {
        let mut capture = ShortcutCapture::default();
        capture.start("go");
        assert_eq!(
            capture.route(&key(
                "shift",
                None,
                Modifiers {
                    shift: true,
                    ..Default::default()
                }
            )),
            CaptureResult::Pending
        );
        assert_eq!(capture.active_id(), Some("go"));
        assert!(matches!(
            capture.route(&key("enter", None, Modifiers::default())),
            CaptureResult::Captured(_)
        ));
        assert_eq!(capture.active_id(), None);
    }

    #[test]
    fn modal_and_editable_states_suppress_go_and_cue() {
        let settings = crate::settings::AppSettings::default();
        let event = key("space", Some(" "), Modifiers::default());
        assert_eq!(
            route_action(
                &event,
                false,
                InteractionState::default(),
                &settings.keyboard_shortcuts.go,
                &settings.keyboard_shortcuts.cue
            ),
            Some(RoutedAction::Go)
        );
        assert_eq!(
            route_action(
                &event,
                false,
                InteractionState {
                    modal_open: true,
                    editable_focused: false
                },
                &settings.keyboard_shortcuts.go,
                &settings.keyboard_shortcuts.cue
            ),
            None
        );
        assert_eq!(
            route_action(
                &event,
                true,
                InteractionState::default(),
                &settings.keyboard_shortcuts.go,
                &settings.keyboard_shortcuts.cue
            ),
            None
        );
    }

    #[gpui_kit::test]
    fn key_contexts_expose_distinct_gpui_scopes(_: &mut gpui_kit::TestAppContext) {
        assert_ne!(global_key_context(), shortcut_capture_key_context());
    }
}
