use gpui_kit::component::button::Button;
use gpui_kit::{ElementId, Styled as _};

pub(super) fn bordered_button(id: impl Into<ElementId>) -> Button {
    Button::new(id).border_1()
}
