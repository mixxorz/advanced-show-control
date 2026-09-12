use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, prelude::FluentBuilder as _, px,
    rgb,
};

use crate::projector::{AppViewState, LogSeverity};

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
        let entries = self.snapshot.logs.iter().map(|entry| {
            let severity_color = match entry.severity {
                LogSeverity::Info => rgb(CONSOLE_PRIMARY),
                LogSeverity::Warning => rgb(STATUS_WARNING),
                LogSeverity::Error => rgb(STATUS_DANGER),
            };
            div()
                .grid()
                .grid_cols(3)
                .gap_3()
                .py_2()
                .border_b_1()
                .border_color(rgb(CONSOLE_LINE_SOFT))
                .font_family("Fira Code")
                .text_sm()
                .child(
                    div()
                        .text_color(rgb(CONSOLE_MUTED))
                        .child(entry.timestamp.clone()),
                )
                .child(
                    div()
                        .text_color(severity_color)
                        .child(format!("{:?}", entry.severity).to_uppercase()),
                )
                .child(
                    div()
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
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(CONSOLE_LINE))
                    .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                    .child("LOGS"),
            )
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
                                .child("No frontend logs yet."),
                        )
                    })
                    .children(entries),
            )
    }
}
