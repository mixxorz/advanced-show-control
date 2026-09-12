use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Disableable as _, Selectable as _, Sizable as _};
use gpui_kit::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div,
    prelude::FluentBuilder as _, px, rgb,
};
use uuid::Uuid;

use crate::native_ui::CommandDispatcher;
use crate::native_ui::theme::{
    ACCENT_ORANGE, CONSOLE_BG, CONSOLE_LINE, CONSOLE_LINE_SOFT, CONSOLE_MUTED, CONSOLE_PANEL,
    CONSOLE_PRIMARY, CONSOLE_SECONDARY, CONSOLE_SECTION, STATUS_CUED, STATUS_CURRENT,
    STATUS_WARNING,
};
use crate::projector::{AppViewState, SceneSummary};
use crate::scenes::{ChannelConfig, SceneConfig};

const GROUP_ORDER: [&str; 7] = [
    "Inputs",
    "Groups",
    "Aux",
    "Masters",
    "Matrix",
    "Link/DCAs",
    "Unknown",
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingOverwrite {
    source_id: Uuid,
    target_index: i32,
    target_name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SceneRowState {
    current: bool,
    cued: bool,
    selected: bool,
    unlinked: bool,
}

pub struct ScenesView {
    snapshot: AppViewState,
    dispatcher: CommandDispatcher,
    duration_input: Entity<InputState>,
    duration_identity: Option<(Uuid, u64)>,
    selected_link_target: Option<i32>,
    pending_overwrite: Option<PendingOverwrite>,
    _duration_subscription: Subscription,
}

impl ScenesView {
    pub fn new(
        snapshot: AppViewState,
        dispatcher: CommandDispatcher,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let duration_identity =
            selected_scene(&snapshot).map(|scene| (scene.internal_scene_id, scene.duration_ms));
        let duration = duration_identity.map_or(0, |(_, duration)| duration);
        let duration_input =
            cx.new(|cx| InputState::new(window, cx).default_value(format_duration(duration)));
        let subscription = cx.subscribe_in(
            &duration_input,
            window,
            |this, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } | InputEvent::Blur => {
                    this.commit_duration(window, cx);
                }
                _ => {}
            },
        );
        let selected_link_target = default_link_target(&snapshot);

        Self {
            snapshot,
            dispatcher,
            duration_input,
            duration_identity,
            selected_link_target,
            pending_overwrite: None,
            _duration_subscription: subscription,
        }
    }

    pub fn set_snapshot(
        &mut self,
        snapshot: AppViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_scene_id = self.duration_identity.map(|(id, _)| id);
        let next_duration_identity =
            selected_scene(&snapshot).map(|scene| (scene.internal_scene_id, scene.duration_ms));
        if next_duration_identity != self.duration_identity {
            let duration = next_duration_identity.map_or(0, |(_, duration)| duration);
            self.duration_input.update(cx, |input, cx| {
                input.set_value(format_duration(duration), window, cx)
            });
            self.duration_identity = next_duration_identity;
        }

        let next_scene_id = next_duration_identity.map(|(id, _)| id);
        if previous_scene_id != next_scene_id
            || !snapshot
                .scenes
                .iter()
                .any(|scene| Some(scene.index) == self.selected_link_target)
        {
            self.selected_link_target = default_link_target(&snapshot);
        }
        self.pending_overwrite = self.pending_overwrite.take().filter(|pending| {
            selected_scene(&snapshot)
                .is_some_and(|scene| scene.internal_scene_id == pending.source_id)
                && snapshot.scenes.iter().any(|scene| {
                    scene.index == pending.target_index && scene.name == pending.target_name
                })
        });
        self.snapshot = snapshot;
        cx.notify();
    }

    fn dispatch(
        &self,
        command: impl FnOnce(
            crate::application::ApplicationCommandContext,
        )
            -> std::pin::Pin<Box<dyn Future<Output = Result<(), String>> + Send>>
        + Send
        + 'static,
    ) {
        self.dispatcher.dispatch(command);
    }

    fn commit_duration(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(scene) = selected_scene(&self.snapshot) else {
            return;
        };
        let projected = scene.duration_ms;
        let scene_id = scene.internal_scene_id;
        let draft = self.duration_input.read(cx).value().to_string();
        let Some(duration_ms) = normalize_duration(&draft) else {
            self.reset_duration(projected, window, cx);
            return;
        };
        if duration_ms == projected {
            self.reset_duration(projected, window, cx);
            return;
        }
        self.dispatch(move |commands| {
            Box::pin(async move {
                commands
                    .set_scene_duration_ms(scene_id, duration_ms)
                    .await
                    .map(|_| ())
            })
        });
        self.reset_duration(projected, window, cx);
    }

    fn reset_duration(&self, duration_ms: u64, window: &mut Window, cx: &mut Context<Self>) {
        self.duration_input.update(cx, |input, cx| {
            input.set_value(format_duration(duration_ms), window, cx)
        });
    }

    fn select_scene(&self, scene_id: Uuid) {
        self.dispatch(move |commands| {
            Box::pin(async move { commands.select_scene_config(scene_id).await.map(|_| ()) })
        });
    }

    fn render_scene_list(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let duplicate_names = duplicate_scene_names(&self.snapshot.scene_configs);
        let cued_id = cued_scene_id(&self.snapshot);
        let selected_id = selected_scene(&self.snapshot).map(|scene| scene.internal_scene_id);

        div()
            .w(px(368.))
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(CONSOLE_PANEL))
            .border_1()
            .border_color(rgb(CONSOLE_LINE))
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(CONSOLE_LINE))
                    .text_lg()
                    .text_color(rgb(CONSOLE_PRIMARY))
                    .child("SCENE LIBRARY"),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(3)
                    .px_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(CONSOLE_LINE_SOFT))
                    .text_sm()
                    .text_color(rgb(CONSOLE_SECONDARY))
                    .child("#")
                    .child("SCENE NAME")
                    .child("X-FADE"),
            )
            .when(!duplicate_names.is_empty(), |panel| {
                panel.child(
                    div()
                        .px_3()
                        .py_2()
                        .bg(rgb(CONSOLE_SECTION))
                        .border_b_1()
                        .border_color(rgb(STATUS_WARNING))
                        .text_color(rgb(STATUS_WARNING))
                        .child(format!(
                            "Duplicate scene names: {}",
                            duplicate_names.join(", ")
                        )),
                )
            })
            .child(
                div()
                    .id("scene-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(self.snapshot.scene_configs.is_empty(), |list| {
                        list.child(
                            div()
                                .p_4()
                                .text_color(rgb(CONSOLE_MUTED))
                                .child("No scenes loaded."),
                        )
                    })
                    .children(self.snapshot.scene_configs.iter().map(|scene| {
                        let scene_id = scene.internal_scene_id;
                        let state = row_state(
                            scene,
                            self.snapshot.current_scene.as_ref(),
                            cued_id,
                            selected_id,
                        );
                        let color = row_color(state);
                        Button::new(SharedString::from(format!("scene-row-{scene_id}")))
                            .accessibility_label(format!(
                                "Select scene {} {}",
                                format_scene_number(scene.scene_index),
                                scene.scene_name
                            ))
                            .child(
                                div()
                                    .w_full()
                                    .grid()
                                    .grid_cols(3)
                                    .gap_2()
                                    .text_color(rgb(color))
                                    .child(format_scene_number(scene.scene_index))
                                    .child(scene.scene_name.clone())
                                    .child(format_duration(scene.duration_ms)),
                            )
                            .selected(state.selected)
                            .on_click(cx.listener(move |this, _, _, _| {
                                this.select_scene(scene_id);
                            }))
                    })),
            )
    }

    fn render_editor(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(scene) = selected_scene(&self.snapshot) else {
            return div()
                .flex_1()
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgb(CONSOLE_PANEL))
                .border_1()
                .border_color(rgb(CONSOLE_LINE))
                .text_color(rgb(CONSOLE_MUTED))
                .child("Select a scene to edit its fade settings.");
        };

        let scene_id = scene.internal_scene_id;
        let unlinked = scene.scene_index.is_none();
        let cued = cued_scene_id(&self.snapshot) == Some(scene_id);
        let current = is_current(scene, self.snapshot.current_scene.as_ref());
        let identity_color = if unlinked {
            STATUS_WARNING
        } else if current {
            STATUS_CURRENT
        } else if cued {
            STATUS_CUED
        } else {
            CONSOLE_PRIMARY
        };

        div()
            .flex_1()
            .h_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .px_5()
                    .py_3()
                    .bg(rgb(CONSOLE_BG))
                    .border_1()
                    .border_color(rgb(CONSOLE_LINE))
                    .text_xl()
                    .text_color(rgb(identity_color))
                    .child(format!(
                        "{}  {}",
                        format_scene_number(scene.scene_index),
                        scene.scene_name
                    )),
            )
            .child(self.render_actions(scene, cx))
            .when(unlinked, |editor| {
                editor.child(self.render_link_controls(scene, cx))
            })
            .child(self.render_scope(scene, cx))
            .when_some(self.pending_overwrite.as_ref(), |editor, pending| {
                editor.child(self.render_overwrite(pending, cx))
            })
    }

    fn render_actions(&self, scene: &SceneConfig, cx: &mut Context<Self>) -> impl IntoElement {
        let scene_id = scene.internal_scene_id;
        let unlinked = scene.scene_index.is_none();
        let clipboard = self.snapshot.scene_settings_clipboard_available;

        div()
            .flex()
            .items_end()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("scene-recall")
                            .label("Recall")
                            .primary()
                            .disabled(unlinked)
                            .on_click(cx.listener(move |this, _, _, _| {
                                this.dispatch(move |commands| {
                                    Box::pin(async move {
                                        commands.recall_scene(scene_id).await.map(|_| ())
                                    })
                                });
                            })),
                    )
                    .child(action_button(
                        "scene-store",
                        "Store",
                        unlinked,
                        cx,
                        move |this| {
                            this.dispatch(move |commands| {
                                Box::pin(async move {
                                    commands.store_scene_config(scene_id).await.map(|_| ())
                                })
                            });
                        },
                    ))
                    .child(action_button(
                        "scene-copy",
                        "Copy",
                        false,
                        cx,
                        move |this| {
                            this.dispatch(move |commands| {
                                Box::pin(async move {
                                    commands.copy_scene_settings(scene_id).await.map(|_| ())
                                })
                            });
                        },
                    ))
                    .child(action_button(
                        "scene-paste",
                        "Paste",
                        unlinked || !clipboard,
                        cx,
                        move |this| {
                            this.dispatch(move |commands| {
                                Box::pin(async move {
                                    commands.paste_scene_settings(scene_id).await.map(|_| ())
                                })
                            });
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(CONSOLE_SECONDARY))
                    .child("X-FADE")
                    .child(div().w(px(96.)).child(Input::new(&self.duration_input)))
                    .child(self.duration_step_button(1, cx))
                    .child(self.duration_step_button(-1, cx)),
            )
    }

    fn duration_step_button(&self, direction: i64, cx: &mut Context<Self>) -> Button {
        let label = if direction > 0 { "+1s" } else { "−1s" };
        Button::new(if direction > 0 {
            "duration-step-up"
        } else {
            "duration-step-down"
        })
        .label(label)
        .small()
        .on_click(cx.listener(move |this, _, window, cx| {
            let Some(scene) = selected_scene(&this.snapshot) else {
                return;
            };
            let next = step_duration(scene.duration_ms, direction);
            this.reset_duration(next, window, cx);
            this.commit_duration(window, cx);
        }))
    }

    fn render_link_controls(
        &self,
        scene: &SceneConfig,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let scene_id = scene.internal_scene_id;
        div()
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .bg(rgb(CONSOLE_SECTION))
            .border_1()
            .border_color(rgb(STATUS_WARNING))
            .child(
                div()
                    .text_color(rgb(STATUS_WARNING))
                    .child("Scene is currently unlinked"),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .children(self.snapshot.scenes.iter().map(|target| {
                        let index = target.index;
                        Button::new(("link-target", index as u64))
                            .label(format!(
                                "{} {}",
                                format_scene_number(Some(index)),
                                target.name
                            ))
                            .small()
                            .selected(self.selected_link_target == Some(index))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_link_target = Some(index);
                                cx.notify();
                            }))
                    })),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("link-scene")
                            .label("Link to scene")
                            .primary()
                            .small()
                            .disabled(self.selected_link_target.is_none())
                            .on_click(cx.listener(|this, _, _, cx| this.request_link(cx))),
                    )
                    .child(
                        Button::new("delete-unlinked-scene")
                            .label("Delete")
                            .danger()
                            .small()
                            .on_click(cx.listener(move |this, _, _, _| {
                                this.dispatch(move |commands| {
                                    Box::pin(async move {
                                        commands.delete_scene_config(scene_id).await.map(|_| ())
                                    })
                                });
                            })),
                    ),
            )
    }

    fn request_link(&mut self, cx: &mut Context<Self>) {
        let Some(source) = selected_scene(&self.snapshot) else {
            return;
        };
        let Some(target) = self
            .snapshot
            .scenes
            .iter()
            .find(|scene| Some(scene.index) == self.selected_link_target)
        else {
            return;
        };
        let source_id = source.internal_scene_id;
        let target_index = target.index;
        if self
            .snapshot
            .scene_configs
            .iter()
            .any(|config| config.scene_index == Some(target_index))
        {
            self.pending_overwrite = Some(PendingOverwrite {
                source_id,
                target_index,
                target_name: target.name.clone(),
            });
            cx.notify();
            return;
        }
        self.dispatch_link(source_id, target_index, false);
    }

    fn dispatch_link(&self, source_id: Uuid, target_index: i32, overwrite: bool) {
        self.dispatch(move |commands| {
            Box::pin(async move {
                commands
                    .link_scene_config(source_id, target_index, overwrite)
                    .await
                    .map(|_| ())
            })
        });
    }

    fn render_overwrite(
        &self,
        pending: &PendingOverwrite,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let source_name = selected_scene(&self.snapshot)
            .map(|scene| scene.scene_name.as_str())
            .unwrap_or("Unknown");
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(0x000000).opacity(0.78))
            .child(
                div()
                    .w(px(480.))
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .bg(rgb(CONSOLE_PANEL))
                    .border_1()
                    .border_color(rgb(CONSOLE_LINE))
                    .child(
                        div()
                            .text_lg()
                            .text_color(rgb(CONSOLE_PRIMARY))
                            .child("OVERWRITE EXISTING FADE SETTINGS?"),
                    )
                    .child(
                        div().text_color(rgb(CONSOLE_SECONDARY)).child(format!(
                            "{} {} already has fade settings. Continue to replace them with the fade settings from {}?",
                            format_scene_number(Some(pending.target_index)),
                            pending.target_name,
                            source_name
                        )),
                    )
                    .child(
                        div().text_color(rgb(CONSOLE_SECONDARY)).child(
                            "Only Advanced Show Control fade settings are replaced. The actual LV1 scene is not changed.",
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                Button::new("cancel-overwrite")
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.pending_overwrite = None;
                                        cx.notify();
                                    })),
                            )
                            .child(
                                Button::new("confirm-overwrite")
                                    .label("Overwrite")
                                    .danger()
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.confirm_overwrite(cx);
                                    })),
                            ),
                    ),
            )
    }

    fn confirm_overwrite(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_overwrite.take() else {
            return;
        };
        let exact_target_exists =
            self.snapshot.scenes.iter().any(|scene| {
                scene.index == pending.target_index && scene.name == pending.target_name
            });
        let exact_source_exists = selected_scene(&self.snapshot)
            .is_some_and(|scene| scene.internal_scene_id == pending.source_id);
        if exact_target_exists && exact_source_exists {
            let conflict = self
                .snapshot
                .scene_configs
                .iter()
                .any(|scene| scene.scene_index == Some(pending.target_index));
            self.dispatch_link(pending.source_id, pending.target_index, conflict);
        }
        cx.notify();
    }

    fn render_scope(&self, scene: &SceneConfig, cx: &mut Context<Self>) -> impl IntoElement {
        if scene.channel_configs.is_empty() {
            return div()
                .flex_1()
                .p_6()
                .bg(rgb(CONSOLE_PANEL))
                .border_1()
                .border_color(rgb(CONSOLE_LINE))
                .text_color(rgb(CONSOLE_MUTED))
                .child(
                    "Store this scene from the current LV1 state before selecting channel scope.",
                );
        }

        let scene_id = scene.internal_scene_id;
        let scoped: BTreeSet<(i32, i32)> = scene
            .scoped_channels
            .iter()
            .map(|channel| (channel.group, channel.channel))
            .collect();
        let all_scoped = scene
            .channel_configs
            .iter()
            .all(|channel| scoped.contains(&(channel.group, channel.channel)));
        let none_scoped = scoped.is_empty();
        let groups = grouped_channels(&scene.channel_configs);
        let faders = scene.scope_toggles.faders;
        let pan = scene.scope_toggles.pan;

        div()
            .flex_1()
            .min_h_0()
            .p_4()
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(CONSOLE_PANEL))
            .border_1()
            .border_color(rgb(CONSOLE_LINE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(rgb(CONSOLE_LINE))
                    .pb_3()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_lg()
                            .text_color(rgb(CONSOLE_PRIMARY))
                            .child("SCOPE")
                            .child(scope_toggle(
                                "scope-faders",
                                "Faders",
                                faders,
                                cx,
                                move |this| {
                                    this.dispatch(move |commands| {
                                        Box::pin(async move {
                                            commands
                                                .set_scene_scope_faders_enabled(scene_id, !faders)
                                                .await
                                                .map(|_| ())
                                        })
                                    });
                                },
                            ))
                            .child(scope_toggle("scope-pan", "Pan", pan, cx, move |this| {
                                this.dispatch(move |commands| {
                                    Box::pin(async move {
                                        commands
                                            .set_scene_scope_pan_enabled(scene_id, !pan)
                                            .await
                                            .map(|_| ())
                                    })
                                });
                            })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(bulk_scope_button(
                                "scope-all",
                                "All",
                                all_scoped,
                                cx,
                                move |this| {
                                    this.dispatch(move |commands| {
                                        Box::pin(async move {
                                            commands
                                                .set_all_channels_scoped(scene_id, true)
                                                .await
                                                .map(|_| ())
                                        })
                                    });
                                },
                            ))
                            .child(bulk_scope_button(
                                "scope-none",
                                "None",
                                none_scoped,
                                cx,
                                move |this| {
                                    this.dispatch(move |commands| {
                                        Box::pin(async move {
                                            commands
                                                .set_all_channels_scoped(scene_id, false)
                                                .await
                                                .map(|_| ())
                                        })
                                    });
                                },
                            )),
                    ),
            )
            .child(
                div()
                    .id("channel-scope-grid")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .children(groups.into_iter().map(|(group_name, channels)| {
                        div()
                            .mb_3()
                            .child(
                                div()
                                    .mb_2()
                                    .text_color(rgb(CONSOLE_SECONDARY))
                                    .child(group_name),
                            )
                            .child(div().flex().flex_wrap().gap_2().children(
                                channels.into_iter().map(|channel| {
                                    let active = scoped.contains(&(channel.group, channel.channel));
                                    let group = channel.group;
                                    let channel_number = channel.channel;
                                    Button::new(SharedString::from(format!(
                                        "scope-channel-{group}-{channel_number}"
                                    )))
                                    .label(channel_label(group, channel_number))
                                    .tooltip(channel_tooltip(channel, &self.snapshot))
                                    .small()
                                    .selected(active)
                                    .on_click(cx.listener(move |this, _, _, _| {
                                        this.dispatch(move |commands| {
                                            Box::pin(async move {
                                                commands
                                                    .set_channel_scoped(
                                                        scene_id,
                                                        group,
                                                        channel_number,
                                                        !active,
                                                    )
                                                    .await
                                                    .map(|_| ())
                                            })
                                        });
                                    }))
                                }),
                            ))
                    })),
            )
    }
}

impl Render for ScenesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .p_3()
            .flex()
            .gap_3()
            .bg(rgb(CONSOLE_BG))
            .text_color(rgb(CONSOLE_PRIMARY))
            .child(self.render_scene_list(cx))
            .child(self.render_editor(cx))
    }
}

fn action_button(
    id: &'static str,
    label: &'static str,
    disabled: bool,
    cx: &mut Context<ScenesView>,
    handler: impl Fn(&ScenesView) + 'static,
) -> Button {
    Button::new(id)
        .label(label)
        .secondary()
        .disabled(disabled)
        .on_click(cx.listener(move |this, _, _, _| handler(this)))
}

fn scope_toggle(
    id: &'static str,
    label: &'static str,
    active: bool,
    cx: &mut Context<ScenesView>,
    handler: impl Fn(&ScenesView) + 'static,
) -> Button {
    Button::new(id)
        .label(label)
        .small()
        .selected(active)
        .on_click(cx.listener(move |this, _, _, _| handler(this)))
}

fn bulk_scope_button(
    id: &'static str,
    label: &'static str,
    active: bool,
    cx: &mut Context<ScenesView>,
    handler: impl Fn(&ScenesView) + 'static,
) -> Button {
    scope_toggle(id, label, active, cx, handler)
}

fn selected_scene(snapshot: &AppViewState) -> Option<&SceneConfig> {
    let selected = snapshot.selected_scene_internal_id.as_deref()?;
    snapshot
        .scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id.to_string() == selected)
}

fn cued_scene_id(snapshot: &AppViewState) -> Option<Uuid> {
    let active_list_id = snapshot.active_cue_list_id.as_deref()?;
    let cued_entry_id = snapshot.cued_cue_entry_id.as_deref()?;
    snapshot
        .cue_lists
        .iter()
        .find(|list| list.id.to_string() == active_list_id)?
        .entries
        .iter()
        .find(|entry| entry.id.to_string() == cued_entry_id)
        .map(|entry| entry.scene_internal_id)
}

fn is_current(scene: &SceneConfig, current: Option<&SceneSummary>) -> bool {
    current.is_some_and(|current| {
        Some(current.index) == scene.scene_index && current.name == scene.scene_name
    })
}

fn row_state(
    scene: &SceneConfig,
    current: Option<&SceneSummary>,
    cued_id: Option<Uuid>,
    selected_id: Option<Uuid>,
) -> SceneRowState {
    SceneRowState {
        current: is_current(scene, current),
        cued: cued_id == Some(scene.internal_scene_id),
        selected: selected_id == Some(scene.internal_scene_id),
        unlinked: scene.scene_index.is_none(),
    }
}

fn row_color(state: SceneRowState) -> u32 {
    if state.unlinked {
        STATUS_WARNING
    } else if state.current {
        STATUS_CURRENT
    } else if state.cued {
        STATUS_CUED
    } else if state.selected {
        ACCENT_ORANGE
    } else {
        CONSOLE_SECONDARY
    }
}

fn duplicate_scene_names(scenes: &[SceneConfig]) -> Vec<String> {
    let mut counts = BTreeMap::new();
    for scene in scenes {
        *counts.entry(scene.scene_name.as_str()).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| name.to_owned())
        .collect()
}

fn default_link_target(snapshot: &AppViewState) -> Option<i32> {
    snapshot
        .scenes
        .iter()
        .find(|scene| {
            !snapshot
                .scene_configs
                .iter()
                .any(|config| config.scene_index == Some(scene.index))
        })
        .or_else(|| snapshot.scenes.first())
        .map(|scene| scene.index)
}

fn normalize_duration(draft: &str) -> Option<u64> {
    let trimmed = draft.trim().trim_end_matches(['s', 'S']);
    if trimmed.is_empty() {
        return None;
    }
    let seconds: f64 = trimmed.parse().ok()?;
    if !seconds.is_finite() || seconds < 0.0 {
        return None;
    }
    if seconds == 0.0 {
        return Some(0);
    }
    Some((seconds.clamp(0.1, 120.0) * 1000.0).round() as u64)
}

fn step_duration(duration_ms: u64, direction: i64) -> u64 {
    let next = (duration_ms as i128 + direction as i128 * 1000).max(0) as f64 / 1000.0;
    if next == 0.0 {
        0
    } else {
        (next.clamp(0.1, 120.0) * 1000.0).round() as u64
    }
}

fn format_scene_number(index: Option<i32>) -> String {
    index.map_or_else(|| "---".to_owned(), |index| format!("{:03}", index + 1))
}

fn format_duration(duration_ms: u64) -> String {
    format!("{:.1}s", duration_ms as f64 / 1000.0)
}

fn channel_group(group: i32) -> &'static str {
    match group {
        0 => "Inputs",
        1 => "Groups",
        2 => "Aux",
        6 => "Matrix",
        12 => "Link/DCAs",
        3 | 4 | 5 | 7 | 8 => "Masters",
        _ => "Unknown",
    }
}

fn grouped_channels(channels: &[ChannelConfig]) -> Vec<(&'static str, Vec<&ChannelConfig>)> {
    GROUP_ORDER
        .into_iter()
        .filter_map(|name| {
            let matching: Vec<_> = channels
                .iter()
                .filter(|channel| channel_group(channel.group) == name)
                .collect();
            (!matching.is_empty()).then_some((name, matching))
        })
        .collect()
}

fn channel_label(group: i32, channel: i32) -> SharedString {
    match group {
        3 => "LR".into(),
        4 => "C".into(),
        5 => "M".into(),
        7 => "Cue".into(),
        8 => "TB".into(),
        _ => (channel + 1).to_string().into(),
    }
}

fn channel_tooltip(channel: &ChannelConfig, snapshot: &AppViewState) -> String {
    let name = snapshot
        .channels
        .iter()
        .find(|entry| entry.group == channel.group && entry.channel == channel.channel)
        .map(|entry| entry.name.as_str())
        .unwrap_or("Unknown");
    let mut pan = Vec::new();
    if let Some(value) = channel.pan {
        pan.push(format!("pan {value:.1}"));
    }
    if let Some(value) = channel.balance {
        pan.push(format!("balance {value:.1}"));
    }
    if let Some(value) = channel.width {
        pan.push(format!("width {value:.1}"));
    }
    let pan = if pan.is_empty() {
        "No pan values".to_owned()
    } else {
        pan.join(" · ")
    };
    format!(
        "{} · {:.1} dB · {}",
        name,
        channel.fader_db.unwrap_or(0.0),
        pan
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenes::{ChannelRef, SceneScopeToggles};

    fn scene(id: Uuid, index: Option<i32>, name: &str) -> SceneConfig {
        SceneConfig {
            internal_scene_id: id,
            scene_index: index,
            scene_name: name.to_owned(),
            duration_ms: 1000,
            channel_configs: vec![],
            scoped_channels: vec![],
            scope_toggles: SceneScopeToggles::default(),
        }
    }

    #[test]
    fn duration_normalization_matches_scene_editor_contract() {
        assert_eq!(normalize_duration("0s"), Some(0));
        assert_eq!(normalize_duration(".01"), Some(100));
        assert_eq!(normalize_duration("1.2346s"), Some(1235));
        assert_eq!(normalize_duration("999"), Some(120_000));
        assert_eq!(normalize_duration("-1"), None);
        assert_eq!(normalize_duration("NaN"), None);
        assert_eq!(normalize_duration(""), None);
    }

    #[test]
    fn current_scene_requires_exact_index_and_name() {
        let id = Uuid::new_v4();
        let config = scene(id, Some(3), "Verse");
        assert!(is_current(
            &config,
            Some(&SceneSummary {
                index: 3,
                name: "Verse".into()
            })
        ));
        assert!(!is_current(
            &config,
            Some(&SceneSummary {
                index: 3,
                name: "Chorus".into()
            })
        ));
    }

    #[test]
    fn duplicate_names_are_case_sensitive_unique_and_sorted() {
        let scenes = vec![
            scene(Uuid::new_v4(), Some(0), "Zulu"),
            scene(Uuid::new_v4(), Some(1), "Alpha"),
            scene(Uuid::new_v4(), Some(2), "Zulu"),
            scene(Uuid::new_v4(), Some(3), "Alpha"),
            scene(Uuid::new_v4(), Some(4), "alpha"),
        ];
        assert_eq!(duplicate_scene_names(&scenes), ["Alpha", "Zulu"]);
    }

    #[test]
    fn aggregate_scope_ignores_duplicate_and_extraneous_pairs() {
        let configs = [
            ChannelConfig {
                group: 0,
                channel: 0,
                fader_db: None,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            },
            ChannelConfig {
                group: 0,
                channel: 1,
                fader_db: None,
                pan: None,
                balance: None,
                width: None,
                pan_mode: None,
            },
        ];
        let scoped = [
            ChannelRef {
                group: 0,
                channel: 0,
            },
            ChannelRef {
                group: 0,
                channel: 0,
            },
            ChannelRef {
                group: 0,
                channel: 1,
            },
            ChannelRef {
                group: 9,
                channel: 9,
            },
        ];
        let pairs: BTreeSet<_> = scoped
            .iter()
            .map(|item| (item.group, item.channel))
            .collect();
        assert!(
            configs
                .iter()
                .all(|config| pairs.contains(&(config.group, config.channel)))
        );
        assert!(!pairs.is_empty());
    }
}
