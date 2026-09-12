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
    [NewShow, OpenShow, SaveShow, SaveShowAs]
);

pub fn install(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(MENU_NEW_SHORTCUT, NewShow, None),
        KeyBinding::new(MENU_OPEN_SHORTCUT, OpenShow, None),
        KeyBinding::new(MENU_SAVE_SHORTCUT, SaveShow, None),
        KeyBinding::new(MENU_SAVE_AS_SHORTCUT, SaveShowAs, None),
    ]);

    cx.set_menus([Menu::new("File").items([
        MenuItem::action("New Session", NewShow),
        MenuItem::action("Open Session…", OpenShow),
        MenuItem::separator(),
        MenuItem::action("Save Session", SaveShow),
        MenuItem::action("Save Session As…", SaveShowAs),
    ])]);
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
}
