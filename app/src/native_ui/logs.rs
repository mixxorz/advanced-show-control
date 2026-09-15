use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
    prelude::FluentBuilder as _, px, rgb,
};

use crate::projector::{AppViewState, LogSeverity};

use super::panel::panel_header;
use super::theme::{
    CONSOLE_LINE, CONSOLE_LINE_SOFT, CONSOLE_MUTED, CONSOLE_PANEL, CONSOLE_PRIMARY, STATUS_DANGER,
    STATUS_WARNING,
};

pub struct LogsView {
    snapshot: AppViewState,
}

impl LogsView {
    pub fn new(snapshot: AppViewState) -> Self {
        Self { snapshot }
    }

    pub fn set_snapshot(
        &mut self,
        snapshot: AppViewState,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for LogsView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let entries = self.snapshot.logs.iter().enumerate().map(|(index, entry)| {
            let severity_color = match entry.severity {
                LogSeverity::Info => rgb(CONSOLE_PRIMARY),
                LogSeverity::Warning => rgb(STATUS_WARNING),
                LogSeverity::Error => rgb(STATUS_DANGER),
            };
            div()
                .flex()
                .gap_3()
                .py_2()
                .border_b_1()
                .border_color(rgb(CONSOLE_LINE_SOFT))
                .font_family("Fira Code")
                .text_sm()
                .child(
                    div()
                        .id(format!("log-timestamp-{index}"))
                        .test_support()
                        .w(px(176.))
                        .flex_shrink_0()
                        .text_color(rgb(CONSOLE_MUTED))
                        .child(entry.timestamp.clone()),
                )
                .child(
                    div()
                        .id(format!("log-severity-{index}"))
                        .test_support()
                        .w(px(88.))
                        .flex_shrink_0()
                        .text_color(severity_color)
                        .child(format!("{:?}", entry.severity).to_uppercase()),
                )
                .child(
                    div()
                        .id(format!("log-message-{index}"))
                        .test_support()
                        .flex_1()
                        .min_w_0()
                        .font_family("Fira Sans")
                        .text_color(rgb(CONSOLE_PRIMARY))
                        .child(entry.message.clone()),
                )
        });

        div()
            .size_full()
            .min_h(px(320.))
            .overflow_hidden()
            .rounded(px(4.))
            .border_1()
            .border_color(rgb(CONSOLE_LINE))
            .bg(rgb(CONSOLE_PANEL))
            .child(panel_header("LOGS"))
            .child(
                div()
                    .id("projected-logs")
                    .size_full()
                    .overflow_y_scroll()
                    .p_4()
                    .when(self.snapshot.logs.is_empty(), |view| {
                        view.child(
                            div()
                                .text_sm()
                                .text_color(rgb(CONSOLE_MUTED))
                                .child("No logs yet."),
                        )
                    })
                    .children(entries),
            )
    }
}
