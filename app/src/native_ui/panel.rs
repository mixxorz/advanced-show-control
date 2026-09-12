use gpui_kit::{Div, IntoElement, ParentElement as _, Styled as _, div, rgb};

use super::theme::{CONSOLE_LINE, CONSOLE_PRIMARY};

pub(super) fn panel_header(title: &'static str) -> Div {
    div()
        .flex_shrink_0()
        .flex()
        .items_center()
        .border_b_1()
        .border_color(rgb(CONSOLE_LINE))
        .px_4()
        .py_3()
        .text_lg()
        .text_color(rgb(CONSOLE_PRIMARY))
        .child(title)
}

pub(super) fn panel_header_with_action(title: &'static str, action: impl IntoElement) -> Div {
    panel_header(title).justify_between().child(action)
}
