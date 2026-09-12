use gpui_kit::base::Button as BaseButton;
use gpui_kit::{
    Div, IntoElement, ParentElement as _, SharedString, Styled as _, div, px, relative, rgb,
};

use crate::scenes::SceneConfig;

use super::panel::panel_header_with_action;
use super::theme::{
    CONSOLE_LINE, CONSOLE_LINE_SOFT, CONSOLE_MUTED, CONSOLE_PANEL, CONSOLE_PRIMARY,
    CONSOLE_SECONDARY,
};

pub(super) fn scene_library_panel() -> Div {
    div()
        .w(px(350.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .bg(rgb(CONSOLE_PANEL))
        .border_1()
        .border_color(rgb(CONSOLE_LINE))
}

pub(super) fn scene_library_header(action: impl IntoElement) -> Div {
    panel_header_with_action("SCENE LIBRARY", action)
}

pub(super) fn scene_library_columns() -> Div {
    div()
        .w_full()
        .flex()
        .items_center()
        .gap_3()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(CONSOLE_LINE_SOFT))
        .text_xs()
        .text_color(rgb(CONSOLE_SECONDARY))
        .child(div().w(px(42.)).child("#"))
        .child(div().flex_1().child("SCENE NAME"))
        .child(div().w(px(64.)).text_right().child("X-FADE"))
}

pub(super) fn scene_library_row(
    id: impl Into<SharedString>,
    scene: &SceneConfig,
    highlight: Option<u32>,
) -> BaseButton {
    BaseButton::new(id.into())
        .w_full()
        .justify_start()
        .line_height(relative(1.5))
        .gap_3()
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(CONSOLE_LINE_SOFT))
        .child(
            div()
                .w(px(42.))
                .font_family("Fira Code")
                .text_color(rgb(highlight.unwrap_or(CONSOLE_SECONDARY)))
                .child(format_scene_number(scene.scene_index)),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .text_color(rgb(highlight.unwrap_or(CONSOLE_PRIMARY)))
                .child(scene.scene_name.clone()),
        )
        .child(
            div()
                .w(px(64.))
                .text_right()
                .font_family("Fira Code")
                .text_color(rgb(CONSOLE_MUTED))
                .child(format_scene_duration(scene.duration_ms)),
        )
}

pub(super) fn format_scene_number(index: Option<i32>) -> String {
    index.map_or_else(|| "---".to_owned(), |index| format!("{:03}", index + 1))
}

fn format_scene_duration(duration_ms: u64) -> String {
    format!("{:.1}s", duration_ms as f64 / 1_000.0)
}

#[cfg(test)]
mod tests {
    use super::{format_scene_duration, format_scene_number};

    #[test]
    fn scene_library_formats_numbers_and_durations_consistently() {
        assert_eq!(format_scene_number(None), "---");
        assert_eq!(format_scene_number(Some(0)), "001");
        assert_eq!(format_scene_number(Some(11)), "012");
        assert_eq!(format_scene_duration(0), "0.0s");
        assert_eq!(format_scene_duration(1_500), "1.5s");
    }
}
