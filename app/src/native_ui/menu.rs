use gpui_kit::component::{
    Disableable as _, IconName,
    button::{Button, ButtonVariants as _},
    menu::DropdownMenu as _,
};
use gpui_kit::{App, FocusHandle, IntoElement, KeyBinding, Styled as _, Window, actions, px, rgb};
#[cfg(target_os = "macos")]
use gpui_kit::{Menu, MenuItem, SystemMenuType};

use super::theme::CONSOLE_LINE;

pub const MENU_NEW_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "cmd-n"
} else {
    "ctrl-n"
};
pub const MENU_OPEN_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "cmd-o"
} else {
    "ctrl-o"
};
pub const MENU_SAVE_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "cmd-s"
} else {
    "ctrl-s"
};
pub const MENU_SAVE_AS_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "cmd-shift-s"
} else {
    "ctrl-shift-s"
};
pub const MENU_QUIT_SHORTCUT: &str = if cfg!(target_os = "macos") {
    "cmd-q"
} else {
    "alt-f4"
};

actions!(
    advanced_show_control,
    [
        About,
        Hide,
        HideOthers,
        NewShow,
        NewShowFromTemplate,
        OpenShow,
        Quit,
        SaveShow,
        SaveShowAs
    ]
);

pub fn install(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(MENU_NEW_SHORTCUT, NewShow, None),
        KeyBinding::new(MENU_OPEN_SHORTCUT, OpenShow, None),
        KeyBinding::new(MENU_SAVE_SHORTCUT, SaveShow, None),
        KeyBinding::new(MENU_SAVE_AS_SHORTCUT, SaveShowAs, None),
        KeyBinding::new(MENU_QUIT_SHORTCUT, Quit, None),
    ]);

    #[cfg(target_os = "macos")]
    {
        cx.bind_keys([
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
        ]);
        cx.set_menus([application_menu()]);
    }
}

pub fn session_menu_button(
    action_context: FocusHandle,
    disabled: bool,
    on_open_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    Button::new("session-menu")
        .ghost()
        .accessibility_label("Session menu")
        .icon(IconName::Menu)
        .size(px(50.))
        .border_0()
        .border_r_1()
        .border_color(rgb(CONSOLE_LINE))
        .rounded_none()
        .disabled(disabled)
        .dropdown_menu(move |menu, _, _| {
            menu.action_context(action_context.clone())
                .menu("New Session", Box::new(NewShow))
                .menu("New from Template…", Box::new(NewShowFromTemplate))
                .menu("Open Session…", Box::new(OpenShow))
                .separator()
                .menu("Save Session", Box::new(SaveShow))
                .menu("Save Session As…", Box::new(SaveShowAs))
                .separator()
                .menu("Quit", Box::new(Quit))
        })
        .on_open_change(on_open_change)
}

#[cfg(target_os = "macos")]
fn application_menu() -> Menu {
    Menu::new("Advanced Show Control").items([
        MenuItem::action("About Advanced Show Control", About),
        MenuItem::separator(),
        MenuItem::os_submenu("Services", SystemMenuType::Services),
        MenuItem::separator(),
        MenuItem::action("Hide Advanced Show Control", Hide),
        MenuItem::action("Hide Others", HideOthers),
        MenuItem::separator(),
        MenuItem::action("Quit Advanced Show Control", Quit),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_shortcuts_use_native_primary_modifier() {
        let expected = if cfg!(target_os = "macos") {
            "cmd-s"
        } else {
            "ctrl-s"
        };
        assert_eq!(MENU_SAVE_SHORTCUT, expected);
    }

    #[gpui_kit::test]
    #[allow(clippy::needless_option_as_deref)]
    async fn session_menu_exposes_actions_and_dispatches_selection(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use std::time::Duration;

        use gpui_kit::component::Root;
        use gpui_kit::test::{TestAppContextExt as _, TestSupportExt as _, TestWindowExt as _};
        use gpui_kit::{
            AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
            Render, Window, div, prelude::FluentBuilder as _, px, size,
        };

        struct SessionMenuHarness {
            selected: bool,
            focus: gpui_kit::FocusHandle,
        }

        impl Render for SessionMenuHarness {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div()
                    .id("session-menu-harness")
                    .test_support()
                    .track_focus(&self.focus)
                    .on_action(cx.listener(|this, _: &NewShow, _, cx| {
                        this.selected = true;
                        cx.notify();
                    }))
                    .child(session_menu_button(self.focus.clone(), false, |_, _, _| {}))
                    .when(self.selected, |element| {
                        element.child(div().id("new-selected").test_support())
                    })
            }
        }

        cx.update(gpui_kit::init);
        let handle = cx.open_window(size(px(640.), px(480.)), |window, cx| {
            let view = cx.new(|cx| SessionMenuHarness {
                selected: false,
                focus: cx.focus_handle(),
            });
            view.read(cx).focus.clone().focus(window, cx);
            Root::new(view, window, cx)
        });
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("session-menu", cx);
            assert_eq!(
                window.within("popup-menu").find(0usize).label().as_deref(),
                Some("New Session")
            );
            assert_eq!(
                window.within("popup-menu").find(1usize).label().as_deref(),
                Some("New from Template…")
            );
            assert_eq!(
                window.within("popup-menu").find(2usize).label().as_deref(),
                Some("Open Session…")
            );
            assert_eq!(
                window.within("popup-menu").find(4usize).label().as_deref(),
                Some("Save Session")
            );
            assert_eq!(
                window.within("popup-menu").find(5usize).label().as_deref(),
                Some("Save Session As…")
            );
            assert_eq!(
                window.within("popup-menu").find(7usize).label().as_deref(),
                Some("Quit")
            );
            window.within("popup-menu").click(0usize, cx);
        })
        .unwrap();
        cx.wait_for(handle.into(), Duration::from_secs(1), |window, _| {
            window.try_find("new-selected").is_some()
        })
        .await;
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_application_menu_has_standard_items() {
        let menu = application_menu();
        assert_eq!(menu.name.as_ref(), "Advanced Show Control");
        assert_eq!(menu.items.len(), 8);
        assert!(matches!(
            &menu.items[2],
            MenuItem::SystemMenu(os_menu)
                if os_menu.name.as_ref() == "Services"
                    && os_menu.menu_type == SystemMenuType::Services
        ));
        let action_names = menu
            .items
            .iter()
            .filter_map(|item| match item {
                MenuItem::Action { name, .. } => Some(name.as_ref()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            action_names,
            [
                "About Advanced Show Control",
                "Hide Advanced Show Control",
                "Hide Others",
                "Quit Advanced Show Control",
            ]
        );
    }
}
