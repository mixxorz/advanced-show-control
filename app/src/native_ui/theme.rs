use std::borrow::Cow;

use anyhow::Context as _;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, px, rgb};

pub const CONSOLE_BG: u32 = 0x050506;
pub const CONSOLE_CHROME: u32 = 0x080809;
pub const CONSOLE_PANEL: u32 = 0x0d0d0f;
pub const CONSOLE_SECTION: u32 = 0x141416;
pub const CONSOLE_CONTROL: u32 = 0x18181b;
pub const CONSOLE_CONTROL_HOVER: u32 = 0x202026;
pub const CONSOLE_LINE: u32 = 0x2a2a2e;
pub const CONSOLE_LINE_SOFT: u32 = 0x242428;
pub const CONSOLE_LINE_STRONG: u32 = 0x3a3a40;
pub const CONSOLE_PRIMARY: u32 = 0xdedbd6;
pub const CONSOLE_SECONDARY: u32 = 0xb8b3ab;
pub const CONSOLE_MUTED: u32 = 0x8f8981;
pub const CONSOLE_DISABLED: u32 = 0x56524d;
pub const ACCENT_ORANGE: u32 = 0xff8a00;
pub const ACCENT_ORANGE_HOVER: u32 = 0xff9f2a;
pub const ACCENT_ORANGE_ACTIVE: u32 = 0xc95700;
pub const STATUS_CURRENT: u32 = 0x2c9dff;
pub const STATUS_CUED: u32 = 0x52d62d;
pub const STATUS_WARNING: u32 = 0xf0b429;
pub const STATUS_DANGER: u32 = 0xff5c5c;
pub const STATUS_DANGER_ACTIVE: u32 = 0x9f1f1f;

const FONTS: &[&[u8]] = &[
    include_bytes!("../../assets/fonts/FiraSans-Regular.ttf"),
    include_bytes!("../../assets/fonts/FiraSans-Medium.ttf"),
    include_bytes!("../../assets/fonts/FiraSans-SemiBold.ttf"),
    include_bytes!("../../assets/fonts/FiraSans-Bold.ttf"),
    include_bytes!("../../assets/fonts/FiraSans-ExtraBold.ttf"),
    include_bytes!("../../assets/fonts/FiraSans-Black.ttf"),
    include_bytes!("../../assets/fonts/FiraCode-Variable.ttf"),
];

/// Installs the native equivalent of the existing console design tokens.
pub fn install(cx: &mut App) -> anyhow::Result<()> {
    cx.text_system()
        .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
        .context("failed to register bundled Fira fonts")?;

    Theme::change(ThemeMode::Dark, None, cx);
    {
        let theme = Theme::global_mut(cx);
        theme.font_family = "Fira Sans".into();
        theme.mono_font_family = "Fira Code".into();
        theme.font_size = px(16.0);
        theme.mono_font_size = px(12.0);
        theme.radius = px(2.5);
        theme.radius_lg = px(4.0);
        theme.shadow = false;

        let colors = &mut theme.colors;
        colors.background = rgb(CONSOLE_BG).into();
        colors.foreground = rgb(CONSOLE_PRIMARY).into();
        colors.border = rgb(CONSOLE_LINE).into();
        colors.input = rgb(CONSOLE_LINE_STRONG).into();
        colors.ring = rgb(ACCENT_ORANGE).into();
        colors.muted = rgb(CONSOLE_SECTION).into();
        colors.muted_foreground = rgb(CONSOLE_MUTED).into();
        colors.primary = rgb(ACCENT_ORANGE).into();
        colors.primary_hover = rgb(ACCENT_ORANGE_HOVER).into();
        colors.primary_active = rgb(ACCENT_ORANGE_ACTIVE).into();
        colors.primary_foreground = rgb(CONSOLE_BG).into();
        colors.secondary = rgb(CONSOLE_CONTROL).into();
        colors.secondary_hover = rgb(CONSOLE_CONTROL_HOVER).into();
        colors.secondary_active = rgb(CONSOLE_LINE_STRONG).into();
        colors.secondary_foreground = rgb(CONSOLE_PRIMARY).into();
        colors.button = rgb(CONSOLE_CONTROL).into();
        colors.button_hover = rgb(CONSOLE_CONTROL_HOVER).into();
        colors.button_active = rgb(CONSOLE_LINE_STRONG).into();
        colors.button_foreground = rgb(CONSOLE_PRIMARY).into();
        colors.button_primary = rgb(ACCENT_ORANGE_ACTIVE).into();
        colors.button_primary_hover = rgb(ACCENT_ORANGE).into();
        colors.button_primary_active = rgb(ACCENT_ORANGE_ACTIVE).into();
        colors.button_primary_foreground = rgb(0xffffff).into();
        colors.list = rgb(CONSOLE_PANEL).into();
        colors.list_even = rgb(CONSOLE_PANEL).into();
        colors.list_hover = rgb(CONSOLE_CONTROL_HOVER).into();
        colors.list_active = rgb(CONSOLE_SECTION).into();
        colors.list_active_border = rgb(ACCENT_ORANGE).into();
        colors.tab_bar = rgb(CONSOLE_CHROME).into();
        colors.tab = rgb(CONSOLE_CHROME).into();
        colors.tab_foreground = rgb(CONSOLE_SECONDARY).into();
        colors.tab_active = rgb(CONSOLE_SECTION).into();
        colors.tab_active_foreground = rgb(ACCENT_ORANGE).into();
        colors.title_bar = rgb(CONSOLE_CHROME).into();
        colors.title_bar_border = rgb(CONSOLE_LINE).into();
        colors.scrollbar = rgb(CONSOLE_BG).into();
        colors.scrollbar_thumb = rgb(CONSOLE_LINE_STRONG).into();
        colors.scrollbar_thumb_hover = rgb(CONSOLE_SECONDARY).into();
        colors.success = rgb(STATUS_CUED).into();
        colors.warning = rgb(STATUS_WARNING).into();
        colors.danger = rgb(STATUS_DANGER).into();
        colors.danger_active = rgb(STATUS_DANGER_ACTIVE).into();
        colors.info = rgb(STATUS_CURRENT).into();
        colors.selection = rgb(ACCENT_ORANGE).opacity(0.25).into();
        theme.tokens = (&*colors).into();
    }
    Theme::sync_base(cx);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_palette_matches_the_existing_console_tokens() {
        assert_eq!(CONSOLE_BG, 0x050506);
        assert_eq!(CONSOLE_PRIMARY, 0xdedbd6);
        assert_eq!(ACCENT_ORANGE, 0xff8a00);
        assert_eq!(STATUS_CURRENT, 0x2c9dff);
        assert_eq!(STATUS_CUED, 0x52d62d);
        assert_eq!(STATUS_DANGER, 0xff5c5c);
    }

    #[gpui_kit::test]
    fn bundled_fonts_and_console_theme_install(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            install(cx).expect("bundled fonts and theme should install");

            let theme = Theme::global(cx);
            assert_eq!(theme.font_family.as_ref(), "Fira Sans");
            assert_eq!(theme.mono_font_family.as_ref(), "Fira Code");
            assert_eq!(theme.mode, ThemeMode::Dark);
            assert_eq!(theme.colors.background, rgb(CONSOLE_BG).into());
            assert_eq!(
                theme.colors.button_primary,
                rgb(ACCENT_ORANGE_ACTIVE).into()
            );
            assert_eq!(
                theme.tokens.button_primary.background,
                rgb(ACCENT_ORANGE_ACTIVE).into()
            );
            assert_eq!(theme.colors.button_primary_hover, rgb(ACCENT_ORANGE).into());
            assert_eq!(theme.colors.button_primary_foreground, rgb(0xffffff).into());
        });
    }
}
