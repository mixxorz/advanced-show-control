use gpui_kit::base::{Button as BaseButton, FocusTrapElement as _};
use gpui_kit::component::{
    Disableable, IconName, Sizable,
    button::ButtonVariants,
    input::{Input, InputEvent, InputState},
};
use gpui_kit::{
    AppContext, Context, Entity, FocusHandle, Focusable as _, MouseButton, Render, Role,
    SharedString, Subscription, TestSupportExt as _, Window, div, prelude::*, px, rgb,
};
use uuid::Uuid;

use super::{
    CommandDispatcher,
    button::bordered_button,
    scene_library::{
        format_scene_number, scene_library_columns, scene_library_header, scene_library_panel,
        scene_library_row,
    },
    theme,
};
use crate::{
    cue_lists::{CueEntry, CueList},
    projector::AppViewState,
    scenes::SceneConfig,
};

#[derive(Clone)]
struct SceneDrag {
    scene_id: Uuid,
    name: SharedString,
}

impl Render for SceneDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        drag_badge("SCENE", self.name.clone())
    }
}

#[derive(Clone)]
struct CueEntryDrag {
    entry_id: Uuid,
    name: SharedString,
}

impl Render for CueEntryDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        drag_badge("CUE", self.name.clone())
    }
}

#[derive(Clone)]
struct CueListDrag {
    list_id: Uuid,
    name: SharedString,
}

impl Render for CueListDrag {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        drag_badge("LIST", self.name.clone())
    }
}

fn drag_badge(kind: &'static str, name: SharedString) -> impl IntoElement {
    div()
        .flex()
        .gap_2()
        .items_center()
        .px_3()
        .py_2()
        .bg(rgb(theme::CONSOLE_SECTION))
        .border_1()
        .border_color(rgb(theme::ACCENT_ORANGE))
        .text_color(rgb(theme::CONSOLE_PRIMARY))
        .child(
            div()
                .text_xs()
                .text_color(rgb(theme::ACCENT_ORANGE))
                .child(kind),
        )
        .child(name)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum NameEditor {
    Create,
    Rename(Uuid),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ManageCommand {
    Name(NameEditor),
    Delete(Uuid),
    Activate,
    Reorder,
}

pub struct CueListsView {
    snapshot: AppViewState,
    dispatcher: CommandDispatcher,
    selected_entry_id: Option<Uuid>,
    manage_open: bool,
    name_editor: Option<NameEditor>,
    pending_delete: Option<Uuid>,
    pending_manage_command: Option<(u64, ManageCommand)>,
    manage_focus: FocusHandle,
    nested_focus: FocusHandle,
    manage_return_focus: Option<FocusHandle>,
    name_input: Entity<InputState>,
    _name_input_subscription: Subscription,
}

impl CueListsView {
    pub fn new(
        snapshot: AppViewState,
        dispatcher: CommandDispatcher,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx));
        let name_input_subscription =
            cx.subscribe(&name_input, |this, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::PressEnter { .. }) {
                    this.submit_name_editor(cx);
                }
            });
        Self {
            snapshot,
            dispatcher,
            selected_entry_id: None,
            manage_open: false,
            name_editor: None,
            pending_delete: None,
            pending_manage_command: None,
            manage_focus: cx.focus_handle(),
            nested_focus: cx.focus_handle(),
            manage_return_focus: None,
            name_input,
            _name_input_subscription: name_input_subscription,
        }
    }

    pub fn modal_open(&self) -> bool {
        self.manage_open
    }

    pub fn dismiss_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.manage_open {
            return false;
        }
        if self.pending_manage_command.is_some() {
            return true;
        }
        if self.pending_delete.take().is_some() || self.name_editor.take().is_some() {
            self.manage_focus.focus(window, cx);
            cx.notify();
            return true;
        }
        self.close_manager(window, cx);
        true
    }

    fn close_manager(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.manage_open = false;
        if let Some(focus) = self.manage_return_focus.take() {
            focus.focus(window, cx);
        } else {
            window.blur(cx);
        }
        cx.notify();
    }

    pub fn set_snapshot(
        &mut self,
        snapshot: AppViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let nested_editor_was_open = self.pending_delete.is_some() || self.name_editor.is_some();
        self.snapshot = snapshot;
        if self
            .selected_entry_id
            .is_some_and(|id| !self.active_entries().iter().any(|entry| entry.id == id))
        {
            self.selected_entry_id = None;
        }
        if self
            .pending_delete
            .is_some_and(|id| !self.snapshot.cue_lists.iter().any(|list| list.id == id))
        {
            self.pending_delete = None;
        }
        if matches!(self.name_editor, Some(NameEditor::Rename(id)) if !self.snapshot.cue_lists.iter().any(|list| list.id == id))
        {
            self.name_editor = None;
        }
        if self.manage_open
            && nested_editor_was_open
            && self.pending_delete.is_none()
            && self.name_editor.is_none()
        {
            self.manage_focus.focus(window, cx);
        }
        cx.notify();
    }

    fn manager_controls_inert(&self) -> bool {
        manager_controls_inert(
            self.name_editor,
            self.pending_delete,
            self.pending_manage_command,
        )
    }

    fn active_list(&self) -> Option<&CueList> {
        active_cue_list(&self.snapshot)
    }

    fn active_entries(&self) -> &[CueEntry] {
        self.active_list()
            .map_or(&[], |list| list.entries.as_slice())
    }

    fn dispatch(
        &self,
        command: impl FnOnce(crate::application::ApplicationCommandContext) -> CommandFuture
        + Send
        + 'static,
    ) -> u64 {
        self.dispatcher.dispatch(command)
    }

    pub fn command_finished(
        &mut self,
        command_id: u64,
        failed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((pending_id, command)) = self.pending_manage_command else {
            return;
        };
        if pending_id != command_id {
            return;
        }
        self.pending_manage_command = None;
        if !failed {
            match command {
                ManageCommand::Name(editor) if self.name_editor == Some(editor) => {
                    self.name_editor = None;
                    self.manage_focus.focus(window, cx);
                }
                ManageCommand::Delete(id) if self.pending_delete == Some(id) => {
                    self.pending_delete = None;
                    self.manage_focus.focus(window, cx);
                }
                ManageCommand::Activate => {
                    self.close_manager(window, cx);
                }
                ManageCommand::Reorder => {}
                _ => {}
            }
        }
        cx.notify();
    }

    fn submit_name_editor(&mut self, cx: &mut Context<Self>) {
        if self.pending_manage_command.is_some() {
            return;
        }
        let Some(editor) = self.name_editor else {
            return;
        };
        let name = self.name_input.read(cx).value().to_string();
        let command_id = match editor {
            NameEditor::Create => self.dispatch(move |commands| {
                Box::pin(async move { commands.create_cue_list(name).await.map(|_| ()) })
            }),
            NameEditor::Rename(id) => self.dispatch(move |commands| {
                Box::pin(async move { commands.rename_cue_list(id, name).await.map(|_| ()) })
            }),
        };
        self.pending_manage_command = Some((command_id, ManageCommand::Name(editor)));
        cx.notify();
    }

    pub fn cue_selected(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(entry_id) = self.selected_entry_id else {
            return false;
        };
        if !self
            .active_entries()
            .iter()
            .any(|entry| entry.id == entry_id)
        {
            self.selected_entry_id = None;
            cx.notify();
            return false;
        }
        self.cue(entry_id, cx);
        true
    }

    fn cue(&mut self, entry_id: Uuid, cx: &mut Context<Self>) {
        if !self
            .active_entries()
            .iter()
            .any(|entry| entry.id == entry_id)
        {
            return;
        }
        self.dispatch(move |commands| {
            Box::pin(async move { commands.cue_entry(Some(entry_id)).await.map(|_| ()) })
        });
        self.selected_entry_id = None;
        cx.notify();
    }

    fn render_scene_library(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let scenes = self.snapshot.scene_configs.clone();
        let recall_scene_id = selected_scene_config(&self.snapshot)
            .filter(|scene| scene.scene_index.is_some())
            .map(|scene| scene.internal_scene_id);
        let recall = bordered_button("cue-scene-library-recall")
            .label("RECALL")
            .primary()
            .disabled(recall_scene_id.is_none())
            .on_click(cx.listener(move |this, _, _, _| {
                if let Some(scene_id) = recall_scene_id {
                    this.dispatch(move |commands| {
                        Box::pin(async move { commands.recall_scene(scene_id).await.map(|_| ()) })
                    });
                }
            }));

        scene_library_panel()
            .child(scene_library_header(recall))
            .child(scene_library_columns())
            .child(
                div()
                    .id("cue-scene-library")
                    .flex_1()
                    .overflow_y_scroll()
                    .children(
                        scenes
                            .into_iter()
                            .map(|scene| self.render_scene_row(scene, cx)),
                    ),
            )
    }

    fn render_scene_row(&self, scene: SceneConfig, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let current = self.snapshot.current_scene.as_ref().is_some_and(|current| {
            scene.scene_index == Some(current.index) && scene.scene_name == current.name
        });
        let scene_id = scene.internal_scene_id;
        let selected = self.snapshot.selected_scene_internal_id.as_deref()
            == Some(scene_id.to_string().as_str());
        let name: SharedString = scene.scene_name.clone().into();
        let selection_label = if selected {
            "Selected scene"
        } else {
            "Select scene"
        };
        scene_library_row(
            SharedString::from(format!("cue-scene-{scene_id}")),
            &scene,
            if current {
                Some(theme::STATUS_CURRENT)
            } else if selected {
                Some(theme::ACCENT_ORANGE)
            } else {
                None
            },
        )
        .accessibility_label(format!(
            "{selection_label} {} {}",
            format_scene_number(scene.scene_index),
            scene.scene_name
        ))
        .selected(selected)
        .when(selected, |row| row.bg(rgb(theme::CONSOLE_CONTROL)))
        .hover(|style| style.bg(rgb(theme::CONSOLE_CONTROL_HOVER)))
        .on_click(cx.listener(move |this, _, _, _| {
            this.dispatch(move |commands| {
                Box::pin(async move { commands.select_scene_config(scene_id).await.map(|_| ()) })
            });
        }))
        .on_drag(SceneDrag { scene_id, name }, |payload, _, _, cx| {
            cx.new(|_| payload.clone())
        })
        .into_any_element()
    }

    fn render_cue_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let list = self.active_list().cloned();
        let title = list.as_ref().map_or_else(
            || "NO ACTIVE CUE LIST".to_string(),
            |list| list.name.clone(),
        );
        let selected = self.selected_entry_id.filter(|id| {
            list.as_ref()
                .is_some_and(|list| list.entries.iter().any(|entry| entry.id == *id))
        });
        let entity = cx.entity();
        let cue_button = bordered_button("cue-selected")
            .small()
            .label("CUE")
            .disabled(selected.is_none())
            .on_click(move |_, _, cx| {
                if let Some(id) = selected {
                    entity.update(cx, |this, cx| this.cue(id, cx));
                }
            });
        let entity = cx.entity();
        let manage = bordered_button("manage-cue-lists")
            .small()
            .label("MANAGE CUE LISTS")
            .on_click(move |_, window, cx| {
                entity.update(cx, |this, cx| {
                    this.manage_return_focus = window.focused(cx);
                    this.manage_open = true;
                    this.manage_focus.focus(window, cx);
                    cx.notify();
                });
            });

        div()
            .h_full()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .bg(rgb(theme::CONSOLE_PANEL))
            .border_1()
            .border_color(rgb(theme::CONSOLE_LINE))
            .child(
                div()
                    .h(px(54.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_4()
                    .border_b_1()
                    .border_color(rgb(theme::CONSOLE_LINE))
                    .child(
                        div()
                            .font_family("Fira Code")
                            .text_lg()
                            .text_color(rgb(theme::ACCENT_ORANGE))
                            .child(title),
                    )
                    .child(div().flex().gap_2().child(cue_button).child(manage)),
            )
            .child(
                div()
                    .flex()
                    .pr_3()
                    .py_2()
                    .border_b_1()
                    .border_color(rgb(theme::CONSOLE_LINE_SOFT))
                    .text_xs()
                    .text_color(rgb(theme::CONSOLE_SECONDARY))
                    .child(div().w(px(25.)))
                    .child(div().flex_1().child("SCENE NAME"))
                    .child(div().w(px(54.)).text_right().child("#"))
                    .child(div().w(px(28.))),
            )
            .child(self.render_active_entries(list, cx))
    }

    fn render_active_entries(
        &self,
        list: Option<CueList>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let append = div()
            .id("cue-entry-append-drop")
            .h(px(42.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgb(theme::CONSOLE_LINE_SOFT))
            .text_color(rgb(theme::CONSOLE_MUTED))
            .child("DROP SCENE HERE TO APPEND")
            .on_drop(move |payload: &SceneDrag, _, cx| {
                let scene_id = payload.scene_id;
                entity.update(cx, |this, cx| {
                    let index = this.active_entries().len();
                    this.dispatch(move |commands| {
                        Box::pin(async move {
                            commands
                                .add_scene_to_active_cue_list(scene_id, index)
                                .await
                                .map(|_| ())
                        })
                    });
                    cx.notify();
                });
            });

        let Some(list) = list else {
            return div()
                .flex_1()
                .p_4()
                .text_color(rgb(theme::CONSOLE_SECONDARY))
                .child("No active cue list.")
                .into_any_element();
        };
        div()
            .id("active-cue-entries")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(
                list.entries
                    .into_iter()
                    .enumerate()
                    .map(|(index, entry)| self.render_entry_row(entry, index, cx)),
            )
            .child(append)
            .into_any_element()
    }

    fn render_entry_row(
        &self,
        entry: CueEntry,
        index: usize,
        cx: &mut Context<Self>,
    ) -> gpui_kit::AnyElement {
        let scene = scene_by_id(&self.snapshot, entry.scene_internal_id);
        let missing = scene.is_none();
        let current = scene.is_some_and(|scene| {
            self.snapshot.current_scene.as_ref().is_some_and(|current| {
                scene.scene_index == Some(current.index) && scene.scene_name == current.name
            })
        });
        let cued =
            self.snapshot.cued_cue_entry_id.as_deref() == Some(entry.id.to_string().as_str());
        let selected = self.selected_entry_id == Some(entry.id);
        let scene_name = scene.map_or("Missing scene", |scene| scene.scene_name.as_str());
        let display_name: SharedString = scene_name.to_string().into();
        let color = if current {
            theme::STATUS_CURRENT
        } else if cued {
            theme::STATUS_CUED
        } else if missing {
            theme::STATUS_WARNING
        } else {
            theme::CONSOLE_PRIMARY
        };
        let left_border = if current {
            theme::STATUS_CURRENT
        } else if cued {
            theme::STATUS_CUED
        } else if selected {
            theme::ACCENT_ORANGE
        } else if missing {
            theme::STATUS_WARNING
        } else {
            theme::CONSOLE_PANEL
        };
        let entry_id = entry.id;
        let drag_name = display_name.clone();
        let select_entity = cx.entity();
        let drop_entity = cx.entity();
        let scene_drop_entity = cx.entity();
        let remove_entity = cx.entity();
        let cue_number = index + 1;
        let select_label = format!("Select cue {cue_number}: {display_name}");
        let remove_label = format!("Remove cue {cue_number}: {display_name}");

        div()
            .id(format!("cue-entry-row-{entry_id}"))
            .h(px(46.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgb(theme::CONSOLE_LINE_SOFT))
            .bg(rgb(if selected {
                theme::CONSOLE_CONTROL
            } else {
                theme::CONSOLE_PANEL
            }))
            .hover(|style| style.bg(rgb(theme::CONSOLE_SECTION)))
            .on_drag(
                CueEntryDrag {
                    entry_id,
                    name: drag_name,
                },
                |payload, _, _, cx| cx.new(|_| payload.clone()),
            )
            .on_drop(move |payload: &CueEntryDrag, _, cx| {
                let from = payload.entry_id;
                drop_entity.update(cx, |this, cx| {
                    if let Some(ids) = reordered_ids(this.active_entries(), from, entry_id) {
                        this.dispatch(move |commands| {
                            Box::pin(
                                async move { commands.reorder_cue_entries(ids).await.map(|_| ()) },
                            )
                        });
                    }
                    cx.notify();
                });
            })
            .on_drop(move |payload: &SceneDrag, _, cx| {
                let scene_id = payload.scene_id;
                scene_drop_entity.update(cx, |this, cx| {
                    if this
                        .active_entries()
                        .get(index)
                        .is_some_and(|entry| entry.id == entry_id)
                    {
                        this.dispatch(move |commands| {
                            Box::pin(async move {
                                commands
                                    .add_scene_to_active_cue_list(scene_id, index)
                                    .await
                                    .map(|_| ())
                            })
                        });
                    }
                    cx.notify();
                });
            })
            .child(div().w(px(3.)).h_full().bg(rgb(left_border)))
            .child(
                BaseButton::new(format!("select-cue-entry-{entry_id}"))
                    .accessibility_label(select_label)
                    .h_full()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .on_click(move |event, _, cx| {
                        select_entity.update(cx, |this, cx| {
                            if event.click_count() >= 2 {
                                this.cue(entry_id, cx);
                            } else {
                                this.selected_entry_id = Some(entry_id);
                                cx.notify();
                            }
                        });
                    })
                    .child(
                        div()
                            .w(px(22.))
                            .text_color(rgb(color))
                            .child(div().ml(px(-2.)).child(
                                if current || cued || selected || missing {
                                    "▶"
                                } else {
                                    ""
                                },
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_color(rgb(color))
                            .child(display_name),
                    )
                    .child(
                        div()
                            .w(px(54.))
                            .text_right()
                            .font_family("Fira Code")
                            .text_color(rgb(color))
                            .child(format_scene_number(
                                scene.and_then(|scene| scene.scene_index),
                            )),
                    ),
            )
            .child(
                bordered_button(format!("remove-cue-entry-{entry_id}"))
                    .small()
                    .ml_2()
                    .mr_2()
                    .danger()
                    .icon(IconName::Delete)
                    .accessibility_label(remove_label)
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        remove_entity.update(cx, |this, cx| {
                            this.dispatch(move |commands| {
                                Box::pin(async move {
                                    commands.remove_cue_entry(entry_id).await.map(|_| ())
                                })
                            });
                            if this.selected_entry_id == Some(entry_id) {
                                this.selected_entry_id = None;
                            }
                            cx.notify();
                        });
                    }),
            )
            .into_any_element()
    }

    /// @cc [owner:mixxorz,label:accessibility] cue-manager-modal-focus
    /// While open, the cue manager MUST expose a named dialog, own keyboard focus, and trap focus
    /// within the manager until dismissed.
    fn render_manage_overlay(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let manager_inert = self.manager_controls_inert();
        let entity = cx.entity();
        let close = bordered_button("close-cue-manager")
            .small()
            .label("CLOSE")
            .disabled(manager_inert)
            .on_click(move |_, window, cx| {
                entity.update(cx, |this, cx| {
                    if this.manager_controls_inert() {
                        return;
                    }
                    this.close_manager(window, cx);
                });
            });
        let entity = cx.entity();
        let create = bordered_button("new-cue-list")
            .small()
            .primary()
            .label("NEW CUE LIST")
            .disabled(manager_inert)
            .on_click(move |_, window, cx| {
                entity.update(cx, |this, cx| {
                    if this.manager_controls_inert() {
                        return;
                    }
                    this.name_input
                        .update(cx, |input, cx| input.set_value("", window, cx));
                    this.name_editor = Some(NameEditor::Create);
                    this.name_input.read(cx).focus_handle(cx).focus(window, cx);
                    cx.notify();
                });
            });

        div()
            .id("cue-list-manager-overlay")
            .role(Role::Dialog)
            .aria_label("Cue list manager")
            .test_support()
            .absolute()
            .inset_0()
            .bg(rgb(0x000000).opacity(0.82))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_key_down(
                cx.listener(|this, event: &gpui_kit::KeyDownEvent, window, cx| {
                    if super::keyboard::normalized_physical_key(&event.keystroke) == "Escape" {
                        this.dismiss_modal(window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .focus_trap("cue-list-manager", &self.manage_focus)
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .id("cue-list-manager")
                    .w(px(620.))
                    .max_h(px(620.))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .p_5()
                    .bg(rgb(theme::CONSOLE_PANEL))
                    .border_1()
                    .border_color(rgb(theme::CONSOLE_LINE_STRONG))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_lg()
                                    .text_color(rgb(theme::ACCENT_ORANGE))
                                    .child("CUE LISTS"),
                            )
                            .child(div().flex().gap_2().child(create).child(close)),
                    )
                    .child(
                        div()
                            .id("cue-list-manager-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .children(
                                self.snapshot
                                    .cue_lists
                                    .clone()
                                    .into_iter()
                                    .map(|list| self.render_manage_row(list, cx)),
                            ),
                    )
                    .when_some(self.name_editor, |panel, editor| {
                        panel.child(self.render_name_editor(editor, cx))
                    })
                    .when_some(self.pending_delete, |panel, id| {
                        panel.child(self.render_delete_confirmation(id, cx))
                    }),
            )
    }

    fn render_manage_row(&self, list: CueList, cx: &mut Context<Self>) -> gpui_kit::AnyElement {
        let manager_inert = self.manager_controls_inert();
        let active =
            self.snapshot.active_cue_list_id.as_deref() == Some(list.id.to_string().as_str());
        let id = list.id;
        let name: SharedString = list.name.clone().into();
        let drag_name = name.clone();
        let rename_label = format!("Rename cue list {name}");
        let delete_label = format!("Delete cue list {name}");
        let select_entity = cx.entity();
        let rename_entity = cx.entity();
        let delete_entity = cx.entity();
        let drop_entity = cx.entity();
        div()
            .id(format!("cue-list-row-{id}"))
            .flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_2()
            .mb_2()
            .bg(rgb(theme::CONSOLE_SECTION))
            .border_1()
            .border_color(rgb(if active {
                theme::ACCENT_ORANGE
            } else {
                theme::CONSOLE_LINE
            }))
            .when(!manager_inert, |row| {
                row.on_drop(move |payload: &CueListDrag, _, cx| {
                    let from = payload.list_id;
                    drop_entity.update(cx, |this, cx| {
                        if this.manager_controls_inert() {
                            return;
                        }
                        if let Some(ids) = reordered_ids(&this.snapshot.cue_lists, from, id) {
                            let command_id = this.dispatch(move |commands| {
                                Box::pin(async move {
                                    commands.reorder_cue_lists(ids).await.map(|_| ())
                                })
                            });
                            this.pending_manage_command =
                                Some((command_id, ManageCommand::Reorder));
                        }
                        cx.notify();
                    });
                })
            })
            .child(
                div()
                    .id(format!("drag-cue-list-{id}"))
                    .test_support()
                    .w(px(22.))
                    .cursor_pointer()
                    .text_color(rgb(theme::CONSOLE_MUTED))
                    .when(!manager_inert, |handle| {
                        handle.on_drag(
                            CueListDrag {
                                list_id: id,
                                name: drag_name,
                            },
                            |payload, _, _, cx| cx.new(|_| payload.clone()),
                        )
                    })
                    .child("↕"),
            )
            .child(
                BaseButton::new(format!("activate-cue-list-{id}"))
                    .accessibility_label(format!("Activate cue list {name}"))
                    .disabled(manager_inert)
                    .flex_1()
                    .text_color(rgb(theme::CONSOLE_PRIMARY))
                    .child(name)
                    .on_click(move |_, window, cx| {
                        select_entity.update(cx, |this, cx| {
                            if this.manager_controls_inert() {
                                return;
                            }
                            if this.snapshot.active_cue_list_id.as_deref()
                                != Some(id.to_string().as_str())
                            {
                                let command_id = this.dispatch(move |commands| {
                                    Box::pin(async move {
                                        commands.set_active_cue_list(Some(id)).await.map(|_| ())
                                    })
                                });
                                this.pending_manage_command =
                                    Some((command_id, ManageCommand::Activate));
                            } else {
                                this.close_manager(window, cx);
                            }
                            cx.notify();
                        });
                    }),
            )
            .child(
                bordered_button(format!("rename-cue-list-{id}"))
                    .small()
                    .label("RENAME")
                    .accessibility_label(rename_label)
                    .disabled(manager_inert)
                    .on_click(move |_, window, cx| {
                        rename_entity.update(cx, |this, cx| {
                            if this.manager_controls_inert() {
                                return;
                            }
                            let value = this
                                .snapshot
                                .cue_lists
                                .iter()
                                .find(|list| list.id == id)
                                .map_or("", |list| list.name.as_str());
                            this.name_input
                                .update(cx, |input, cx| input.set_value(value, window, cx));
                            this.name_editor = Some(NameEditor::Rename(id));
                            this.pending_delete = None;
                            this.name_input.read(cx).focus_handle(cx).focus(window, cx);
                            cx.notify();
                        });
                    }),
            )
            .child(
                bordered_button(format!("delete-cue-list-{id}"))
                    .small()
                    .danger()
                    .label("DELETE")
                    .accessibility_label(delete_label)
                    .disabled(manager_inert)
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        delete_entity.update(cx, |this, cx| {
                            if this.manager_controls_inert() {
                                return;
                            }
                            this.pending_delete = Some(id);
                            this.name_editor = None;
                            this.nested_focus.focus(window, cx);
                            cx.notify();
                        });
                    }),
            )
            .into_any_element()
    }

    fn render_name_editor(&self, editor: NameEditor, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let cancel_entity = cx.entity();
        let submitting = matches!(
            self.pending_manage_command,
            Some((_, ManageCommand::Name(pending))) if pending == editor
        );
        let title = match editor {
            NameEditor::Create => "NEW CUE LIST",
            NameEditor::Rename(_) => "RENAME CUE LIST",
        };
        div()
            .id("cue-list-name-editor")
            .role(Role::Dialog)
            .aria_label(title)
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .bg(rgb(theme::CONSOLE_CHROME))
            .border_1()
            .border_color(rgb(theme::ACCENT_ORANGE))
            .child(div().text_color(rgb(theme::ACCENT_ORANGE)).child(title))
            .child(Input::new(&self.name_input).id("cue-list-name"))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        bordered_button("cancel-cue-list-name")
                            .small()
                            .label("CANCEL")
                            .disabled(submitting)
                            .on_click(move |_, window, cx| {
                                cancel_entity.update(cx, |this, cx| {
                                    this.name_editor = None;
                                    this.manage_focus.focus(window, cx);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        bordered_button("submit-cue-list-name")
                            .small()
                            .primary()
                            .label("SAVE")
                            .disabled(submitting)
                            .on_click(move |_, _, cx| {
                                entity.update(cx, |this, cx| this.submit_name_editor(cx));
                            }),
                    ),
            )
    }

    fn render_delete_confirmation(&self, id: Uuid, cx: &mut Context<Self>) -> impl IntoElement {
        let name = self
            .snapshot
            .cue_lists
            .iter()
            .find(|list| list.id == id)
            .map_or("this cue list", |list| list.name.as_str())
            .to_string();
        let cancel_entity = cx.entity();
        let delete_entity = cx.entity();
        let submitting = matches!(
            self.pending_manage_command,
            Some((_, ManageCommand::Delete(pending))) if pending == id
        );
        div()
            .id("delete-cue-list-confirmation")
            .role(Role::Dialog)
            .aria_label("Delete cue list confirmation")
            .test_support()
            .track_focus(&self.nested_focus)
            .p_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_3()
            .bg(rgb(theme::CONSOLE_CHROME))
            .border_1()
            .border_color(rgb(theme::STATUS_DANGER))
            .child(format!(
                "Delete {name}? This only removes the app-managed cue list."
            ))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        bordered_button("cancel-delete-cue-list")
                            .small()
                            .label("CANCEL")
                            .disabled(submitting)
                            .on_click(move |_, window, cx| {
                                cancel_entity.update(cx, |this, cx| {
                                    this.pending_delete = None;
                                    this.manage_focus.focus(window, cx);
                                    cx.notify();
                                });
                            }),
                    )
                    .child(
                        bordered_button("confirm-delete-cue-list")
                            .small()
                            .danger()
                            .label("DELETE")
                            .disabled(submitting)
                            .on_click(move |_, _, cx| {
                                delete_entity.update(cx, |this, cx| {
                                    if this.pending_manage_command.is_some() {
                                        return;
                                    }
                                    if this.snapshot.cue_lists.iter().any(|list| list.id == id) {
                                        let command_id = this.dispatch(move |commands| {
                                            Box::pin(async move {
                                                commands.delete_cue_list(id).await.map(|_| ())
                                            })
                                        });
                                        this.pending_manage_command =
                                            Some((command_id, ManageCommand::Delete(id)));
                                        cx.notify();
                                    }
                                });
                            }),
                    ),
            )
    }
}

type CommandFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send>>;

impl Render for CueListsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .gap_3()
            .bg(rgb(theme::CONSOLE_BG))
            .text_color(rgb(theme::CONSOLE_PRIMARY))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap_3()
                    .child(self.render_scene_library(cx))
                    .child(self.render_cue_pane(cx)),
            )
            .when(self.manage_open, |root| {
                root.child(self.render_manage_overlay(cx))
            })
    }
}

fn manager_controls_inert(
    name_editor: Option<NameEditor>,
    pending_delete: Option<Uuid>,
    pending_command: Option<(u64, ManageCommand)>,
) -> bool {
    name_editor.is_some() || pending_delete.is_some() || pending_command.is_some()
}

fn active_cue_list(snapshot: &AppViewState) -> Option<&CueList> {
    let active = snapshot.active_cue_list_id.as_deref()?;
    snapshot
        .cue_lists
        .iter()
        .find(|list| list.id.to_string() == active)
}

fn selected_scene_config(snapshot: &AppViewState) -> Option<&SceneConfig> {
    let selected_id = snapshot.selected_scene_internal_id.as_deref()?;
    snapshot
        .scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id.to_string() == selected_id)
}

fn scene_by_id(snapshot: &AppViewState, scene_id: Uuid) -> Option<&SceneConfig> {
    snapshot
        .scene_configs
        .iter()
        .find(|scene| scene.internal_scene_id == scene_id)
}

#[cfg(test)]
fn valid_cued_entry(snapshot: &AppViewState) -> Option<&CueEntry> {
    let cued = snapshot.cued_cue_entry_id.as_deref()?;
    let entry = active_cue_list(snapshot)?
        .entries
        .iter()
        .find(|entry| entry.id.to_string() == cued)?;
    scene_by_id(snapshot, entry.scene_internal_id)?;
    Some(entry)
}

trait StableId {
    fn stable_id(&self) -> Uuid;
}

impl StableId for CueEntry {
    fn stable_id(&self) -> Uuid {
        self.id
    }
}

impl StableId for CueList {
    fn stable_id(&self) -> Uuid {
        self.id
    }
}

fn reordered_ids<T: StableId>(items: &[T], from: Uuid, to: Uuid) -> Option<Vec<Uuid>> {
    if from == to {
        return None;
    }
    let from_index = items.iter().position(|item| item.stable_id() == from)?;
    let to_index = items.iter().position(|item| item.stable_id() == to)?;
    let mut ids: Vec<_> = items.iter().map(StableId::stable_id).collect();
    let moved = ids.remove(from_index);
    ids.insert(to_index, moved);
    Some(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn entry(entry: u128, scene: u128) -> CueEntry {
        CueEntry {
            id: id(entry),
            scene_internal_id: id(scene),
        }
    }

    #[test]
    fn reorder_is_a_complete_stable_permutation() {
        let entries = vec![entry(1, 11), entry(2, 12), entry(3, 13)];
        assert_eq!(
            reordered_ids(&entries, id(1), id(3)),
            Some(vec![id(2), id(3), id(1)])
        );
        assert_eq!(reordered_ids(&entries, id(2), id(2)), None);
        assert_eq!(reordered_ids(&entries, id(99), id(2)), None);
        assert_eq!(reordered_ids(&entries, id(1), id(99)), None);
    }

    #[test]
    fn active_and_cued_lookup_requires_exact_nested_ids_and_a_scene_config() {
        let list_id = id(1);
        let cue = entry(2, 3);
        let mut snapshot = AppViewState {
            cue_lists: vec![CueList {
                id: list_id,
                name: "Main".into(),
                entries: vec![cue.clone()],
            }],
            active_cue_list_id: Some(list_id.to_string()),
            cued_cue_entry_id: Some(cue.id.to_string()),
            ..Default::default()
        };
        assert!(valid_cued_entry(&snapshot).is_none());
        snapshot.scene_configs.push(SceneConfig {
            internal_scene_id: cue.scene_internal_id,
            scene_index: Some(0),
            scene_name: "Opening".into(),
            duration_ms: 0,
            channel_configs: vec![],
            scoped_channels: vec![],
            scope_toggles: Default::default(),
        });
        assert_eq!(
            valid_cued_entry(&snapshot).map(|entry| entry.id),
            Some(cue.id)
        );
        snapshot.active_cue_list_id = Some(id(99).to_string());
        assert!(valid_cued_entry(&snapshot).is_none());
    }

    #[test]
    fn manager_controls_are_inert_for_nested_ui_or_pending_commands() {
        assert!(!manager_controls_inert(None, None, None));
        assert!(manager_controls_inert(Some(NameEditor::Create), None, None));
        assert!(manager_controls_inert(None, Some(id(1)), None));
        assert!(manager_controls_inert(
            None,
            None,
            Some((7, ManageCommand::Delete(id(1))))
        ));
    }

    #[test]
    fn scene_numbers_match_console_one_based_format() {
        assert_eq!(format_scene_number(None), "---");
        assert_eq!(format_scene_number(Some(0)), "001");
        assert_eq!(format_scene_number(Some(11)), "012");
    }
}
