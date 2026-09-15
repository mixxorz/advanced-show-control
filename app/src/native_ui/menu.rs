#[cfg(target_os = "macos")]
use gpui_kit::SystemMenuType;
use gpui_kit::{App, KeyBinding, Menu, MenuItem, actions};

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
    ]);

    #[cfg(target_os = "macos")]
    {
        cx.bind_keys([
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
            KeyBinding::new("cmd-q", Quit, None),
        ]);
        cx.set_menus([application_menu(), file_menu()]);
    }

    #[cfg(not(target_os = "macos"))]
    cx.set_menus([file_menu()]);
}

fn file_menu() -> Menu {
    Menu::new("File").items([
        MenuItem::action("New Session", NewShow),
        MenuItem::action("New from Template…", NewShowFromTemplate),
        MenuItem::action("Open Session…", OpenShow),
        MenuItem::separator(),
        MenuItem::action("Save Session", SaveShow),
        MenuItem::action("Save Session As…", SaveShowAs),
    ])
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

    #[test]
    fn file_menu_exposes_new_from_template() {
        let menu = file_menu();
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
                "New Session",
                "New from Template…",
                "Open Session…",
                "Save Session",
                "Save Session As…",
            ]
        );
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
