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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use gpui_kit::component::Root;
    use gpui_kit::component::input::InputEvent;
    use gpui_kit::test::{TestAppContextExt as _, TestWindowExt as _};
    use gpui_kit::{
        AppContext as _, Context, IntoElement, Render, Subscription, prelude::FluentBuilder as _,
        px, size,
    };

    struct NumericControlHarness {
        input: Entity<InputState>,
        committed: Option<String>,
        _subscription: Subscription,
    }

    impl NumericControlHarness {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let input = cx.new(|cx| InputState::new(window, cx).default_value("5"));
            let subscription =
                cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.committed = Some(input.read(cx).value().to_string());
                        cx.notify();
                    }
                });
            Self {
                input,
                committed: None,
                _subscription: subscription,
            }
        }

        fn step(&mut self, direction: i64, window: &mut Window, cx: &mut Context<Self>) {
            let value = self.input.read(cx).value().parse::<i64>().unwrap_or(5);
            let next = (value + direction).clamp(0, 99).to_string();
            self.input
                .update(cx, |input, cx| input.set_value(next.clone(), window, cx));
            self.committed = Some(next);
            cx.notify();
        }
    }

    impl Render for NumericControlHarness {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .child(editable_numeric_control(
                    "test-number",
                    "Test number",
                    &self.input,
                    true,
                    true,
                    cx.listener(|this, _, window, cx| this.step(-1, window, cx)),
                    cx.listener(|this, _, window, cx| this.step(1, window, cx)),
                    |_, _, _| {},
                ))
                .when_some(self.committed.clone(), |view, value| {
                    view.child(
                        div()
                            .id(SharedString::from(format!("committed-{value}")))
                            .test_support()
                            .child(value),
                    )
                })
        }
    }

    #[gpui_kit::test]
    async fn numeric_control_accepts_typing_and_step_buttons(cx: &mut gpui_kit::TestAppContext) {
        cx.update(gpui_kit::init);
        let handle = cx.open_window(size(px(420.), px(180.)), |window, cx| {
            let view = cx.new(|cx| NumericControlHarness::new(window, cx));
            Root::new(view, window, cx)
        });

        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            window.click("test-number-input", cx);
            window.press(
                if cfg!(target_os = "macos") {
                    "cmd-a"
                } else {
                    "ctrl-a"
                },
                cx,
            );
            window.input("42", cx);
            assert_eq!(window.find("test-number-input").value(), Some("42"));
        })
        .unwrap();
        cx.simulate_keystrokes(handle.into(), "enter");
        cx.wait_for(handle.into(), Duration::from_secs(1), |window, _| {
            window.try_find("committed-42").is_some()
        })
        .await;
        cx.update_window(handle.into(), |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("committed-42").is_some());

            window.click("test-number-increase", cx);
            window.render_frame(cx);
            assert!(window.try_find("committed-43").is_some());

            window.click("test-number-decrease", cx);
            window.render_frame(cx);
            assert!(window.try_find("committed-42").is_some());
        })
        .unwrap();
    }
}
