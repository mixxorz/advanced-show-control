use gpui_kit::component::Disableable as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::{
    AnyElement, App, ClickEvent, Entity, InteractiveElement as _, IntoElement as _, KeyDownEvent,
    ParentElement as _, SharedString, Styled as _, TestSupportExt as _, Window, div, px,
};

use super::button::bordered_button;

#[allow(clippy::too_many_arguments)]
pub(super) fn editable_numeric_control(
    id: &'static str,
    label: &'static str,
    input: &Entity<InputState>,
    can_decrease: bool,
    can_increase: bool,
    decrease: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    increase: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_key_down: impl Fn(&KeyDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .flex()
        .items_center()
        .on_key_down(on_key_down)
        .child(
            bordered_button(SharedString::from(format!("{id}-decrease")))
                .border_r_0()
                .rounded_r_none()
                .label("−")
                .accessibility_label(format!("Decrease {label}"))
                .disabled(!can_decrease)
                .on_click(decrease),
        )
        .child(
            div().w(px(92.)).child(
                Input::new(input)
                    .id(SharedString::from(format!("{id}-input")))
                    .aria_label(label)
                    .border_r_0()
                    .rounded_none(),
            ),
        )
        .child(
            bordered_button(SharedString::from(format!("{id}-increase")))
                .rounded_l_none()
                .label("+")
                .accessibility_label(format!("Increase {label}"))
                .disabled(!can_increase)
                .on_click(increase),
        )
        .into_any_element()
}
